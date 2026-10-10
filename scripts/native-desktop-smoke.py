#!/usr/bin/env python3
"""Linux-only native release startup and real service IPC under an existing Xvfb display."""

import argparse
import json
import logging
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import time
import urllib.error
import urllib.request


class WebDriverError(RuntimeError):
    pass


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class Driver:
    def __init__(self, address, deadline):
        self.address = address
        self.deadline = deadline
        self.session = None
        self.http = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def call(self, method, path, payload=None):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Native smoke exceeded its 120-second deadline")
        data = None if payload is None else json.dumps(payload).encode("utf-8")
        request = urllib.request.Request(self.address + path, data=data, method=method,
                                         headers={"Content-Type": "application/json; charset=utf-8"})
        try:
            with self.http.open(request, timeout=min(10, remaining)) as response:
                result = json.loads(response.read(1024 * 1024).decode("utf-8"))
        except urllib.error.HTTPError as error:
            raise WebDriverError(f"Native WebDriver request failed with HTTP {error.code}") from error
        value = result.get("value")
        if isinstance(value, dict) and value.get("error"):
            raise WebDriverError(f"Native WebDriver error: {value['error']}")
        return value

    def script(self, script, *args):
        return self.call("POST", f"/session/{self.session}/execute/sync", {"script": script, "args": list(args)})

    def wait(self, script, *args, seconds=30):
        deadline = min(self.deadline, time.monotonic() + seconds)
        last = None
        while time.monotonic() < deadline:
            last = self.script(script, *args)
            if last:
                return last
            time.sleep(min(0.1, max(0, deadline - time.monotonic())))
        raise TimeoutError("Native application did not reach the expected UI/IPC state")


def identity(pid):
    try:
        fields = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8").rsplit(")", 1)[1].split()
        return fields[19], fields[0]
    except (OSError, ValueError, IndexError):
        return None


def descendants(parent):
    found = set()
    parents = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text(encoding="utf-8").rsplit(")", 1)[1].split()
            parents[int(entry.name)] = int(fields[1])
        except (OSError, ValueError, IndexError):
            continue
    frontier = {parent}
    while frontier:
        children = {pid for pid, ppid in parents.items() if ppid in frontier} - found
        found.update(children)
        frontier = children
    return {pid: state[0] for pid in found if (state := identity(pid))}


def cleanup(process, owned):
    owned.update(descendants(process.pid))
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        for pid, started in owned.items():
            state = identity(pid)
            if not state or state[0] != started:
                continue
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass
        if sig == signal.SIGTERM:
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                if all(not (state := identity(pid)) or state[0] != started or state[1] == "Z" for pid, started in owned.items()):
                    break
                time.sleep(0.05)
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            continue
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if all(not (state := identity(pid)) or state[0] != started or state[1] == "Z" for pid, started in owned.items()):
            break
        time.sleep(0.05)
    alive = []
    for pid, started in owned.items():
        state = identity(pid)
        if state and state[0] == started and state[1] != "Z":
            alive.append(pid)
    if process.poll() is None or alive:
        raise RuntimeError(f"Native smoke left owned processes alive: {alive}")


