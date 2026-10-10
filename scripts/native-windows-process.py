"""Win32 owner for a CI-only, never-switched desktop and kill-on-close process tree."""

if __name__ == "__main__":
    print("Native launch probe entered Python", flush=True)

import argparse
import ctypes as c
import json
from ctypes import wintypes as w
from pathlib import Path
import logging
import os
import subprocess
import time


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


def restricted_medium_token(token: object) -> object:
    """Restrict only a new child token; never change caller privileges, UAC or accounts."""
    restricted = w.HANDLE()
    checked(restrict_token(token, 1 | 4, 0, None, 0, None, 0, None, c.byref(restricted)),
            "CreateRestrictedToken(DISABLE_MAX_PRIVILEGE|LUA_TOKEN)")
    try:
        medium = sid(67)  # WinMediumLabelSid, S-1-16-8192.
        label = SidAttributes(c.cast(medium, c.c_void_p), 0x20)  # SE_GROUP_INTEGRITY.
        checked(set_token(restricted, 25, c.byref(label), c.sizeof(label) + sid_length(medium)),
                "SetTokenInformation(child Medium integrity)")
        require_standard_token(restricted)
        return restricted
    except BaseException:
        checked(close(restricted), "CloseHandle(restricted token)")
        raise


def diagnose_launch_access(token: object, command: list[str], work: Path, desktop: str) -> None:
    """Probe access without changing ACLs or executing a child; always revert the thread."""
    impersonate = api(A, "ImpersonateLoggedOnUser", w.BOOL, w.HANDLE)
    revert = api(A, "RevertToSelf", w.BOOL)
    open_file = api(K, "CreateFileW", w.HANDLE, w.LPCWSTR, w.DWORD, w.DWORD,
                    c.c_void_p, w.DWORD, w.DWORD, w.HANDLE)
    open_desktop = api(U, "OpenDesktopW", w.HANDLE, w.LPCWSTR, w.DWORD, w.BOOL, w.DWORD)
    station = api(U, "GetProcessWindowStation", w.HANDLE)
    get_name = api(U, "GetUserObjectInformationW", w.BOOL, w.HANDLE, c.c_int,
                   c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
    open_station = api(U, "OpenWindowStationW", w.HANDLE, w.LPCWSTR, w.BOOL, w.DWORD)
    close_station = api(U, "CloseWindowStation", w.BOOL, w.HANDLE)
    name, size = c.create_unicode_buffer(256), w.DWORD()
    checked(get_name(station(), 2, name, c.sizeof(name), c.byref(size)), "Window station name")
    if not impersonate(token):
        logging.error("Launch access probe impersonation failed error=%d", c.get_last_error())
        return
    observations = []
    open_process = api(K, "OpenProcess", w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
    duplicate_token = api(A, "DuplicateTokenEx", w.BOOL, w.HANDLE, w.DWORD, c.c_void_p,
                          c.c_int, c.c_int, c.POINTER(w.HANDLE))
    try:
        for label, access in [("caller-process-duplicate-handle", 0x40),
                              ("caller-process-query-limited", 0x1000)]:
            handle = open_process(access, False, os.getpid())
            observations.append((label, access, 0 if handle else c.get_last_error()))
            if handle:
                checked(close(handle), "CloseHandle(caller access probe)")
        duplicate = w.HANDLE()
        result = duplicate_token(token, 0x000B, None, 2, 1, c.byref(duplicate))
        observations.append(("candidate-token-duplicate-primary", 0x000B, 0 if result else c.get_last_error()))
        if result:
            checked(close(duplicate), "CloseHandle(duplicate access probe)")
        # Opening existing local objects only; no reads, writes, process or desktop switching.
        for label, path, access, flags in [
            ("worker-executable-read-execute", command[0], 0xA0000000, 0),
            ("worker-script-read", command[1], 0x80000000, 0),
            ("working-directory-read-traverse", str(work), 0xA0000000, 0x02000000),
        ]:
            handle = open_file(path, access, 7, None, 3, flags, None)
            error = c.get_last_error() if handle == c.c_void_p(-1).value else 0
            observations.append((label, access, error))
            if not error:
                checked(close(handle), "CloseHandle(access probe)")
        for label, function, arguments, release in [
            ("private-desktop-specific-all", open_desktop, (desktop, 0, False, 0x01FF), close_desktop),
            ("window-station-read-create", open_station, (name.value, False, 0x000B), close_station),
        ]:
            handle = function(*arguments)
            observations.append((label, arguments[-1], 0 if handle else c.get_last_error()))
            if handle:
                checked(release(handle), "Close user object(access probe)")
    finally:
        checked(revert(), "RevertToSelf(access probe)")
    for label, access, error in observations:
        logging.error("Launch access probe object=%s mask=%#x result=%s error=%d", label, access,
                      "failed" if error else "allowed", error)


def diagnose_thread_wait(thread_id: int) -> None:
    """Read only the owned thread's first wait boundary; never enable debug privileges."""
    class Lock(c.Structure):
        _fields_ = [("name", w.WCHAR * 128), ("timeout", c.c_longlong), ("alertable", w.BOOL)]

    class Thread(c.Structure):
        _fields_ = [("pid", w.DWORD), ("tid", w.DWORD), ("wait", w.DWORD), ("switches", w.DWORD)]

    class Detail(c.Union):
        _fields_ = [("lock", Lock), ("thread", Thread)]

    class Node(c.Structure):
        _fields_ = [("kind", c.c_int), ("status", c.c_int), ("detail", Detail)]

    open_session = api(A, "OpenThreadWaitChainSession", w.HANDLE, w.DWORD, c.c_void_p)
    close_session = api(A, "CloseThreadWaitChainSession", None, w.HANDLE)
    get_chain = api(A, "GetThreadWaitChain", w.BOOL, w.HANDLE, c.c_size_t, w.DWORD,
                    w.DWORD, c.POINTER(w.DWORD), c.POINTER(Node), c.POINTER(w.BOOL))
    session = open_session(0, None)
    if not session:
        logging.error("Wait-chain session unavailable error=%d", c.get_last_error())
        return
    try:
        nodes, count, cycle = (Node * 16)(), w.DWORD(16), w.BOOL()
        if not get_chain(session, 0, 0, thread_id, c.byref(count), nodes, c.byref(cycle)):
            logging.error("Owned thread wait-chain unavailable error=%d", c.get_last_error())
            return
        logging.error("Owned thread wait-chain count=%d cycle=%s", count.value, bool(cycle.value))
        for node in nodes[:min(count.value, 16)]:
            if node.kind == 8:  # WctThreadType.
                logging.error("Wait-chain thread pid=%d tid=%d status=%d wait=%d switches=%d",
                              node.detail.thread.pid, node.detail.thread.tid, node.status,
                              node.detail.thread.wait, node.detail.thread.switches)
            else:
                logging.error("Wait-chain object type=%d status=%d", node.kind, node.status)
    finally:
        close_session(session)


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
                    linked = restricted_medium_token(token)
                    logging.info("Missing linked token; restricted duplicate verified non-elevated/Medium-or-lower/no enabled Administrators SID")
                require_standard_token(linked)
                if not create_with_token(linked, 0, command[0], line, flags, block, str(work),
                                         c.byref(startup), c.byref(self.info)):
                    error = c.get_last_error()
                    logging.error("CreateProcessWithTokenW failed error=%d before child spawn", error)
                    launch_error = OSError(error, "CreateProcessWithTokenW(unelevated) failed")
                    try:
                        diagnose_launch_access(linked, command, work, self.desktop_name)
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

    def diagnose_startup(self) -> None:
        """Inspect only window captions on this owned desktop; never send UI messages."""
        callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
        enum_windows = api(U, "EnumDesktopWindows", w.BOOL, w.HANDLE, callback_type, w.LPARAM)
        window_pid = api(U, "GetWindowThreadProcessId", w.DWORD, w.HWND, c.POINTER(w.DWORD))
        window_text = api(U, "GetWindowTextW", c.c_int, w.HWND, w.LPWSTR, c.c_int)
        amount = 0

        @callback_type
        def inspect(handle: object, unused: int) -> bool:
            nonlocal amount
            amount += 1
            pid, title = w.DWORD(), c.create_unicode_buffer(512)
            window_pid(handle, c.byref(pid))
            # GetWindowText does not send messages to windows in another process.
            if pid.value != os.getpid():
                window_text(handle, title, len(title))
                logging.error("Owned desktop window pid=%d caption=%s", pid.value, title.value)
            return amount < 32

        enum_windows(self.desktop, inspect, 0)
        logging.error("Startup timeout owned desktop windows=%d Job active=%d", amount, self.active())
        diagnose_thread_wait(self.info.tid)

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
        command = [sys.executable, "-I", "-S", str(Path(__file__).resolve()), "--worker", "--work", str(work)]
        with Owner(command, environment, work) as owner:
            deadline = time.monotonic() + 15
            while (result := owner.poll()) is None:
                if time.monotonic() >= deadline:
                    owner.diagnose_startup()
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
