"""Win32 owner for a CI-only, never-switched desktop and kill-on-close process tree."""

import argparse
import ctypes as c
import json
from ctypes import wintypes as w
from pathlib import Path
import logging
import os
import subprocess
import time
import sys

import importlib.util

_spec = importlib.util.spec_from_file_location("native_windows_diagnostics", Path(__file__).with_name("native-windows-diagnostics.py"))
diagnostics = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(diagnostics)


class Startup(c.Structure):
    _fields_ = [("cb", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR),
                ("title", w.LPWSTR), ("x", w.DWORD), ("y", w.DWORD),
                ("width", w.DWORD), ("height", w.DWORD), ("chars_x", w.DWORD),
                ("chars_y", w.DWORD), ("fill", w.DWORD), ("flags", w.DWORD),
                ("show", w.WORD), ("reserved_size", w.WORD),
                ("reserved_bytes", c.POINTER(c.c_byte)), ("stdin", w.HANDLE),
                ("stdout", w.HANDLE), ("stderr", w.HANDLE)]


class SidAttributes(c.Structure):
    _fields_ = [("sid", c.c_void_p), ("attributes", w.DWORD)]


class TokenGroups(c.Structure):
    _fields_ = [("count", w.DWORD), ("groups", SidAttributes * 1)]


class Process(c.Structure):
    _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE),
                ("pid", w.DWORD), ("tid", w.DWORD)]


