#!/usr/bin/env python3
"""Hosted-only real CLI acquisition through owned TLS/auth and pinned-agent SSH transports."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import logging
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import time

from source_transport_fixture import Processes, https_repository

PAYLOAD = b"---\nname: transport-demo\ndescription: First-party protocol fixture\n---\nRead-only harmless instructions.\n"
NOTICE = b"MIT License\n\nCopyright (c) 2026 Numera Skill Forge contributors\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the Software), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED AS IS, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n"


def write(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")


def wait_listener(port: int, process: object, owner: Processes) -> None:
    deadline = time.monotonic() + owner.remaining(10)
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Owned SSH server stopped before readiness")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(min(0.05, max(0, deadline - time.monotonic())))
    raise TimeoutError("Owned SSH server readiness deadline expired")


def verify(application: Path, work: Path) -> None:
    root = work / "run"
    if root.exists():
        raise RuntimeError("Fresh transport fixture directory already exists")
    root.mkdir(mode=0o700)
    home = root / "home"
    home.mkdir(mode=0o700)
    owner = Processes(home)
    cli_roots: list[Path] = []
    auth = "Bearer first-party-public-test-value"
    previous_handler = signal.signal(signal.SIGTERM, lambda signum, frame: (_ for _ in ()).throw(TimeoutError("Fixture cancelled")))
    try:
        owner.run(["ip", "link", "set", "lo", "up"])
        # All mounts are private to the workflow's unshare mount namespace.
        ca = root / "ca.crt"
        ca_key = root / "ca.key"
        key = root / "server.key"
        certificate = root / "server.crt"
        csr = root / "server.csr"
        extensions = root / "server.ext"
        extensions.write_text("subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n", encoding="utf-8")
        owner.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=Owned transport fixture", "-keyout", str(ca_key), "-out", str(ca)])
        owner.run(["openssl", "req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=127.0.0.1", "-keyout", str(key), "-out", str(csr)])
        owner.run(["openssl", "x509", "-req", "-in", str(csr), "-CA", str(ca), "-CAkey", str(ca_key), "-CAcreateserial", "-days", "1", "-extfile", str(extensions), "-out", str(certificate)])
        bundle = root / "ca-bundle.crt"
        trust = Path("/etc/ssl/certs/ca-certificates.crt")
        bundle.write_bytes(trust.read_bytes() + b"\n" + ca.read_bytes())
        owner.run(["mount", "--bind", str(bundle), str(trust)])

        repository = root / "repo.git"
        owner.run(["git", "init", "-b", "main", str(repository)])
        (repository / "skills/demo").mkdir(parents=True)
        (repository / "skills/demo/SKILL.md").write_bytes(PAYLOAD)
        (repository / "LICENSE").write_bytes(NOTICE)
        owner.run(["git", "-C", str(repository), "add", "LICENSE", "skills"])
        owner.run(["git", "-C", str(repository), "commit", "-m", "fixture"])
        owner.run(["git", "-C", str(repository), "config", "http.receivepack", "false"])
        initial = owner.run(["git", "-C", str(repository), "rev-parse", "HEAD"])[1].decode("ascii").strip()

        def request(workspace: Path, payload: dict[str, object], env: dict[str, str] | None = None,
                    expected: int | None = 0, offline: bool = False) -> dict[str, object]:
            path = root / "request.json"
            write(path, payload)
            command = [str(application), "--repository", str(workspace), "--json", "--timeout", "20"]
            if offline:
                command.append("--offline")
            code, stdout, stderr = owner.run(command + ["request", str(path)], env, None, seconds=25)
            if auth.encode() in stdout + stderr:
                raise RuntimeError("CLI exposed selected authentication value")
            result = json.loads(stdout.decode("utf-8"))
            if expected is not None and code != expected:
                raise RuntimeError(f"CLI action={payload['action']} exit={code} expected={expected}; error_code={result.get('code')}")
            if expected is None and code == 0:
                raise RuntimeError("Expected acquisition refusal returned success")
            return result

        def registered(identifier: str, url: str, metadata: dict[str, object], credential: str | None = None) -> Path:
            workspace = root / identifier
            workspace.mkdir()
            cli_roots.append(workspace)
            request(workspace, {"action": "source-add", "id": identifier, "repository": url, "enabled": True,
                                "roots": ["skills/demo"], "targets": ["claude-code"], "credential_reference": credential,
                                "metadata": metadata, "update": "approved-content-only"})
            return workspace

        def candidate(workspace: Path, identifier: str, env: dict[str, str], expected_revision: str = initial) -> dict[str, object]:
            result = request(workspace, {"action": "discover", "source": identifier}, env)
            assert result["revision"] == expected_revision
            selected = next(file for file in result["files"] if file["path"] == "skills/demo/SKILL.md")
            assert selected["digest"] == hashlib.sha256(PAYLOAD).hexdigest()
            assert selected["executable"] is False
            return result

        with https_repository(root, certificate, key, auth, owner.environment) as stats:
            policy = {"provider_policy": {"transport": "https", "host": "127.0.0.1", "reviewed": True}}
            workspace = registered("https-demo", "https://127.0.0.1/private/repo.git", policy, "FORGE_FIXTURE_HEADER")
            wrong = {"FORGE_FIXTURE_HEADER": "Bearer incorrect-first-party-test-value"}
            with socket.create_connection(("127.0.0.1", 443), timeout=2):
                request(workspace, {"action": "discover", "source": "https-demo"}, wrong, expected=None)
            assert stats["handshake_timeout"] == 1 and stats["rejected"] > 0
            environment = {"FORGE_FIXTURE_HEADER": auth}
            snapshot = candidate(workspace, "https-demo", environment)
            assert stats["upload"] > 0, "Real Git smart-protocol transfer was not observed"
            request(workspace, {"action": "approve", "source": "https-demo", "digest": snapshot["content_digest"]})
            request(workspace, {"action": "package-add", "package": {"id": "demo", "name": "Protocol fixture", "source": "https-demo", "components": [], "targets": ["claude-code"]}})
            request(workspace, {"action": "build", "package": "demo", "target": "claude-code"})
            packaged = workspace / "packages/claude-code/demo/skills/transport-demo/SKILL.md"
            assert packaged.read_bytes() == PAYLOAD
            lock = workspace / "forge.lock.json"
            accepted = lock.read_bytes()
            fail = request(workspace, {"action": "sync", "apply": True}, wrong, expected=2)
            assert fail["status"] == "partial" and fail["sources"][0]["retained"] is True
            assert lock.read_bytes() == accepted and packaged.read_bytes() == PAYLOAD
            offline = request(workspace, {"action": "discover", "source": "https-demo", "offline": True}, offline=True)
            assert offline["revision"] == initial
            update = PAYLOAD + b"Additional safe first-party guidance.\n"
            (repository / "skills/demo/SKILL.md").write_bytes(update)
            owner.run(["git", "-C", str(repository), "add", "skills"])
            owner.run(["git", "-C", str(repository), "commit", "-m", "fixture update"])
            changed = request(workspace, {"action": "sync", "apply": True}, environment)
            assert changed["sources"][0]["status"] == "accepted"
            request(workspace, {"action": "build", "package": "demo", "target": "claude-code"})
            assert packaged.read_bytes() == update
            unchanged = request(workspace, {"action": "sync", "apply": True}, environment)
            assert unchanged["sources"][0]["status"] == "unchanged"
            redirect = registered("redirect-demo", "https://127.0.0.1/redirect/repo.git", policy, "FORGE_FIXTURE_HEADER")
            before = stats["authorized"]
            request(redirect, {"action": "discover", "source": "redirect-demo"}, environment, expected=None)
            assert stats["redirect"] > 0 and stats["authorized"] == before
            logging.info("HTTPS real auth/refusal/transfer/update/offline/retention/redirect contracts verified")

        for name in ("host", "identity", "other-host"):
            owner.run(["ssh-keygen", "-t", "ed25519", "-N", "", "-f", str(root / name)])
        socket_path = root / "agent.sock"
        agent = owner.start(["ssh-agent", "-D", "-a", str(socket_path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + owner.remaining(5)
        while not socket_path.exists():
            if agent.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Owned agent did not create its socket")
            time.sleep(0.02)
        owner.run(["ssh-add", str(root / "identity")], {"SSH_AUTH_SOCK": str(socket_path)})
        root_home = root / "root-home"
        root_home.mkdir(mode=0o700)
        owner.run(["mount", "--bind", str(root_home), "/root"])
        authorized = root_home / "authorized_keys"
        authorized.write_text('restrict,command="git-upload-pack ' + str(repository) + '" ' + (root / "identity.pub").read_text(encoding="utf-8"), encoding="utf-8")
        host_fields = (root / "host.pub").read_text(encoding="utf-8").split()
        hosts = root / "known_hosts"
        hosts.write_text("127.0.0.1 " + " ".join(host_fields[:2]) + "\n", encoding="utf-8")
        config = root / "sshd_config"
        config.write_text(f"Port 22\nListenAddress 127.0.0.1\nHostKey {root / 'host'}\nPidFile {root / 'sshd.pid'}\nAuthorizedKeysFile /root/authorized_keys\nPermitRootLogin forced-commands-only\nPubkeyAuthentication yes\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM yes\nStrictModes yes\nAllowUsers root\nDisableForwarding yes\nPermitUserRC no\nLogLevel ERROR\n", encoding="utf-8")
        Path("/run/sshd").mkdir(exist_ok=True)
        owner.run(["/usr/sbin/sshd", "-t", "-f", str(config)])
        server = owner.start(["/usr/sbin/sshd", "-D", "-e", "-f", str(config)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wait_listener(22, server, owner)
        ssh_policy = {"provider_policy": {"transport": "ssh", "host": "127.0.0.1", "reviewed": True},
                      "ssh_authentication": {"public_key_reference": "FORGE_FIXTURE_PUBLIC", "known_hosts_reference": "FORGE_FIXTURE_HOSTS", "agent_reference": "FORGE_FIXTURE_AGENT"}}
        workspace = registered("ssh-demo", "ssh://root@127.0.0.1/owned/repo.git", ssh_policy)
        environment = {"FORGE_FIXTURE_PUBLIC": str(root / "identity.pub"), "FORGE_FIXTURE_HOSTS": str(hosts), "FORGE_FIXTURE_AGENT": str(socket_path)}
        snapshot = request(workspace, {"action": "discover", "source": "ssh-demo"}, environment)
        selected = next(file for file in snapshot["files"] if file["path"] == "skills/demo/SKILL.md")
        assert selected["digest"] == hashlib.sha256(update).hexdigest()
        request(workspace, {"action": "approve", "source": "ssh-demo", "digest": snapshot["content_digest"]})
        prior = (workspace / "forge.lock.json").read_bytes()
        other = (root / "other-host.pub").read_text(encoding="utf-8").split()
        hosts.write_text("127.0.0.1 " + " ".join(other[:2]) + "\n", encoding="utf-8")
        fail = request(workspace, {"action": "sync", "apply": True}, environment, expected=2)
        assert fail["status"] == "partial" and fail["sources"][0]["retained"] is True
        assert (workspace / "forge.lock.json").read_bytes() == prior
        owner.run(["ssh-add", "-D"], {"SSH_AUTH_SOCK": str(socket_path)})
        hosts.write_text("127.0.0.1 " + " ".join(host_fields[:2]) + "\n", encoding="utf-8")
        request(workspace, {"action": "discover", "source": "ssh-demo"}, environment, expected=None)
        logging.info("SSH real agent identity and pinned-host success/refusal contracts verified")
        for workspace in cli_roots:
            for path in workspace.rglob("*"):
                if path.is_file() and auth.encode() in path.read_bytes():
                    raise RuntimeError("Authentication value persisted in CLI workspace")
        logging.info("Selected authentication value absent from workspace files and logs")
    finally:
        signal.signal(signal.SIGTERM, previous_handler)
        try:
            owner.close()
        finally:
            shutil.rmtree(root)
            if root.exists():
                raise RuntimeError("Owned sensitive transport fixture was not removed")
            logging.info("Owned credentials/certificates/agent/repository fixtures removed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    if (sys.platform != "linux" or os.geteuid() != 0 or os.getpid() != 1 or
            os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"):
        raise RuntimeError("Transport fixture requires its hosted Linux PID/mount/network namespace; never run on a user machine")
    project = Path(__file__).resolve().parent.parent
    application = args.application.resolve(strict=True)
    application.relative_to(project / "target/release")
    work = args.work.resolve()
    work.relative_to(project / ".ci-work")
    work.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    handler = logging.FileHandler(work / f"verify-source-transport_{stamp}.log", encoding="utf-8", mode="x")
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [transport] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        logging.info("Hosted first-party protocol verification started")
        verify(application, work)
        logging.info("Transport verification completed; no upstream scripts or user credentials used")
        print('{"status":"success","surface":"owned-https-ssh-protocol"}')
    except Exception:
        logging.error("Transport verification failed; inspect the owning CI step", exc_info=True)
        raise
    finally:
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    main()
