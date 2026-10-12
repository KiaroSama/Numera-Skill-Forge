#!/usr/bin/env python3
"""Original macOS release UI/IPC acceptance, restricted to ephemeral hosted Mac CI."""

import argparse
import ctypes
from datetime import datetime, timezone
import errno
import hashlib
import json
import logging
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import time
import uuid

LIMIT = 1024 * 1024


class BsdInfo(ctypes.Structure):
    # Public Darwin proc_bsdinfo, PROC_PIDTBSDINFO=3; start identity includes microseconds.
    _fields_ = [(name, ctypes.c_uint32) for name in (
        "flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid", "rgid", "svuid", "svgid", "reserved")]
    _fields_ += [("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)]
    _fields_ += [(name, ctypes.c_uint32) for name in ("nfiles", "pgid", "pjobc", "tdev", "tpgid")]
    _fields_ += [("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64), ("start_usec", ctypes.c_uint64)]


class Owner:
    def __init__(self, deadline):
        self.deadline = deadline
        self.lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
        self.lib.proc_pidinfo.restype = ctypes.c_int
        self.lib.proc_listpids.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int]
        self.lib.proc_listpids.restype = ctypes.c_int
        self.roots = []
        self.owned = {}
        self.last_sweep = 0.0
        self.cleanup_errors = []

    def info(self, pid):
        info = BsdInfo()
        ctypes.set_errno(0)
        # Nonzero arg includes zombies, retaining their PID/start identity until reaped.
        size = self.lib.proc_pidinfo(pid, 3, 1, ctypes.byref(info), ctypes.sizeof(info))
        error = ctypes.get_errno()
        if size == 0:
            if error == errno.ESRCH:
                return None
            raise RuntimeError(f"Cannot verify process identity pid={pid} errno={error} expected={self.owned.get(pid)}")
        if size != ctypes.sizeof(info) or info.pid != pid:
            raise RuntimeError("Darwin process identity ABI did not match")
        return (info.start_sec, info.start_usec), info.ppid, info.pgid, info.status

    def sweep(self, force=False):
        if not force and time.monotonic() - self.last_sweep < 0.2:
            return
        self.last_sweep = time.monotonic()
        pids = (ctypes.c_int * 8192)()
        count = self.lib.proc_listpids(4, os.getuid(), pids, ctypes.sizeof(pids))
        if count <= 0 or count >= ctypes.sizeof(pids) or count % ctypes.sizeof(ctypes.c_int):
            raise RuntimeError("Process ownership inventory incomplete or exceeds8192 PIDs")
        rows = {}
        # PROC_UID_ONLY excludes unrelated privileged/system processes; never inspect their source/config.
        for pid in pids[:count // ctypes.sizeof(ctypes.c_int)]:
            if pid > 0:
                row = self.info(pid)
                if row:
                    rows[pid] = row
        known = {pid for pid, identity in self.owned.items() if rows.get(pid, (None,))[0] == identity}
        root_groups = {child.pid for child in self.roots if child.pid in self.owned}
        groups = {rows[pid][2] for pid in known if rows[pid][2] in root_groups}
        while True:
            found = {pid for pid, row in rows.items() if pid not in known and pid != os.getpid() and
                     ((row[1] in known and row[0] >= self.owned[row[1]]) or
                      any(row[2] == group and row[0] >= self.owned[group] for group in groups))}
            if not found:
                break
            for pid in found:
                self.owned[pid] = rows[pid][0]
            known.update(found)

    def spawn(self, arguments, cwd, env, output=None):
        child = subprocess.Popen(arguments, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                 stdout=output or subprocess.PIPE, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        self.roots.append(child)
        try:
            identity = self.info(child.pid)
            if identity:
                self.owned[child.pid] = identity[0]
            self.sweep(force=True)
        except BaseException:
            if child.poll() is None:
                child.kill()  # Unreaped direct-child identity remains owned even if proc_pidinfo failed.
            child.wait(timeout=2)
            if child.stdout and output is None:
                child.stdout.close()
            raise
        return child

    def captured(self, arguments, cwd, env):
        child = self.spawn(arguments, cwd, env)
        output = bytearray()
        until = min(self.deadline, time.monotonic() + 10)
        try:
            with selectors.DefaultSelector() as watcher:
                os.set_blocking(child.stdout.fileno(), False)
                watcher.register(child.stdout, selectors.EVENT_READ)
                while watcher.get_map():
                    self.sweep()
                    if time.monotonic() >= until:
                        raise TimeoutError("Native helper exceeded10s call bound")
                    for key, _ in watcher.select(min(0.1, max(0, until - time.monotonic()))):
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            watcher.unregister(key.fileobj)
                        else:
                            output.extend(chunk)
                            if len(output) > LIMIT:
                                raise RuntimeError("Native helper output exceeded1MiB")
                code = child.wait(timeout=max(0.01, until - time.monotonic()))
            if code != 0:
                logging.error("Helper exited=%d diagnostic=%s", code, output.decode("utf-8")[:3000])
                raise RuntimeError(f"Native helper failed exit={code}; see captured diagnostics")
            return bytes(output)
        finally:
            child.stdout.close()
            if child.poll() is None:
                self.stop(child)

    def signal_owned(self, sig, errors, child=None):
        roots = [child] if child else list(reversed(self.roots))
        for process in roots:
            try:
                current = self.info(process.pid)
                if current and current[0] == self.owned.get(process.pid):
                    os.killpg(process.pid, sig)
                elif process.pid not in self.owned and process.poll() is None:
                    os.kill(process.pid, sig)
            except ProcessLookupError:
                pass
            except Exception as error:
                errors.append(error)
                # An unreaped direct child cannot have its PID reused. Group identity is unknown.
                if process.poll() is None:
                    try:
                        os.kill(process.pid, sig)
                    except ProcessLookupError:
                        pass
                    except OSError as kill_error:
                        errors.append(kill_error)
        if child is None:
            for pid, identity in self.owned.items():
                try:
                    current = self.info(pid)
                    if current and current[0] == identity and current[3] != 5:
                        os.kill(pid, sig)
                except ProcessLookupError:
                    pass
                except Exception as error:
                    errors.append(error)  # Never kill an unverifiable descendant; continue others.

    def stop(self, child):
        errors = []
        try:
            self.sweep(force=True)
        except Exception as error:
            errors.append(error)
        for sig in (signal.SIGTERM, signal.SIGKILL):
            self.signal_owned(sig, errors, child)
            try:
                child.wait(timeout=1)
                break
            except subprocess.TimeoutExpired:
                continue
        if errors:
            self.cleanup_errors.extend(errors)
            logging.error("Helper-stop ownership errors=%d; final cleanup still required", len(errors))

    def cleanup(self):
        errors = list(self.cleanup_errors)
        try:
            self.sweep(force=True)
        except Exception as error:
            errors.append(error)
            logging.exception("Final ownership inventory failed; still terminating proven owned PIDs")
        for sig in (signal.SIGTERM, signal.SIGKILL):
            self.signal_owned(sig, errors)
            until = time.monotonic() + 2
            while time.monotonic() < until:
                for child in self.roots:
                    child.poll()
                try:
                    if not self.survivors():
                        break
                except Exception as error:
                    errors.append(error)
                    break
                time.sleep(0.05)  # Bounded identity-based process-exit readiness.
        for child in self.roots:
            try:
                child.wait(timeout=1)
            except subprocess.TimeoutExpired as error:
                errors.append(error)
        alive = []
        for pid, identity in self.owned.items():
            try:
                current = self.info(pid)
                if current and current[0] == identity and current[3] != 5:
                    alive.append(pid)
            except Exception as error:
                errors.append(error)
        if alive or errors:
            raise RuntimeError(f"Owned cleanup incomplete survivors={alive} identity_errors={len(errors)}")
        logging.info("PID/start-identity sweep confirmed owned survivors=0 count=%d", len(self.owned))

    def survivors(self):
        return [pid for pid, identity in self.owned.items()
                if (current := self.info(pid)) and current[0] == identity and current[3] != 5]


def bounded_text(path):
    if not path.is_file() or path.is_symlink() or path.stat().st_size > LIMIT:
        raise RuntimeError("Evidence file is absent, linked or exceeds1MiB")
    return path.read_text(encoding="utf-8")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class AX:
    def __init__(self, owner, helper, run, env, application_output):
        self.owner, self.helper, self.run, self.env = owner, helper, run, env
        self.application_output = application_output
        self.process = None
        self.pid = None

    def call(self, operation, *arguments):
        require(time.monotonic() < self.owner.deadline, "Native smoke exceeded120s wall bound")
        if self.process:
            require(self.process.poll() is None, "Original release exited before acceptance completed")
            require(self.application_output.stat().st_size <= LIMIT, "Original application output exceeded1MiB")
        command = ["/usr/bin/osascript", "-l", "JavaScript", str(self.helper)]
        command.extend(["preflight"] if operation == "preflight" else [str(self.pid), operation, *arguments])
        raw = self.owner.captured(command, self.run, self.env)
        value = json.loads(raw.decode("utf-8"))
        if operation not in ("snapshot", "preflight"):
            require(value == {"status": "performed", "operation": operation}, "AX helper did not confirm requested action")
        return value

    def snapshot(self):
        result = self.call("snapshot")
        if result == {"status": "not-ready", "ax_error": -25204, "attribute": "AXWindows"}:
            return None
        require(isinstance(result.get("nodes"), list) and len(result["nodes"]) <= 2000, "AX snapshot schema invalid")
        return result["nodes"]

    def wait(self, predicate, description):
        until = min(self.owner.deadline, time.monotonic() + 30)
        not_ready = False
        while time.monotonic() < until:
            nodes = self.snapshot()
            if nodes is None:
                if not not_ready:
                    logging.info("Waiting stage=%s pid=%s attribute=AXWindows ax_error=-25204", description, self.pid)
                not_ready = True
            else:
                not_ready = False
                result = predicate(nodes)
                if result:
                    logging.info("Verified stage=%s", description)
                    return result
            time.sleep(min(0.1, max(0, until - time.monotonic())))  # Poll actual AX state under deadline.
        detail = " attribute=AXWindows ax_error=-25204" if not_ready else ""
        raise TimeoutError(f"Original macOS AX state not reached: {description} pid={self.pid}{detail}")

    @staticmethod
    def text(nodes):
        return "\n".join(str(value) for node in nodes for value in [*node["names"], node["value"]] if value is not None)

    @staticmethod
    def response(nodes):
        found = [index for index, node in enumerate(nodes) if "Service response JSON" in node["names"]]
        require(len(found) <= 1, "Accessible service JSON is ambiguous")
        if not found:
            return None
        index = found[0]
        direct = nodes[index]["value"]
        if isinstance(direct, str) and direct.strip():
            return json.loads(direct)
        # WKWebView may expose a labelled pre as a group of static text, not an AXValue.
        values = []
        for node in nodes:
            parent = node["parent"]
            while parent >= 0 and parent != index:
                parent = nodes[parent]["parent"]
            if parent == index and node["role"] == "AXStaticText":
                value = node["value"] if isinstance(node["value"], str) else "".join(node["names"][:1])
                values.append(value)
        return json.loads("\n".join(values)) if values else None

    def role(self, name, allowed):
        nodes = self.snapshot()
        require(nodes is not None, "AXWindows messaging failed outside a read-only readiness wait")
        found = [node for node in nodes if name in node["names"] and node["role"] in allowed]
        require(len(found) == 1, f"Expected one semantic control named {name}; found={len(found)}")
        return found[0]["role"]

    def press(self, name, role="AXButton"):
        self.call("press", role, name)

    def set(self, name, value):
        self.call("set", self.role(name, ("AXTextField", "AXTextArea")), name, value)

    def check(self, name, value):
        self.call("check", "AXCheckBox", name, str(value).lower())

    def open(self, workspace):
        self.set("Workspace folder", str(workspace))
        self.press("Open workspace")


def scenario(ax, workspace, run):
    ax.wait(lambda nodes: "Native desktop bridge connected" in AX.text(nodes), "native-bridge")
    ax.open(workspace)
    response = ax.wait(lambda nodes: AX.response(nodes) if "Workspace opened" in AX.text(nodes) else None, "workspace-open")
    require(all(response.get(key) == 0 for key in ("sources", "packages", "installed")), "Native empty-workspace status incorrect")
    require(Path(response["workspace"]).resolve() == workspace, "Native IPC used a different workspace")
    ax.press("Refresh status")
    ax.wait(lambda nodes: "status:" in AX.text(nodes) and AX.response(nodes), "status-refresh")
    ax.press("Updates", "AXLink")
    sync = ax.wait(AX.response, "sync-response")
    require(sync.get("status") == "success" and sync.get("sources") == [], "Native sync response incorrect")
    jobs = list((workspace / ".forge-state/jobs").glob("*.json"))
    require(len(jobs) == 1, "Open did not persist exactly one sync job")
    job = json.loads(bounded_text(jobs[0]))
    require(job["status"] == "success" and job["request"] == {"action": "sync", "apply": False, "offline": False}, "Persisted job changed check-only policy")
    ax.press("List jobs")
    listed = ax.wait(lambda nodes: AX.response(nodes) if "job-list:" in AX.text(nodes) else None, "persisted-job-list")
    require(jobs[0].stem in json.dumps(listed), "Native job list omitted persisted job")
    ax.press("Sources", "AXLink")
    ax.wait(lambda nodes: "Source update policy" in AX.text(nodes), "source-policy-view")
    ax.set("Source ID", "fixture")
    ax.set("Repository", f"local:{workspace}")
    ax.press("Register source")
    ax.wait(lambda nodes: "source-add:" in AX.text(nodes), "source-registration")
    ax.set("Source policy ID", "fixture")
    ax.call("select", ax.role("Update policy", ("AXPopUpButton", "AXComboBox")), "Update policy", "Allow approved safe content only")
    ax.check("Include this source in update checks", True)
    ax.check("I authorize these source policy changes", True)
    ax.press("Save source policy")
    ax.wait(lambda nodes: "source-policy:" in AX.text(nodes) and "updated" in AX.text(nodes), "policy-persisted")
    registration = bounded_text(workspace / "registry/sources.json").encode("utf-8")
    rows = json.loads(registration.decode("utf-8"))["sources"]
    require(len(rows) == 1 and rows[0]["id"] == "fixture" and rows[0]["enabled"] is True and rows[0]["update"] == "approved-content-only", "Native policy did not persist explicit selection")
    nodes = ax.snapshot()
    consent = [node for node in nodes if node["role"] == "AXCheckBox" and "I authorize these source policy changes" in node["names"]]
    require(len(consent) == 1 and consent[0]["value"] in (False, 0), "Policy confirmation did not reset")
    require(not (workspace / "forge.lock.json").exists(), "Policy update unexpectedly approved source")
    ax.set("Source policy ID", "missing-source")
    ax.check("I authorize these source policy changes", True)
    ax.press("Save source policy")
    ax.wait(lambda nodes: "Unknown source" in AX.text(nodes), "unknown-source-refused")
    require((workspace / "registry/sources.json").read_bytes() == registration, "Refused policy changed registration bytes")
    missing = run / "missing-workspace"
    ax.open(missing)
    ax.wait(lambda nodes: "workspace: Select an absolute path to an existing workspace folder" in AX.text(nodes), "invalid-root-refused")
    require(not missing.exists(), "Invalid workspace unexpectedly created")
    logs = list((workspace / ".forge-state/logs").glob("numera-forge-desktop_*.log"))
    require(len(logs) == 1, "Native dispatch did not create one per-run log")
    log = bounded_text(logs[0])
    require(all(f"action={action}" in log for action in ("status", "job-run", "job-list", "source-add", "source-policy")), "Real dispatch log disagrees with UI actions")
    require("code=" in log and "[ERROR]" in log, "Unknown-source native error was not logged")
    (run / "application-dispatch.log").write_text(log, encoding="utf-8")
    logging.info("Original UI/status/sync/job/policy/refusal/error/log agreement verified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == "darwin" and os.environ.get("GITHUB_ACTIONS") == "true" and
            os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted" and os.environ.get("RUNNER_OS") == "macOS",
            "Native macOS smoke is restricted to ephemeral GitHub-hosted macOS CI")
    root = Path(__file__).resolve().parent.parent
    application = args.application.resolve(strict=True)
    work = args.work.resolve()
    require(application.is_relative_to(root / "target/release") and work.is_relative_to(root / ".ci-work"), "Native binary/work paths must be project-owned release/CI paths")
    require(application.is_file() and not args.application.is_symlink(), "Original release must be a regular executable")
    work.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    run = work / f"run-{uuid.uuid4().hex}"
    run.mkdir(mode=0o700)
    log_path = work / f"native-macos-smoke_{stamp}_{run.name}.log"
    try:
        handler = logging.FileHandler(log_path, mode="x", encoding="utf-8")
    except OSError:
        print("Native macOS logging initialization failed; refusing application launch", file=sys.stderr)
        run.rmdir()
        raise
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [NATIVE-MACOS] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    owner = None
    app_output = None
    try:
        env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "en_US.UTF-8", "LC_ALL": "en_US.UTF-8",
               "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "macOS",
               "HOOKMAKER_MAX_TEST_WORKERS": "1"}
        for key in ("HOME", "TMPDIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            directory = run / key.lower()
            directory.mkdir(mode=0o700)
            env[key] = str(directory)
        workspace = run / "workspace"
        workspace.mkdir()
        application_output = run / "application-startup.log"
        helper = Path(__file__).with_name("native-macos-ax.js").resolve(strict=True)
        with application.open("rb") as binary:
            digest = hashlib.file_digest(binary, "sha256").hexdigest()
        logging.info("Original binary sha256=%s architecture=%s image=%s worker=1", digest, os.uname().machine, os.environ.get("ImageVersion", "unknown"))
        owner = Owner(time.monotonic() + 120)
        ax = AX(owner, helper, run, env, application_output)
        def interrupted(signum, frame):
            raise TimeoutError("Native macOS smoke interrupted; terminating owned processes")
        for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM):
            signal.signal(signum, interrupted)
        signal.setitimer(signal.ITIMER_REAL, max(0.01, owner.deadline - time.monotonic()))
        require(ax.call("preflight") == {"trusted": True}, "AX preflight did not confirm existing trust")
        app_output = application_output.open("xb")
        ax.process = owner.spawn([str(application)], run, env, app_output)
        ax.pid = ax.process.pid
        logging.info("Owned original app pid=%d identity=%s", ax.pid, owner.owned.get(ax.pid))
        scenario(ax, workspace, run)
        with application.open("rb") as binary:
            require(hashlib.file_digest(binary, "sha256").hexdigest() == digest, "Original release bytes changed during acceptance")
    except BaseException:
        logging.exception("Original macOS native acceptance failed or blocked")
        raise
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        try:
            cleanup_failure = None
            try:
                if owner:
                    owner.cleanup()
            except Exception as error:
                cleanup_failure = error
                logging.exception("Native process cleanup failed; retaining fixture diagnostics")
            finally:
                if app_output:
                    app_output.close()
            for source, label in ((run / "application-startup.log", "startup"),):
                if source.exists():
                    target = work / f"native-macos-{label}_{stamp}_{run.name}.log"
                    with source.open("rb") as original, target.open("xb") as saved:
                        saved.write(original.read(LIMIT))
                    if source.stat().st_size > LIMIT:
                        logging.error("Original startup output exceeded1MiB; diagnostic capture truncated")
            diagnostic_logs = list((run / "workspace/.forge-state/logs").glob("numera-forge-desktop_*.log"))
            if len(diagnostic_logs) == 1:
                (work / f"native-macos-dispatch_{stamp}_{run.name}.log").write_text(bounded_text(diagnostic_logs[0]), encoding="utf-8")
            if cleanup_failure:
                raise cleanup_failure
            for name in ("workspace", "home", "tmpdir", "xdg_config_home", "xdg_data_home", "xdg_cache_home"):
                path = run / name
                if path.exists():
                    require(not path.is_symlink() and path.parent == run, "Owned fixture cleanup boundary changed")
                    shutil.rmtree(path)
                    require(not path.exists(), "Owned fixture was not removed")
            logging.info("Owned first-party workspace/config/temp fixtures removed")
        finally:
            handler.flush()
            handler.close()
            logging.getLogger().removeHandler(handler)
    print(json.dumps({"status": "success", "surface": "original-macOS-release-external-AX",
                      "checks": ["real-ui-ipc", "check-only-job", "source-policy-persistence", "consent-reset", "policy-refusal", "invalid-root", "dispatch-log", "owned-process-cleanup"]}))


if __name__ == "__main__":
    main()
