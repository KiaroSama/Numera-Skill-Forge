"""Bounded read-only diagnostics for the owned Windows native smoke process tree."""

import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
import logging
import os

def diagnose_launch_access(windows: object, token: object, command: list[str], work: Path, desktop: str) -> None:
    """Probe access without changing ACLs or executing a child; always revert the thread."""
    K, U, A, api, checked = windows.K, windows.U, windows.A, windows.api, windows.checked
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
    close = windows.close
    close_desktop = windows.close_desktop
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


def diagnose_thread_wait(windows: object, thread_id: int) -> None:
    """Read only the owned thread's first wait boundary; never enable debug privileges."""
    K, A, api, checked = windows.K, windows.A, windows.api, windows.checked
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
                open_process = api(K, "OpenProcess", w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
                query_image = api(K, "QueryFullProcessImageNameW", w.BOOL, w.HANDLE, w.DWORD,
                                  w.LPWSTR, c.POINTER(w.DWORD))
                session_id = api(K, "ProcessIdToSessionId", w.BOOL, w.DWORD, c.POINTER(w.DWORD))
                handle = open_process(0x1000, False, node.detail.thread.pid)
                image, length, endpoint_session = c.create_unicode_buffer(1024), w.DWORD(1024), w.DWORD()
                session_id(node.detail.thread.pid, c.byref(endpoint_session))
                if handle:
                    try:
                        if query_image(handle, 0, image, c.byref(length)):
                            logging.error("Wait endpoint image=%s session=%d", Path(image.value).name, endpoint_session.value)
                    finally:
                        checked(windows.close(handle), "CloseHandle(wait endpoint query)")
            else:
                logging.error("Wait-chain object type=%d status=%d", node.kind, node.status)
    finally:
        close_session(session)


def diagnose_owned_processes(windows: object, self) -> None:
    """Read token and desktop boundaries only for processes recorded in this owned Job."""
    K, U, api, checked = windows.K, windows.U, windows.api, windows.checked
    query_job, open_token, close = windows.query_job, windows.open_token, windows.close
    token_buffer, SidAttributes = windows.token_buffer, windows.SidAttributes
    sid_count, sid_part, elevated = windows.sid_count, windows.sid_part, windows.elevated
    class ProcessIds(c.Structure):
        _fields_ = [("assigned", w.DWORD), ("amount", w.DWORD), ("pids", c.c_size_t * 64)]

    class ThreadEntry(c.Structure):
        _fields_ = [("size", w.DWORD), ("usage", w.DWORD), ("tid", w.DWORD),
                    ("pid", w.DWORD), ("base", c.c_long), ("delta", c.c_long), ("flags", w.DWORD)]

    ids = ProcessIds()
    checked(query_job(self.job, 3, c.byref(ids), c.sizeof(ids), None), "QueryInformationJobObject(process IDs)")
    selected = set(ids.pids[:min(ids.amount, 64)])
    tcp = c.WinDLL("iphlpapi", use_last_error=True)
    table = api(tcp, "GetExtendedTcpTable", w.DWORD, c.c_void_p, c.POINTER(w.DWORD),
                w.BOOL, w.ULONG, c.c_int, w.ULONG)
    size, data = w.DWORD(262144), c.create_string_buffer(262144)
    error = table(data, c.byref(size), False, 2, 3, 0)
    if error == 0:
        amount = w.DWORD.from_buffer(data).value
        if 4 + amount * 24 <= c.sizeof(data):
            for index in range(amount):
                row = (w.DWORD * 6).from_buffer(data, 4 + index * 24)
                if row[5] in selected:
                    port = int.from_bytes(int(row[2]).to_bytes(4, "little")[:2], "big")
                    logging.info("Owned TCP listener pid=%d port=%d loopback=%s", row[5], port, row[1] == 0x0100007F)
    else:
        logging.warning("Owned TCP listener inspection unavailable error=%d", error)
    class UnicodeString(c.Structure):
        _fields_ = [("length", w.USHORT), ("maximum", w.USHORT), ("buffer", c.c_void_p)]

    nt = c.WinDLL("ntdll", use_last_error=True)
    query_command = api(nt, "NtQueryInformationProcess", c.c_long, w.HANDLE, c.c_int,
                        c.c_void_p, w.ULONG, c.POINTER(w.ULONG))
    open_process = api(K, "OpenProcess", w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
    query_image = api(K, "QueryFullProcessImageNameW", w.BOOL, w.HANDLE, w.DWORD, w.LPWSTR, c.POINTER(w.DWORD))
    snapshot = api(K, "CreateToolhelp32Snapshot", w.HANDLE, w.DWORD, w.DWORD)
    first = api(K, "Thread32First", w.BOOL, w.HANDLE, c.POINTER(ThreadEntry))
    following = api(K, "Thread32Next", w.BOOL, w.HANDLE, c.POINTER(ThreadEntry))
    desktop = api(U, "GetThreadDesktop", w.HANDLE, w.DWORD)
    get_name = api(U, "GetUserObjectInformationW", w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
    for pid in selected:
        process = open_process(0x1000, False, pid)
        if not process:
            continue
        token, image, length = w.HANDLE(), c.create_unicode_buffer(1024), w.DWORD(1024)
        try:
            if query_image(process, 0, image, c.byref(length)) and open_token(process, 8, c.byref(token)):
                integrity = token_buffer(token, 25)
                label = SidAttributes.from_buffer(integrity)
                count = sid_count(label.sid)[0]
                logging.info("Owned process pid=%d image=%s elevated=%s integrity=%d", pid,
                             Path(image.value).name, elevated(token), sid_part(label.sid, count - 1)[0])
                buffer, required = c.create_string_buffer(65536), w.ULONG()
                status = query_command(process, 60, buffer, c.sizeof(buffer), c.byref(required))
                if status >= 0:
                    command = UnicodeString.from_buffer(buffer)
                    start, end = c.addressof(buffer), c.addressof(buffer) + c.sizeof(buffer)
                    if command.length <= 32768 and start <= command.buffer <= end - command.length:
                        arguments = c.wstring_at(command.buffer, command.length // 2)
                        import re
                        remote = re.search(r"--remote-debugging-port=(\d+)", arguments)
                        logging.info("Owned argument flags pid=%d debug=%s port=%s automation=%s renderer=%s gpu=%s", pid,
                                     remote is not None, remote.group(1) if remote else "none", "--enable-automation" in arguments,
                                     "--type=renderer" in arguments, "--type=gpu-process" in arguments)
        finally:
            if token.value:
                checked(close(token), "CloseHandle(owned query token)")
            checked(close(process), "CloseHandle(owned query process)")
    handle = snapshot(4, 0)
    if handle == c.c_void_p(-1).value:
        logging.warning("Owned thread snapshot unavailable error=%d", c.get_last_error())
        return
    try:
        entry = ThreadEntry()
        entry.size = c.sizeof(entry)
        found, observed = first(handle, c.byref(entry)), set()
        while found:
            if entry.pid in selected and entry.pid not in observed:
                name, size = c.create_unicode_buffer(256), w.DWORD()
                if get_name(desktop(entry.tid), 2, name, c.sizeof(name), c.byref(size)):
                    logging.info("Owned process pid=%d first-thread desktop=%s", entry.pid, name.value)
                    observed.add(entry.pid)
            found = following(handle, c.byref(entry))
    finally:
        checked(close(handle), "CloseHandle(owned thread snapshot)")

def diagnose_startup(windows: object, self) -> None:
    """Inspect only window captions on this owned desktop; never send UI messages."""
    U, api = windows.U, windows.api
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
    diagnose_thread_wait(windows, self.info.tid)

