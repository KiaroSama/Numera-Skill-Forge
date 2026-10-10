"""Win32 owner for a CI-only, never-switched desktop and kill-on-close process tree."""

import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
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


def checked(value: object, operation: str) -> object:
    if not value:
        raise OSError(c.get_last_error(), f"{operation} failed; hidden owned Windows automation unavailable")
    return value


def elevated(token: object) -> bool:
    value, size = w.DWORD(), w.DWORD()
    checked(token_info(token, 20, c.byref(value), c.sizeof(value), c.byref(size)), "TokenElevation")
    return bool(value.value)


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
        if elevated(token):
            raise RuntimeError("WebView2 automation requires an unelevated worker")
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
        token, linked = w.HANDLE(), w.HANDLE()
        try:
            self.desktop = checked(create_desktop(self.desktop_name, None, None, 0, 0x01FF, None), "CreateDesktopW")
            self.job = checked(create_job(None, None), "CreateJobObjectW")
            limits = ExtendedLimits()
            limits.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE; never allow breakaway.
            checked(set_job(self.job, 9, c.byref(limits), c.sizeof(limits)), "SetInformationJobObject")
            checked(open_token(current(), 0x000A, c.byref(token)), "OpenProcessToken")
            startup = Startup()
            startup.cb = c.sizeof(startup)
            startup.desktop = self.desktop_name
            startup.flags = 1  # STARTF_USESHOWWINDOW, SW_HIDE.
            startup.show = 0
            environment = {**environment, "FORGE_NATIVE_DESKTOP": self.desktop_name}
            block = c.create_unicode_buffer("\0".join(f"{key}={value}" for key, value in sorted(environment.items(), key=lambda item: item[0].upper())) + "\0\0")
            line = c.create_unicode_buffer(subprocess.list2cmdline(command))
            flags = 0x00000004 | 0x00000400 | 0x08000000  # SUSPENDED, UNICODE_ENVIRONMENT, NO_WINDOW.
            if elevated(token):
                size = w.DWORD()
                checked(token_info(token, 19, c.byref(linked), c.sizeof(linked), c.byref(size)), "TokenLinkedToken")
                if elevated(linked):
                    raise RuntimeError("Existing linked token is elevated; refusing WebView2 launch")
                checked(create_with_token(linked, 0, command[0], line, flags, block, str(work),
                                          c.byref(startup), c.byref(self.info)), "CreateProcessWithTokenW(unelevated)")
            else:
                checked(create_process(command[0], line, None, None, False, flags, block, str(work),
                                       c.byref(startup), c.byref(self.info)), "CreateProcessW")
            self.pid = self.info.pid
            checked(assign_job(self.job, self.info.process), "AssignProcessToJobObject(suspended)")
            self.assigned = True
            if resume(self.info.thread) == 0xFFFFFFFF:
                raise OSError(c.get_last_error(), "ResumeThread failed")
        except BaseException:
            self.stop()
            raise
        finally:
            try:
                for handle in (linked, token):
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
