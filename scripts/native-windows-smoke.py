#!/usr/bin/env python3
"""Hosted-Windows-only original release DOM/IPC smoke, never on the input desktop."""

import argparse
import ctypes
from ctypes import wintypes
import importlib.util
import json
import logging
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import urllib.request
import zipfile

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


def edge_driver(tools: Path) -> tuple[Path, Path, str]:
    """Download only the exact installed Edge version, with traversal/size checks."""
    candidates = []
    runtimes = []
    for key in ("ProgramFiles(x86)", "ProgramFiles"):
        root = os.environ.get(key)
        if not root:
            continue
        base = Path(root) / "Microsoft"
        edge = base / "Edge/Application/msedge.exe"
        if edge.is_file():
            candidates.append(edge)
        runtimes.extend((base / "EdgeWebView/Application").glob("*/msedgewebview2.exe"))
    if not candidates or not runtimes:
        raise RuntimeError("Hosted runner lacks installed Microsoft Edge or WebView2 runtime")
    edge_version = file_version(candidates[0])
    if not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", edge_version):
        raise RuntimeError("Installed Edge version is not a four-part numeric version")
    compatible = [path for path in runtimes if file_version(path).split(".")[:3] == edge_version.split(".")[:3]]
    if not compatible:
        raise RuntimeError(f"Installed Edge {edge_version} has no matching WebView2 runtime; refusing mismatched driver")
    runtime = max(compatible, key=lambda path: tuple(map(int, file_version(path).split(".")))).parent
    destination = tools / f"edge-{edge_version}"
    destination.mkdir(parents=True, exist_ok=False)
    archive = destination / "edgedriver_win64.zip"
    url = f"https://msedgedriver.microsoft.com/{edge_version}/edgedriver_win64.zip"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(url, timeout=30) as response, archive.open("xb") as output:
        if response.url != url:
            raise RuntimeError("Official EdgeDriver download redirected to an unexpected URL")
        deadline, total = time.monotonic() + 60, 0
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError("EdgeDriver download exceeded 60-second deadline")
            chunk = response.read(65536)
            if not chunk:
                break
            total += len(chunk)
            if total > 32 * 1024 * 1024:
                raise RuntimeError("EdgeDriver archive exceeded 32 MiB limit")
            output.write(chunk)
    driver = destination / "msedgedriver.exe"
    with zipfile.ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            relative = Path(entry.filename.replace("\\", "/"))
            if relative.is_absolute() or ".." in relative.parts or ":" in entry.filename:
                raise RuntimeError("EdgeDriver archive contains an unsafe path")
            if entry.file_size > 64 * 1024 * 1024 or (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise RuntimeError("EdgeDriver archive contains oversized or linked content")
        entries = [entry for entry in bundle.infolist() if entry.filename == "msedgedriver.exe"]
        if len(entries) != 1:
            raise RuntimeError("Official EdgeDriver archive has no unique root executable")
        with bundle.open(entries[0]) as source, driver.open("xb") as target:
            shutil.copyfileobj(source, target)
    archive.unlink()
    if file_version(driver).split(".")[:3] != edge_version.split(".")[:3]:
        raise RuntimeError("Downloaded EdgeDriver does not match installed Edge/WebView2")
    logging.info("Matched installed Edge=%s WebView2=%s driver=%s", edge_version, runtime.name, file_version(driver))
    return driver, runtime, edge_version


def worker(args: argparse.Namespace, run: Path) -> None:
    """Run unchanged UI contracts inside the already-owned private desktop."""
    windows = load("native_windows_process", "native-windows-process.py")
    windows.require_worker_isolation()
    linux = load("native_desktop_contracts", "native-desktop-smoke.py")
    progress = run / "progress.json"

    def checkpoint(stage: str) -> None:
        temporary = run / "progress.tmp"
        temporary.write_text(json.dumps({"stage": stage, "pid": os.getpid()}), encoding="utf-8")
        temporary.replace(progress)
        logging.info("Stage=%s", stage)

    checkpoint("private-desktop-unelevated-job-confirmed")
    driver_port, debug_port = linux.port(), linux.port()
    while debug_port == driver_port:
        debug_port = linux.port()
    driver = linux.Driver(f"http://127.0.0.1:{driver_port}", time.monotonic() + 120)
    workspace = run / "workspace"
    workspace.mkdir()
    process = application_process = None
    inherited_desktop = windows.inherit_private_desktop()
    try:
        with (run / "driver-output.log").open("w", encoding="utf-8") as output, \
                (run / "application-startup.log").open("x", encoding="utf-8") as application_output:
            startup = subprocess.STARTUPINFO()
            startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
            startup.wShowWindow = 0
            process = subprocess.Popen([str(args.native_driver), f"--port={driver_port}", "--verbose", f"--log-path={run / 'edge-session.log'}"], cwd=run,
                                       stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                       startupinfo=startup, close_fds=False, creationflags=subprocess.DETACHED_PROCESS)
            checkpoint("driver-started")
            ready = time.monotonic() + 20
            while True:
                if process.poll() is not None:
                    raise RuntimeError("tauri-driver exited before readiness")
                try:
                    status = driver.call("GET", "/status")
                    if isinstance(status, dict) and status.get("ready") is True:
                        break
                except (OSError, linux.WebDriverError):
                    pass
                if time.monotonic() >= ready:
                    raise TimeoutError("Windows native driver did not become ready")
                time.sleep(0.1)
            checkpoint("driver-ready")
            environment = {**os.environ,
                           "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS": f"--remote-debugging-port={debug_port}"}
            application_process = subprocess.Popen([str(args.application)], cwd=run, env=environment,
                                                    stdin=subprocess.DEVNULL, stdout=application_output,
                                                    stderr=subprocess.STDOUT, close_fds=False,
                                                    creationflags=subprocess.CREATE_NO_WINDOW)
            logging.info("Original application pid=%d started with captured diagnostics", application_process.pid)
            checkpoint("original-application-started")
            cdp = linux.Driver(f"http://127.0.0.1:{debug_port}", driver.deadline)
            ready = time.monotonic() + 30
            while True:
                code = application_process.poll()
                if code is not None:
                    raise RuntimeError(f"Original application exited during startup code={code:#x}; see application-startup.log")
                try:
                    with cdp.http.open(cdp.address + "/json/version", timeout=1) as response:
                        metadata = json.loads(response.read(65536).decode("utf-8"))
                    if metadata.get("webSocketDebuggerUrl"):
                        break
                except (OSError, ValueError):
                    pass
                if time.monotonic() >= ready:
                    raise TimeoutError("Original application did not expose its loopback debugging endpoint in 30s")
                time.sleep(0.1)
            checkpoint("original-webview-ready")
            session = driver.call("POST", "/session", {"capabilities": {"alwaysMatch": {
                "browserName": "webview2", "ms:edgeChromium": True,
                "ms:edgeOptions": {"debuggerAddress": f"127.0.0.1:{debug_port}"}}}}, request_timeout=30)
            driver.session = session["sessionId"]
            checkpoint("native-session-created")
            driver.call("POST", f"/session/{driver.session}/timeouts", {"implicit": 0, "pageLoad": 30000, "script": 10000})
            driver.wait("return document.querySelector('#bridge-state')?.textContent.includes('Native desktop bridge connected');")
            checkpoint("real-release-dom-started")
            driver.script("const field = document.querySelector('#workspace-root'); field.value = arguments[0]; document.querySelector('#workspace-form').requestSubmit(); return true;", str(workspace))
            response = driver.wait("if(document.querySelector('#main')?.getAttribute('aria-busy') !== 'false') return null; const p = document.querySelector('[data-result=dashboard] pre'); return p ? JSON.parse(p.textContent) : null;")
            if response["sources"] != 0 or response["packages"] != 0 or response["installed"] != 0 or Path(response["workspace"]).resolve() != workspace.resolve():
                raise RuntimeError("Real native status did not report isolated empty workspace")
            sync = driver.script("return JSON.parse(document.querySelector('[data-result=updates] pre').textContent);")
            if sync["status"] != "success" or sync["sources"] != []:
                raise RuntimeError("Real native sequential check failed")
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
            driver.script("document.querySelector('#workspace-root').value = arguments[0]; document.querySelector('#workspace-form').requestSubmit(); return true;", str(run / "missing-workspace"))
            driver.wait("const n=document.querySelector('#notice'); return n?.classList.contains('error') && n.textContent.includes('workspace:') && document.querySelector('#main').getAttribute('aria-busy') === 'false' ? n.textContent : null;")
            if (run / "missing-workspace").exists():
                raise RuntimeError("Invalid workspace was unexpectedly created")
            checkpoint("invalid-workspace-rejected")
    finally:
        # Only startup diagnostics from the isolated empty profile, never database/profile state.
        for candidate in list((run / "tmp").glob("scoped_dir*/EBWebView/chrome_debug.log"))[:2]:
            data = candidate.read_bytes()
            if len(data) <= 262144:
                (run / f"webview-startup-{candidate.parent.parent.name}.log").write_bytes(data)
        if driver.session:
            driver.deadline = time.monotonic() + 5
            try:
                driver.call("DELETE", f"/session/{driver.session}")
            except (OSError, linux.WebDriverError, ValueError, TimeoutError):
                logging.warning("Session close failed; outer owner will terminate its Job Object")
        for child in (application_process, process):
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
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--native-driver", type=Path)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise RuntimeError("Native Windows smoke is restricted to ephemeral GitHub-hosted Windows CI")
    root = Path(__file__).resolve().parent.parent
    application = args.application.resolve(strict=True)
    args.application = application
    args.driver = args.driver.resolve(strict=True)
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
        native_driver, runtime, version = edge_driver(tools)
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
        environment["TAURI_WEBVIEW_AUTOMATION"] = "true"
        interpreter = Path(sys.executable).with_name("pythonw.exe").resolve(strict=True)
        command = [str(interpreter), str(Path(__file__).resolve()), "--worker", "--application", str(application),
                   "--driver", str(args.driver), "--native-driver", str(native_driver), "--work", str(run)]
        result = None
        with windows.Owner(command, environment, run) as owner:
            logging.info("Owned suspended worker pid=%d started=%s private-desktop=%s Edge=%s", owner.pid, owner.started, owner.desktop_name, version)
            deadline, idle, prior = time.monotonic() + 150, time.monotonic() + 40, None
            inspection = time.monotonic() + 2
            progress = run / "progress.json"
            while (result := owner.poll()) is None:
                state = progress.read_text(encoding="utf-8") if progress.exists() else None
                if state != prior:
                    logging.info("Worker progress=%s", state)
                    prior, idle = state, time.monotonic() + 40
                if time.monotonic() >= inspection:
                    windows.diagnostics.diagnose_owned_processes(windows, owner)
                    inspection = time.monotonic() + 5
                if time.monotonic() >= deadline or time.monotonic() >= idle:
                    raise TimeoutError("Windows native worker exceeded 150s wall or 40s progress deadline")
                time.sleep(0.1)
            if result != 0:
                windows.diagnostics.diagnose_owned_processes(windows, owner)
                raise RuntimeError(f"Windows native worker failed with exit code {result}; see captured per-run logs")
            final = json.loads(progress.read_text(encoding="utf-8"))
            if final.get("stage") != "contracts-complete":
                raise RuntimeError("Worker exited without real DOM/IPC contract evidence")
        logging.info("Verified owned Job Object active processes=0; private desktop closed")
        for directory in ("workspace", "home", "config", "data", "tmp", "webview-profile"):
            path = run / directory
            if path.exists():
                shutil.rmtree(path)
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
