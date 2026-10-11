"""Owned Linux integration processes and authenticated smart-Git fixture; never a product server."""

from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
import logging
import os
from pathlib import Path
import selectors
import signal
import ssl
import subprocess
import threading
import time
from typing import Iterator
from urllib.parse import urlsplit

LIMIT = 4 * 1024 * 1024


class Processes:
    def __init__(self, home: Path, seconds: int = 150) -> None:
        self.home = home
        self.deadline = time.monotonic() + seconds
        self.children: list[subprocess.Popen[bytes]] = []
        self.environment = {
            "PATH": "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            "HOME": str(home), "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8",
            "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_TERMINAL_PROMPT": "0", "GIT_AUTHOR_NAME": "Fixture",
            "GIT_COMMITTER_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "Kiaro.Sama.Dev@gmail.com",
            "GIT_COMMITTER_EMAIL": "Kiaro.Sama.Dev@gmail.com",
        }

    def remaining(self, seconds: int = 30) -> float:
        left = min(seconds, self.deadline - time.monotonic())
        if left <= 0:
            raise TimeoutError("Transport fixture exceeded its wall ceiling")
        return left

    def start(self, arguments: list[str], environment: dict[str, str] | None = None,
              stdout: int | object = subprocess.PIPE, stderr: int | object = subprocess.PIPE) -> subprocess.Popen[bytes]:
        self.remaining()
        process = subprocess.Popen(arguments, cwd=self.home, env=self.environment | (environment or {}),
                                   stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        self.children.append(process)
        return process

    def stop(self, process: subprocess.Popen[bytes]) -> None:
        # The session belongs exclusively to this child, even when its leader exited first.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)
        if process in self.children:
            self.children.remove(process)
        for stream in (process.stdout, process.stderr):
            if stream is not None:
                stream.close()

    def run(self, arguments: list[str], environment: dict[str, str] | None = None,
            expected: int | None = 0, seconds: int = 30) -> tuple[int, bytes, bytes]:
        process = self.start(arguments, environment)
        deadline = time.monotonic() + self.remaining(seconds)
        last = time.monotonic()
        output = [bytearray(), bytearray()]
        try:
            with selectors.DefaultSelector() as selector:
                assert process.stdout is not None and process.stderr is not None
                for index, stream in enumerate((process.stdout, process.stderr)):
                    os.set_blocking(stream.fileno(), False)
                    selector.register(stream, selectors.EVENT_READ, index)
                while selector.get_map():
                    now = time.monotonic()
                    if now >= deadline or now - last >= 20:
                        raise TimeoutError("Owned fixture child exceeded wall/idle bounds")
                    for key, _ in selector.select(min(0.2, deadline - now)):
                        block = os.read(key.fileobj.fileno(), 65536)
                        if not block:
                            selector.unregister(key.fileobj)
                            continue
                        last = time.monotonic()
                        output[key.data].extend(block)
                        if sum(map(len, output)) > LIMIT:
                            raise RuntimeError("Owned fixture child output exceeded limit")
            code = process.wait(timeout=max(0.1, deadline - time.monotonic()))
            if expected is not None and code != expected:
                raise RuntimeError(f"Fixture prerequisite or operation failed: {Path(arguments[0]).name} exit={code}")
            return code, bytes(output[0]), bytes(output[1])
        finally:
            self.stop(process)

    def close(self) -> None:
        errors = []
        for process in reversed(self.children):
            try:
                self.stop(process)
            except (OSError, subprocess.TimeoutExpired) as error:
                errors.append(type(error).__name__)
        if errors:
            raise RuntimeError("Owned process cleanup failed: " + ",".join(errors))
        logging.info("Owned process groups closed count=%d", len(self.children))


@contextmanager
def https_repository(root: Path, certificate: Path, key: Path, authorization: str,
                     environment: dict[str, str]) -> Iterator[dict[str, int]]:
    stats = {"authorized": 0, "rejected": 0, "upload": 0, "redirect": 0, "handshake_timeout": 0}

    class Handler(BaseHTTPRequestHandler):
        timeout = 10

        def log_message(self, format: str, *args: object) -> None:
            pass

        def do_GET(self) -> None:
            self.request_git()

        def do_POST(self) -> None:
            self.request_git()

        def request_git(self) -> None:
            self.connection.settimeout(10)
            selected = urlsplit(self.path)
            if selected.path == "/redirect/repo.git/info/refs":
                stats["redirect"] += 1
                self.send_response(302)
                self.send_header("Location", "https://127.0.0.1/private/repo.git/info/refs?service=git-upload-pack")
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            if (self.headers.get("Authorization") != authorization or
                    selected.path not in ("/private/repo.git/info/refs", "/private/repo.git/git-upload-pack")):
                stats["rejected"] += 1
                self.send_response(401)
                self.send_header("WWW-Authenticate", 'Basic realm="owned-fixture"')
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            if (self.command == "GET" and selected.query != "service=git-upload-pack") or (
                    self.command == "POST" and selected.query):
                self.send_error(403)
                return
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 <= length <= LIMIT:
                self.send_error(413)
                return
            body = self.rfile.read(length)
            if len(body) != length:
                raise RuntimeError("Truncated Git request")
            stats["authorized"] += 1
            stats["upload"] += int(self.command == "POST")
            env = environment | {
                "GIT_PROJECT_ROOT": str(root), "GIT_HTTP_EXPORT_ALL": "1",
                "PATH_INFO": selected.path.removeprefix("/private"),
                "QUERY_STRING": selected.query, "REQUEST_METHOD": self.command,
                "CONTENT_TYPE": self.headers.get("Content-Type", ""),
                "CONTENT_LENGTH": str(length), "REMOTE_ADDR": "127.0.0.1",
                "HTTP_GIT_PROTOCOL": self.headers.get("Git-Protocol", ""),
            }
            process = subprocess.Popen(["git", "-c", "core.hooksPath=", "http-backend"], env=env,
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       start_new_session=True)
            try:
                response, _ = process.communicate(body, timeout=15)
                if process.returncode != 0 or len(response) > LIMIT:
                    raise RuntimeError("Git backend failed or exceeded fixture output bound")
            finally:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=5)
            headers, separator, payload = response.partition(b"\r\n\r\n")
            if not separator:
                headers, separator, payload = response.partition(b"\n\n")
            if not separator:
                raise RuntimeError("Invalid Git CGI response")
            fields = [line.decode("ascii").split(":", 1) for line in headers.splitlines()]
            status = next((int(value.strip().split()[0]) for name, value in fields if name.lower() == "status"), 200)
            self.send_response(status)
            for name, value in fields:
                if name.lower() not in ("status", "content-length"):
                    self.send_header(name, value.strip())
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(certificate, key)

    class TlsServer(HTTPServer):
        def get_request(self) -> tuple[ssl.SSLSocket, tuple[str, int]]:
            connection, address = self.socket.accept()
            connection.settimeout(2)
            try:
                return context.wrap_socket(connection, server_side=True), address
            except TimeoutError:
                stats["handshake_timeout"] += 1
                connection.close()
                raise
            except BaseException:
                connection.close()
                raise

    with TlsServer(("127.0.0.1", 443), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.1}, daemon=False)
        thread.start()
        logging.info("Loopback authenticated TLS smart-Git server started")
        try:
            yield stats
        finally:
            server.shutdown()
            thread.join(timeout=20)
            if thread.is_alive():
                raise RuntimeError("Owned HTTPS server thread did not terminate")
            logging.info("Loopback HTTPS server closed authorized=%d rejected=%d upload=%d", stats["authorized"], stats["rejected"], stats["upload"])