class Limits(c.Structure):
    _fields_ = [("process_time", c.c_longlong), ("job_time", c.c_longlong),
                ("flags", w.DWORD), ("min_working", c.c_size_t),
                ("max_working", c.c_size_t), ("active_limit", w.DWORD),
                ("affinity", c.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]


class Counters(c.Structure):
    _fields_ = [(name, c.c_ulonglong) for name in
                ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]


class ExtendedLimits(c.Structure):
    _fields_ = [("basic", Limits), ("io", Counters), ("process_memory", c.c_size_t),
                ("job_memory", c.c_size_t), ("peak_process", c.c_size_t), ("peak_job", c.c_size_t)]


class Accounting(c.Structure):
    _fields_ = [("user", c.c_longlong), ("kernel", c.c_longlong),
                ("period_user", c.c_longlong), ("period_kernel", c.c_longlong),
                ("faults", w.DWORD), ("total", w.DWORD),
                ("active", w.DWORD), ("terminated", w.DWORD)]


def api(dll: object, name: str, result: object, *arguments: object) -> object:
    """Bind explicit pointer-width signatures; ctypes defaults truncate Windows handles."""
    fn = getattr(dll, name)
    fn.restype = result
    fn.argtypes = list(arguments)
    return fn


K = c.WinDLL("kernel32", use_last_error=True)
U = c.WinDLL("user32", use_last_error=True)
A = c.WinDLL("advapi32", use_last_error=True)
close = api(K, "CloseHandle", w.BOOL, w.HANDLE)
current = api(K, "GetCurrentProcess", w.HANDLE)
create_job = api(K, "CreateJobObjectW", w.HANDLE, c.c_void_p, w.LPCWSTR)
set_job = api(K, "SetInformationJobObject", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD)
query_job = api(K, "QueryInformationJobObject", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.c_void_p)
assign_job = api(K, "AssignProcessToJobObject", w.BOOL, w.HANDLE, w.HANDLE)
kill_job = api(K, "TerminateJobObject", w.BOOL, w.HANDLE, w.UINT)
kill_process = api(K, "TerminateProcess", w.BOOL, w.HANDLE, w.UINT)
wait_process = api(K, "WaitForSingleObject", w.DWORD, w.HANDLE, w.DWORD)
exit_code = api(K, "GetExitCodeProcess", w.BOOL, w.HANDLE, c.POINTER(w.DWORD))
resume = api(K, "ResumeThread", w.DWORD, w.HANDLE)
create_desktop = api(U, "CreateDesktopW", w.HANDLE, w.LPCWSTR, c.c_void_p, c.c_void_p, w.DWORD, w.DWORD, c.c_void_p)
close_desktop = api(U, "CloseDesktop", w.BOOL, w.HANDLE)
open_token = api(A, "OpenProcessToken", w.BOOL, w.HANDLE, w.DWORD, c.POINTER(w.HANDLE))
token_info = api(A, "GetTokenInformation", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
create_process = api(K, "CreateProcessW", w.BOOL, w.LPCWSTR, w.LPWSTR, c.c_void_p, c.c_void_p,
                     w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(Startup), c.POINTER(Process))
create_with_token = api(A, "CreateProcessWithTokenW", w.BOOL, w.HANDLE, w.DWORD, w.LPCWSTR,
                        w.LPWSTR, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(Startup), c.POINTER(Process))
process_in_job = api(K, "IsProcessInJob", w.BOOL, w.HANDLE, w.HANDLE, c.POINTER(w.BOOL))
restrict_token = api(A, "CreateRestrictedToken", w.BOOL, w.HANDLE, w.DWORD, w.DWORD,
                     c.c_void_p, w.DWORD, c.c_void_p, w.DWORD, c.c_void_p, c.POINTER(w.HANDLE))
set_token = api(A, "SetTokenInformation", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD)
well_known_sid = api(A, "CreateWellKnownSid", w.BOOL, c.c_int, c.c_void_p, c.c_void_p, c.POINTER(w.DWORD))
equal_sid = api(A, "EqualSid", w.BOOL, c.c_void_p, c.c_void_p)
sid_count = api(A, "GetSidSubAuthorityCount", c.POINTER(c.c_byte), c.c_void_p)
sid_part = api(A, "GetSidSubAuthority", c.POINTER(w.DWORD), c.c_void_p, w.DWORD)
valid_sid = api(A, "IsValidSid", w.BOOL, c.c_void_p)
sid_length = api(A, "GetLengthSid", w.DWORD, c.c_void_p)


def checked(value: object, operation: str) -> object:
    if not value:
        raise OSError(c.get_last_error(), f"{operation} failed; hidden owned Windows automation unavailable")
    return value


def elevated(token: object) -> bool:
    value, size = w.DWORD(), w.DWORD()
    checked(token_info(token, 20, c.byref(value), c.sizeof(value), c.byref(size)), "TokenElevation")
    return bool(value.value)


def token_buffer(token: object, kind: int) -> object:
    size = w.DWORD()
    if token_info(token, kind, None, 0, c.byref(size)) or c.get_last_error() != 122:
        raise OSError(c.get_last_error(), "Token information size query failed")
    if not 0 < size.value <= 65536:
        raise RuntimeError("Token information exceeded bounded buffer")
    buffer = c.create_string_buffer(size.value)
    checked(token_info(token, kind, buffer, size.value, c.byref(size)), "GetTokenInformation")
    return buffer


def sid(kind: int) -> object:
    buffer, size = c.create_string_buffer(68), w.DWORD(68)
    checked(well_known_sid(kind, None, buffer, c.byref(size)), "CreateWellKnownSid")
    return buffer


def require_standard_token(token: object) -> None:
    """Check integrity separately: denying administrator SIDs alone does not lower IL."""
    if elevated(token):
        raise RuntimeError("WebView2 automation token is elevated")
    integrity = token_buffer(token, 25)  # TokenIntegrityLevel.
    label = SidAttributes.from_buffer(integrity)
    checked(valid_sid(label.sid), "IsValidSid(integrity)")
    count = sid_count(label.sid)[0]
    if count <= 0 or sid_part(label.sid, count - 1)[0] > 8192:
        raise RuntimeError("WebView2 automation token exceeds Medium integrity")
    groups = token_buffer(token, 2)  # TokenGroups.
    amount = w.DWORD.from_buffer(groups).value
    offset = TokenGroups.groups.offset
    if offset + amount * c.sizeof(SidAttributes) > c.sizeof(groups):
        raise RuntimeError("Invalid token group buffer")
    admin = sid(26)  # WinBuiltinAdministratorsSid.
    for index in range(amount):
        group = SidAttributes.from_buffer(groups, offset + index * c.sizeof(SidAttributes))
        if equal_sid(group.sid, admin) and group.attributes & 4:  # SE_GROUP_ENABLED.
            raise RuntimeError("WebView2 automation token retains enabled Administrators SID")


def inherit_private_desktop() -> object:
    """Open only the verified worker desktop as the single inheritable desktop handle."""
    require_worker_isolation()
    open_desktop = api(U, "OpenDesktopW", w.HANDLE, w.LPCWSTR, w.DWORD, w.BOOL, w.DWORD)
    return checked(open_desktop(os.environ["FORGE_NATIVE_DESKTOP"], 0, True, 0x01FF),
                   "OpenDesktopW(inheritable owned desktop)")


def ordinary_shell_token(owner: object) -> object | None:
    """Use only an existing same-user/session ordinary shell token; never borrow another identity."""
    shell = api(U, "GetShellWindow", w.HWND)
    window_pid = api(U, "GetWindowThreadProcessId", w.DWORD, w.HWND, c.POINTER(w.DWORD))
    open_process = api(K, "OpenProcess", w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
    duplicate = api(A, "DuplicateTokenEx", w.BOOL, w.HANDLE, w.DWORD, c.c_void_p,
                    c.c_int, c.c_int, c.POINTER(w.HANDLE))
    pid, original, selected = w.DWORD(), w.HANDLE(), w.HANDLE()
    window = shell()
    if not window:
        logging.info("No existing shell window; ordinary token unavailable")
        return None
    window_pid(window, c.byref(pid))
    process = open_process(0x1000, False, pid.value)
    if not process:
        logging.info("Shell process token query unavailable error=%d", c.get_last_error())
        return None
    try:
        checked(open_token(process, 0x000A, c.byref(original)), "OpenProcessToken(shell query/duplicate)")
        owner_user, shell_user = token_buffer(owner, 1), token_buffer(original, 1)
        owner_sid, shell_sid = SidAttributes.from_buffer(owner_user), SidAttributes.from_buffer(shell_user)
        if not equal_sid(owner_sid.sid, shell_sid.sid):
            raise RuntimeError("Existing shell belongs to another user; refusing token")
        if w.DWORD.from_buffer(token_buffer(owner, 12)).value != w.DWORD.from_buffer(token_buffer(original, 12)).value:
            raise RuntimeError("Existing shell belongs to another session; refusing token")
        if elevated(original):
            logging.info("Existing same-user shell is elevated; ordinary token unavailable")
            return None
        require_standard_token(original)
        checked(duplicate(original, 0x018B, None, 2, 1, c.byref(selected)), "DuplicateTokenEx(ordinary shell)")
        require_standard_token(selected)
        logging.info("Selected existing same-user/session ordinary non-elevated shell token")
        result, selected = selected, w.HANDLE()
        return result
    finally:
        for handle in (selected, original):
            if handle.value:
                checked(close(handle), "CloseHandle(shell token)")
        checked(close(process), "CloseHandle(shell process query)")


def normalize_child_default_dacl(token: object) -> None:
    """Grant the child user access to its own newly-created objects, preserving existing ACEs."""
    class Trustee(c.Structure):
        _fields_ = [("multiple", c.c_void_p), ("operation", c.c_int), ("form", c.c_int),
                    ("kind", c.c_int), ("name", c.c_void_p)]

    class Entry(c.Structure):
        _fields_ = [("permissions", w.DWORD), ("mode", c.c_int), ("inherit", w.DWORD), ("trustee", Trustee)]

    user = token_buffer(token, 1)
    identity = SidAttributes.from_buffer(user)
    checked(valid_sid(identity.sid), "IsValidSid(child user)")
    old = token_buffer(token, 6)
    acl = c.c_void_p.from_buffer(old)
    if not acl.value:
        raise RuntimeError("Child token has no bounded existing default DACL")
    entry = Entry(0x10000000, 1, 0, Trustee(None, 0, 0, 1, identity.sid))  # GENERIC_ALL, GRANT_ACCESS, SID/user.
    merge = api(A, "SetEntriesInAclW", w.DWORD, w.DWORD, c.POINTER(Entry), c.c_void_p, c.POINTER(c.c_void_p))
    free = api(K, "LocalFree", c.c_void_p, c.c_void_p)
    result = c.c_void_p()
    error = merge(1, c.byref(entry), acl, c.byref(result))
    if error:
        raise OSError(error, "SetEntriesInAclW(child default DACL) failed")
    try:
        checked(set_token(token, 6, c.byref(result), c.sizeof(result)), "SetTokenInformation(child default DACL)")
    finally:
        if free(result):
            raise OSError(c.get_last_error(), "LocalFree(child default DACL) failed")


def restricted_medium_token(token: object) -> object:
    """Restrict only a new child token; never change caller privileges, UAC or accounts."""
    restricted = w.HANDLE()
    # LUA filtering removes administrator rights without deleting normal-user privileges.
    checked(restrict_token(token, 4, 0, None, 0, None, 0, None, c.byref(restricted)),
            "CreateRestrictedToken(LUA_TOKEN)")
    try:
        medium = sid(67)  # WinMediumLabelSid, S-1-16-8192.
        label = SidAttributes(c.cast(medium, c.c_void_p), 0x20)  # SE_GROUP_INTEGRITY.
        checked(set_token(restricted, 25, c.byref(label), c.sizeof(label) + sid_length(medium)),
                "SetTokenInformation(child Medium integrity)")
        require_standard_token(restricted)
        normalize_child_default_dacl(restricted)
        return restricted
    except BaseException:
        checked(close(restricted), "CloseHandle(restricted token)")
        raise


def require_worker_isolation() -> None:
    """Refuse direct worker entry outside the owner's job or with an elevated token."""
    member, token = w.BOOL(), w.HANDLE()
    checked(process_in_job(current(), None, c.byref(member)), "IsProcessInJob")
    if not member.value:
        raise RuntimeError("Windows native worker is not contained in a Job Object")
    get_thread = api(K, "GetCurrentThreadId", w.DWORD)
    get_desktop = api(U, "GetThreadDesktop", w.HANDLE, w.DWORD)
    get_name = api(U, "GetUserObjectInformationW", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
    name, size = c.create_unicode_buffer(256), w.DWORD()
    checked(get_name(get_desktop(get_thread()), 2, name, c.sizeof(name), c.byref(size)), "GetUserObjectInformationW(desktop)")
    if name.value != os.environ.get("FORGE_NATIVE_DESKTOP") or not name.value.startswith("NumeraNative-"):
        raise RuntimeError("Native worker is not on the owner's private desktop")
    checked(open_token(current(), 8, c.byref(token)), "OpenProcessToken")
    try:
        require_standard_token(token)
        open_process = api(K, "OpenProcess", w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
        for access in (0x40, 0x1000):
            process = open_process(access, False, os.getpid())
            logging.info("Worker self-process access mask=%#x allowed=%s error=%d", access,
                         bool(process), 0 if process else c.get_last_error())
            if not process:
                raise OSError(c.get_last_error(), "Native worker cannot access its own process")
            checked(close(process), "CloseHandle(worker self access)")
    finally:
        checked(close(token), "CloseHandle(token)")


class Owner:
    """Acquire desktop/job before suspended launch; release all handles on every exit."""

    def __init__(self, command: list[str], environment: dict[str, str], work: Path) -> None:
        self.job = self.desktop = None
        self.info = Process()
        self.assigned = False
        self.pid = 0
        self.started = time.monotonic()
        self.desktop_name = f"NumeraNative-{os.getpid()}-{time.monotonic_ns()}"
        token, linked, child_token = w.HANDLE(), w.HANDLE(), w.HANDLE()
        streams = []
        try:
            self.desktop = checked(create_desktop(self.desktop_name, None, None, 0, 0x01FF, None), "CreateDesktopW")
            self.job = checked(create_job(None, None), "CreateJobObjectW")
            limits = ExtendedLimits()
            limits.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE; never allow breakaway.
            checked(set_job(self.job, 9, c.byref(limits), c.sizeof(limits)), "SetInformationJobObject")
            checked(open_token(current(), 0x018B, c.byref(token)), "OpenProcessToken(query/duplicate/assign/default/session)")
            startup = Startup()
            startup.cb = c.sizeof(startup)
            station = api(U, "GetProcessWindowStation", w.HANDLE)
            get_name = api(U, "GetUserObjectInformationW", w.BOOL, w.HANDLE, c.c_int,
                           c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
            station_name, size = c.create_unicode_buffer(256), w.DWORD()
            checked(get_name(station(), 2, station_name, c.sizeof(station_name), c.byref(size)),
                    "GetUserObjectInformationW(window station)")
            startup.desktop = f"{station_name.value}\\{self.desktop_name}"
            logging.info("Worker destination window-station=%s private-desktop=%s", station_name.value, self.desktop_name)
            startup.flags = 1 | 0x100  # USESHOWWINDOW | USESTDHANDLES; never inherit CI console.
            startup.show = 0
            import msvcrt

            for attribute, path, mode in [("stdin", "NUL", "rb"),
                                          ("stdout", work / "worker-stdout.log", "xb"),
                                          ("stderr", work / "worker-stderr.log", "xb")]:
                stream = open(path, mode)
                streams.append(stream)
                handle = msvcrt.get_osfhandle(stream.fileno())
                os.set_handle_inheritable(handle, True)
                setattr(startup, attribute, handle)
            environment = {**environment, "FORGE_NATIVE_DESKTOP": self.desktop_name}
            block = c.create_unicode_buffer("\0".join(f"{key}={value}" for key, value in sorted(environment.items(), key=lambda item: item[0].upper())) + "\0\0")
            line = c.create_unicode_buffer(subprocess.list2cmdline(command))
            flags = 0x00000004 | 0x00000400 | 0x08000000  # SUSPENDED, UNICODE_ENVIRONMENT, NO_WINDOW.
            if elevated(token):
                size = w.DWORD()
                if not token_info(token, 19, c.byref(linked), c.sizeof(linked), c.byref(size)):
                    error = c.get_last_error()
                    if error != 1312:
                        raise OSError(error, "TokenLinkedToken failed")
                    ordinary = ordinary_shell_token(token)
                    if ordinary is None:
                        linked = restricted_medium_token(token)
                        logging.info("Missing ordinary token; limited-user duplicate verified non-elevated/Medium-or-lower/no enabled Administrators SID")
                    else:
                        linked = ordinary
                require_standard_token(linked)
                if not create_with_token(linked, 0, command[0], line, flags, block, str(work),
                                         c.byref(startup), c.byref(self.info)):
                    error = c.get_last_error()
                    logging.error("CreateProcessWithTokenW failed error=%d before child spawn", error)
                    launch_error = OSError(error, "CreateProcessWithTokenW(unelevated) failed")
                    try:
                        diagnostics.diagnose_launch_access(sys.modules[__name__], linked, command, work, self.desktop_name)
                    except BaseException as diagnostic_error:
                        raise diagnostic_error from launch_error
                    raise launch_error
            else:
                require_standard_token(token)
                checked(create_process(command[0], line, None, None, True, flags, block, str(work),
                                       c.byref(startup), c.byref(self.info)), "CreateProcessW")
            self.pid = self.info.pid
            checked(assign_job(self.job, self.info.process), "AssignProcessToJobObject(suspended)")
            self.assigned = True
            checked(open_token(self.info.process, 8, c.byref(child_token)), "OpenProcessToken(suspended worker)")
            require_standard_token(child_token)
            logging.info("Suspended worker token verified non-elevated/Medium-or-lower/no enabled Administrators SID; Job assigned before resume")
            suspended_count = resume(self.info.thread)
            logging.info("Worker ResumeThread previous suspend count=%d", suspended_count)
            if suspended_count == 0xFFFFFFFF:
                raise OSError(c.get_last_error(), "ResumeThread failed")
            if suspended_count != 1:
                raise RuntimeError(f"Worker thread had unexpected suspend count {suspended_count}")
        except BaseException:
            self.stop()
            raise
        finally:
            try:
                for stream in streams:
                    stream.close()
                for handle in (child_token, linked, token):
                    if handle.value:
                        checked(close(handle), "CloseHandle(token)")
            except OSError:
                self.stop()
                raise

    def poll(self) -> int | None:
        state = wait_process(self.info.process, 0)
        if state == 0x102:
            return None
        if state != 0:
            raise OSError(c.get_last_error(), "WaitForSingleObject failed")
        code = w.DWORD()
        checked(exit_code(self.info.process, c.byref(code)), "GetExitCodeProcess")
        return code.value

    def active(self) -> int:
        accounting = Accounting()
        checked(query_job(self.job, 1, c.byref(accounting), c.sizeof(accounting), None), "QueryInformationJobObject")
        return accounting.active

    def stop(self) -> None:
        """Kill only handle-owned processes; verify active count zero before desktop close."""
        errors = []
        try:
            if self.job and self.assigned:
                checked(kill_job(self.job, 1), "TerminateJobObject")
                deadline = time.monotonic() + 10
                while self.active() and time.monotonic() < deadline:
                    time.sleep(0.05)
                if self.active():
                    raise RuntimeError("Native Windows cleanup could not prove zero owned processes")
            elif self.info.process:
                checked(kill_process(self.info.process, 1), "TerminateProcess(suspended)")
            if self.info.process and wait_process(self.info.process, 5000) != 0:
                raise RuntimeError("Native Windows worker did not terminate within cleanup deadline")
        except BaseException as error:
            errors.append(error)
        finally:
            for handle in (self.info.thread, self.info.process, self.job):
                if handle:
                    try:
                        checked(close(handle), "CloseHandle")
                    except OSError as error:
                        errors.append(error)
            self.info = Process()
            self.job = None
            if self.desktop:
                try:
                    checked(close_desktop(self.desktop), "CloseDesktop")
                except OSError as error:
                    errors.append(error)
                self.desktop = None
        if errors:
            raise RuntimeError("Windows owned process/desktop cleanup failed") from errors[0]

    def __enter__(self) -> "Owner":
        return self

    def __exit__(self, kind: object, error: object, traceback: object) -> None:
        self.stop()


def launch_probe() -> None:
    """Exercise the real owner before expensive builds, without loading a GUI or driver."""
    import sys

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise RuntimeError("Launch probe requires ephemeral GitHub-hosted Windows CI")
    root = Path(__file__).resolve().parent.parent
    work = args.work.resolve()
    if not work.is_relative_to(root / ".ci-work"):
        raise RuntimeError("Launch probe work must be project-owned")
    work.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y-%m-%d_%H-%M-%S_UTC", time.gmtime())
    handler = logging.FileHandler(work / f"native-launch_{stamp}_{os.getpid()}.log", mode="x", encoding="utf-8")
    handler.setFormatter(logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [NATIVE-LAUNCH] %(message)s", "%Y-%m-%d %H:%M:%S"))
    logging.Formatter.converter = time.gmtime
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        if args.worker:
            require_worker_isolation()
            (work / "worker.json").write_text(json.dumps({"status": "isolated-worker-started"}), encoding="utf-8")
            return
        allowed = {"SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "COMSPEC", "PATH", "GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"}
        environment = {key: value for key, value in os.environ.items() if key.upper() in allowed}
        environment.update(PYTHONUTF8="1", PYTHONDONTWRITEBYTECODE="1", TEMP=str(work), TMP=str(work))
        interpreter = Path(sys.executable).with_name("pythonw.exe").resolve(strict=True)
        command = [str(interpreter), "-I", "-S", str(Path(__file__).resolve()), "--worker", "--work", str(work)]
        with Owner(command, environment, work) as owner:
            deadline = time.monotonic() + 15
            while (result := owner.poll()) is None:
                if time.monotonic() >= deadline:
                    diagnostics.diagnose_startup(sys.modules[__name__], owner)
                    raise TimeoutError("Native launch probe exceeded 15s")
                time.sleep(0.05)
            if result != 0 or json.loads((work / "worker.json").read_text(encoding="utf-8")) != {"status": "isolated-worker-started"}:
                raise RuntimeError(f"Native launch probe worker failed exit={result}")
        logging.info("Isolated non-elevated worker executed; owned Job active=0 and desktop closed")
    except BaseException:
        logging.exception("Native launch probe failed")
        raise
    finally:
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    launch_probe()