def main():
    def interrupted(signum, frame):
        raise TimeoutError("Native smoke interrupted; cleaning owned processes")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "posix" or not Path("/proc").is_dir() or not os.environ.get("DISPLAY"):
        raise RuntimeError("Native smoke requires Linux and an isolated Xvfb DISPLAY")
    application = args.application.resolve(strict=True)
    executable = args.driver.resolve(strict=True)
    native_driver = shutil.which("WebKitWebDriver")
    if not native_driver:
        raise RuntimeError("WebKitWebDriver is missing; install the documented CI prerequisite")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d_%H-%M-%S_UTC", time.gmtime())
    log_path = work / f"native-desktop-smoke_{stamp}_{os.getpid()}.log"
    logging.basicConfig(filename=log_path, encoding="utf-8", level=logging.INFO,
                        format="[%(asctime)s UTC] [%(levelname)s] [NATIVE] %(message)s", datefmt="%Y-%m-%d %H:%M:%S")
    logging.Formatter.converter = time.gmtime
    isolated = work / f"run-{os.getpid()}"
    isolated.mkdir()
    workspace = isolated / "workspace"
    workspace.mkdir()
    env = os.environ.copy()
    for variable, relative in {"HOME": "home", "XDG_CONFIG_HOME": "config", "XDG_DATA_HOME": "data", "XDG_CACHE_HOME": "cache", "XDG_RUNTIME_DIR": "runtime", "TMPDIR": "tmp"}.items():
        directory = isolated / relative
        directory.mkdir(mode=0o700)
        env[variable] = str(directory)
    env["TAURI_WEBVIEW_AUTOMATION"] = "true"
    driver_port, native_port = port(), port()
    while native_port == driver_port:
        native_port = port()
    deadline = time.monotonic() + 120
    driver = Driver(f"http://127.0.0.1:{driver_port}", deadline)
    process = None
    owned = {}
    passed = False
    try:
        logging.info("Starting original release binary via tauri-driver")
        with (work / "driver-output.log").open("w", encoding="utf-8") as output:
            process = subprocess.Popen([str(executable), "--port", str(driver_port), "--native-port", str(native_port), "--native-driver", native_driver],
                                       cwd=isolated, env=env, stdin=subprocess.DEVNULL,
                                       stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            ready_deadline = min(deadline, time.monotonic() + 20)
            while True:
                if process.poll() is not None:
                    raise RuntimeError("tauri-driver exited before readiness")
                try:
                    status = driver.call("GET", "/status")
                    if isinstance(status, dict) and status.get("ready") is True:
                        break
                except (OSError, WebDriverError):
                    pass
                if time.monotonic() >= ready_deadline:
                    raise TimeoutError("Native WebDriver did not become ready")
                time.sleep(0.1)
            session = driver.call("POST", "/session", {"capabilities": {"alwaysMatch": {"browserName": "wry", "tauri:options": {"application": str(application)}}}})
            driver.session = session["sessionId"]
            owned.update(descendants(process.pid))
            driver.call("POST", f"/session/{driver.session}/timeouts", {"implicit": 0, "pageLoad": 30000, "script": 10000})
            driver.wait("return document.querySelector('#bridge-state')?.textContent.includes('Native desktop bridge connected');")
            driver.script("const field = document.querySelector('#workspace-root'); field.value = arguments[0]; document.querySelector('#workspace-form').requestSubmit(); return true;", str(workspace))
            response = driver.wait("if(document.querySelector('#main')?.getAttribute('aria-busy') !== 'false') return null; const p = document.querySelector('[data-result=dashboard] pre'); return p ? JSON.parse(p.textContent) : null;")
            assert response["sources"] == 0 and response["packages"] == 0 and response["installed"] == 0, "Native status did not report the isolated empty workspace"
            assert Path(response["workspace"]).resolve() == workspace.resolve(), "IPC used a different workspace"
            sync = driver.script("return JSON.parse(document.querySelector('[data-result=updates] pre').textContent);")
            assert sync["status"] == "success" and sync["sources"] == [], "Native sequential check failed"
            app_logs = list((workspace / ".forge-state/logs").glob("numera-forge-desktop_*.log"))
            assert len(app_logs) == 1, "Native IPC did not create a per-run desktop log"
            log_text = app_logs[0].read_text(encoding="utf-8")
            assert "action=status" in log_text and "action=job-run" in log_text, "Real native dispatch evidence missing"
            job_files = list((workspace / ".forge-state/jobs").glob("*.json"))
            assert len(job_files) == 1, "Native open did not persist exactly one sequential sync job"
            job = json.loads(job_files[0].read_text(encoding="utf-8"))
            assert job["status"] == "success" and job["request"] == {"action": "sync", "apply": False, "offline": False}, "Native open job did not preserve check-only policy"
            driver.script("document.querySelector('#workspace-root').value = arguments[0]; document.querySelector('#workspace-form').requestSubmit(); return true;", str(isolated / "missing-workspace"))
            failure = driver.wait("const n=document.querySelector('#notice'); return n?.classList.contains('error') && n.textContent.includes('workspace:') && document.querySelector('#main').getAttribute('aria-busy') === 'false' ? n.textContent : null;")
            assert failure, "Invalid workspace was not rejected through native IPC"
            assert not (isolated / "missing-workspace").exists(), "Invalid workspace was unexpectedly created"
            passed = True
            logging.info("Native startup, real status/sync IPC, file diagnostics and failure path passed")
    except BaseException:
        logging.exception("Native smoke failed")
        raise
    finally:
        try:
            if process:
                owned.update(descendants(process.pid))
            if driver.session:
                driver.deadline = time.monotonic() + 5
                try:
                    driver.call("DELETE", f"/session/{driver.session}")
                except (OSError, WebDriverError, ValueError, TimeoutError):
                    logging.warning("Session close failed; terminating owned process tree")
            if process:
                cleanup(process, owned)
            logging.info("Owned process tree stopped")
            if passed:
                shutil.rmtree(isolated)
        finally:
            logging.shutdown()
    print(json.dumps({"status": "success", "surface": "Linux native release/Xvfb", "checks": ["startup", "real-service-ipc", "invalid-workspace", "diagnostic-file", "owned-process-cleanup"]}))


if __name__ == "__main__":
    main()
