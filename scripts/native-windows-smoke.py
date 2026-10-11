#!/usr/bin/env python3
"""Hosted-Windows original release accessibility/IPC smoke, never on the input desktop."""

import argparse
import ctypes
from ctypes import wintypes
import importlib.util
import json
import logging
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True


def load(name: str, filename: str) -> object:
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def file_version(path: Path) -> str:
    """Read PE version metadata without executing the installed browser or runtime."""
    dll = ctypes.WinDLL("version", use_last_error=True)
    size = dll.GetFileVersionInfoSizeW
    size.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(wintypes.DWORD)]
    size.restype = wintypes.DWORD
    read = dll.GetFileVersionInfoW
    read.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
    read.restype = wintypes.BOOL
    query = dll.VerQueryValueW
    query.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(wintypes.UINT)]
    query.restype = wintypes.BOOL
    length = size(str(path), None)
    data = ctypes.create_string_buffer(length)
    pointer, count = ctypes.c_void_p(), wintypes.UINT()
    if not length or not read(str(path), 0, length, data) or not query(data, "\\", ctypes.byref(pointer), ctypes.byref(count)):
        raise OSError(ctypes.get_last_error(), "Cannot read Edge/WebView2/driver file version")
    fields = ctypes.cast(pointer, ctypes.POINTER(wintypes.DWORD))
    if count.value < 52 or fields[0] != 0xFEEF04BD:
        raise RuntimeError("Invalid PE version metadata")
    return f"{fields[2] >> 16}.{fields[2] & 65535}.{fields[3] >> 16}.{fields[3] & 65535}"


def worker(args: argparse.Namespace, run: Path) -> None:
    """Run unchanged UI contracts inside the already-owned private desktop."""
    windows = load("native_windows_process", "native-windows-process.py")
    windows.require_worker_isolation()
    progress = run / "progress.json"

    def checkpoint(stage: str) -> None:
        temporary = run / "progress.tmp"
        temporary.write_text(json.dumps({"stage": stage, "pid": os.getpid()}), encoding="utf-8")
        temporary.replace(progress)
        logging.info("Stage=%s", stage)

    checkpoint("private-desktop-unelevated-job-confirmed")
    workspace = run / "workspace"
    workspace.mkdir()
    process = application_process = None
    inherited_desktop = windows.inherit_private_desktop()
    try:
        with ((run / "automation-output.log").open("x", encoding="utf-8") as output,
              (run / "application-startup.log").open("x", encoding="utf-8") as application_output):
            application_process = subprocess.Popen([str(args.application)], cwd=run,
                                                    stdin=subprocess.DEVNULL, stdout=application_output,
                                                    stderr=subprocess.STDOUT, close_fds=False,
                                                    creationflags=subprocess.CREATE_NO_WINDOW)
            checkpoint("original-application-started")
            process = subprocess.Popen([str(args.native_driver), str(application_process.pid), str(workspace), str(run)],
                                       cwd=run, stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                       close_fds=False, creationflags=subprocess.CREATE_NO_WINDOW)
            checkpoint("native-accessibility-started")
            deadline = time.monotonic() + 120
            while (code := process.poll()) is None:
                if application_process.poll() is not None:
                    raise RuntimeError(f"Original application exited code={application_process.returncode}; see application-startup.log")
                if time.monotonic() >= deadline:
                    raise TimeoutError("Native accessibility contracts exceeded 120s")
                time.sleep(0.1)
            if code != 0:
                raise RuntimeError(f"Native accessibility helper failed exit={code}; see native-uia log")
            result = json.loads((run / "uia-result.json").read_text(encoding="utf-8"))
            if result != {"status": "success", "surface": "native-accessibility"}:
                raise RuntimeError("Native helper did not confirm real UI contracts")
            checkpoint("native-release-ui-and-failure-confirmed")
            logs = list((workspace / ".forge-state/logs").glob("numera-forge-desktop_*.log"))
            if len(logs) != 1:
                raise RuntimeError("Native IPC did not create exactly one per-run desktop log")
            text = logs[0].read_text(encoding="utf-8")
            if "action=status" not in text or "action=job-run" not in text:
                raise RuntimeError("Real native dispatch diagnostics missing")
            shutil.copyfile(logs[0], run / "application-output.log")
            jobs = list((workspace / ".forge-state/jobs").glob("*.json"))
            if len(jobs) != 1:
                raise RuntimeError("Native open did not persist exactly one sequential sync job")
            job = json.loads(jobs[0].read_text(encoding="utf-8"))
            if job["status"] != "success" or job["request"] != {"action": "sync", "apply": False, "offline": False}:
                raise RuntimeError("Native open did not preserve check-only policy")
            checkpoint("status-sync-job-log-confirmed")
    finally:
        for child in (process, application_process):
            if child:
                child.terminate() if child.poll() is None else None
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
        windows.checked(windows.close_desktop(inherited_desktop), "CloseDesktop(inherited worker desktop)")
    checkpoint("contracts-complete")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--native-driver", type=Path)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise RuntimeError("Native Windows smoke is restricted to ephemeral GitHub-hosted Windows CI")
    root = Path(__file__).resolve().parent.parent
    application = args.application.resolve(strict=True)
    args.application = application
    work = args.work.resolve()
    if not work.is_relative_to(root / ".ci-work") or not application.is_relative_to(root / "target/release"):
        raise RuntimeError("Native Windows binary/work paths must be project-owned release/CI paths")
    work.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d_%H-%M-%S_UTC", time.gmtime())
    log = work / f"native-windows-smoke_{stamp}_{os.getpid()}.log"
    try:
        handler = logging.FileHandler(log, mode="x", encoding="utf-8")
    except OSError:
        print("Windows native smoke could not initialize its diagnostic log; refusing to start", file=sys.stderr)
        raise
    handler.setFormatter(logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [NATIVE-WINDOWS] %(message)s", "%Y-%m-%d %H:%M:%S"))
    logging.Formatter.converter = time.gmtime
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        if args.worker:
            args.native_driver = args.native_driver.resolve(strict=True)
            worker(args, work)
            return
        windows = load("native_windows_process", "native-windows-process.py")
        run = work / f"run-{os.getpid()}"
        run.mkdir()
        tools = root / ".ci-work/native-tools"
        tools.mkdir(parents=True, exist_ok=True)
        runtime_candidates = []
        for key in ("ProgramFiles(x86)", "ProgramFiles"):
            base = os.environ.get(key)
            if base:
                runtime_candidates.extend((Path(base) / "Microsoft/EdgeWebView/Application").glob("*/msedgewebview2.exe"))
        if not runtime_candidates:
            raise RuntimeError("Hosted runner lacks WebView2 runtime")
        runtime_executable = max(runtime_candidates, key=lambda path: tuple(map(int, file_version(path).split("."))))
        runtime, version = runtime_executable.parent, file_version(runtime_executable)
        native_driver = tools / "native-windows-uia.exe"
        framework = Path(os.environ["WINDIR"]) / "Microsoft.NET/Framework64/v4.0.30319"
        references = [framework / "WPF" / name for name in ("UIAutomationClient.dll", "UIAutomationTypes.dll", "WindowsBase.dll")]
        command = [str(framework / "csc.exe"), "/nologo", "/target:winexe", f"/out:{native_driver}",
                   *[f"/reference:{reference}" for reference in references], str(root / "scripts/native-windows-uia.cs")]
        build = subprocess.run(command, cwd=root, capture_output=True, text=True, encoding="utf-8",
                               timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
        if build.returncode != 0:
            logging.error("Native UI helper compilation failed: %s", build.stdout + build.stderr)
            raise RuntimeError("Native UI helper compilation failed")
        logging.info("Original WebView2 runtime=%s; native UI Automation helper compiled", version)
        allowed = {"SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "COMSPEC", "PATH", "PROGRAMFILES", "PROGRAMFILES(X86)",
                   "PROGRAMDATA", "GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"}
        environment = {key: value for key, value in os.environ.items() if key.upper() in allowed}
        environment["PYTHONUTF8"] = "1"
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        for key, relative in {"HOME": "home", "USERPROFILE": "home", "APPDATA": "config", "LOCALAPPDATA": "data", "TEMP": "tmp", "TMP": "tmp"}.items():
            directory = run / relative
            directory.mkdir(exist_ok=True)
            environment[key] = str(directory)
        environment["WEBVIEW2_BROWSER_EXECUTABLE_FOLDER"] = str(runtime)
        environment["WEBVIEW2_USER_DATA_FOLDER"] = str(run / "webview-profile")
        interpreter = Path(sys.executable).with_name("pythonw.exe").resolve(strict=True)
        command = [str(interpreter), str(Path(__file__).resolve()), "--worker", "--application", str(application),
                   "--native-driver", str(native_driver), "--work", str(run)]
        result = None
        with windows.Owner(command, environment, run) as owner:
            logging.info("Owned suspended worker pid=%d started=%s private-desktop=%s WebView2=%s", owner.pid, owner.started, owner.desktop_name, version)
            deadline, idle, prior = time.monotonic() + 150, time.monotonic() + 40, None
            progress = run / "progress.json"
            while (result := owner.poll()) is None:
                state = progress.read_text(encoding="utf-8") if progress.exists() else None
                if state != prior:
                    logging.info("Worker progress=%s", state)
                    prior, idle = state, time.monotonic() + 40
                if time.monotonic() >= deadline or time.monotonic() >= idle:
                    raise TimeoutError("Windows native worker exceeded 150s wall or 40s progress deadline")
                time.sleep(0.1)
            if result != 0:
                raise RuntimeError(f"Windows native worker failed with exit code {result}; see captured per-run logs")
            final = json.loads(progress.read_text(encoding="utf-8"))
            if final.get("stage") != "contracts-complete":
                raise RuntimeError("Worker exited without real native UI/IPC contract evidence")
        logging.info("Verified owned Job Object active processes=0; private desktop closed")
        for directory in ("workspace", "home", "config", "data", "tmp", "webview-profile"):
            path = run / directory
            if path.exists():
                def cleanup_error(function: object, failed: str, error: BaseException) -> None:
                    if isinstance(error, FileNotFoundError) and Path(failed) != path:
                        return
                    raise error
                shutil.rmtree(path, onexc=cleanup_error)
                if path.exists():
                    raise RuntimeError("Owned native fixture cleanup did not remove its directory")
        print(json.dumps({"status": "success", "surface": "Windows native release/private desktop", "checks": ["startup", "real-service-ipc", "check-only-persisted-job", "invalid-workspace", "diagnostic-file", "owned-process-cleanup"]}))
    except BaseException:
        logging.exception("Windows native smoke failed or blocked")
        raise
    finally:
        handler.flush()
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    main()
