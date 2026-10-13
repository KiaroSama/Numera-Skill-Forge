#!/usr/bin/env python3
"""Hosted Linux owner for original Pi SDK and Gemini core non-model acceptance."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import logging
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import time
import uuid


WALL_SECONDS = 120
IDLE_SECONDS = 20
OUTPUT_LIMIT = 1024 * 1024
DOCKER = ["docker", "--host", "unix:///var/run/docker.sock"]


class Owner:
    def __init__(self, work: Path) -> None:
        self.deadline = time.monotonic() + WALL_SECONDS
        self.environment = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8",
                            "LC_ALL": "C.UTF-8", "HOME": str(work / "home"),
                            "DOCKER_CONFIG": str(work / "docker-config"),
                            "FORGE_HOST_SECRET_CANARY": "inert-must-not-enter-container"}
        for name in ("home", "docker-config"):
            (work / name).mkdir(mode=0o700)
        self.processes: dict[int, subprocess.Popen] = {}
        self.output_bytes = 0

    def run(self, arguments: list[str], seconds: float = 20, expected: int | None = 0,
            cleanup: bool = False, progress: bool = False) -> tuple[int, bytes, bytes]:
        remaining = seconds if cleanup else min(seconds, self.deadline - time.monotonic())
        if remaining <= 0:
            raise TimeoutError("Host activation wall deadline expired")
        process = subprocess.Popen(DOCKER + arguments, env=self.environment, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        self.processes[process.pid] = process
        logging.info("Owned Docker operation started pid=%s operation=%s", process.pid, arguments[0])
        start = time.monotonic()
        last_output = start
        output: dict[str, bytearray] = {"stdout": bytearray(), "stderr": bytearray()}
        pending = bytearray()
        try:
            with selectors.DefaultSelector() as selector:
                for name, pipe in (("stdout", process.stdout), ("stderr", process.stderr)):
                    os.set_blocking(pipe.fileno(), False)
                    selector.register(pipe, selectors.EVENT_READ, name)
                while selector.get_map():
                    now = time.monotonic()
                    if now - start >= remaining or now - last_output >= IDLE_SECONDS:
                        raise TimeoutError("Owned Docker operation exceeded wall or idle deadline")
                    for key, _ in selector.select(min(0.25, remaining - (now - start))):
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            selector.unregister(key.fileobj)
                            continue
                        last_output = time.monotonic()
                        output[key.data].extend(chunk)
                        self.output_bytes += len(chunk)
                        if sum(map(len, output.values())) > OUTPUT_LIMIT or (not cleanup and self.output_bytes > OUTPUT_LIMIT):
                            raise RuntimeError("Host activation output exceeded 1 MiB")
                        if progress and key.data == "stdout":
                            pending.extend(chunk)
                            while b"\n" in pending:
                                line, _, rest = pending.partition(b"\n")
                                pending = bytearray(rest)
                                if line.startswith(b"FORGE_ACTIVATION_RECORD "):
                                    record = json.loads(line.removeprefix(b"FORGE_ACTIVATION_RECORD ").decode("utf-8"))
                                    logging.info("Native probe progress stage=%s", record.get("stage", "unknown"))
            code = process.wait(timeout=max(0.1, remaining - (time.monotonic() - start)))
            if expected is not None and code != expected:
                logging.error("Docker operation failed exit=%s stderr_digest=%s", code,
                              hashlib.sha256(output["stderr"]).hexdigest())
                raise RuntimeError(f"Docker {arguments[0]} failed with exit {code}")
            return code, bytes(output["stdout"]), bytes(output["stderr"])
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
                logging.warning("Owned Docker client terminated pid=%s", process.pid)
            for pipe in (process.stdout, process.stderr):
                pipe.close()
            self.processes.pop(process.pid, None)

    def close(self) -> None:
        for process in list(self.processes.values()):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        self.processes.clear()


def container_contract(details: dict, image: str) -> None:
    host = details["HostConfig"]
    assert details["Image"] == image
    assert details["Config"]["User"] == "1000:1000"
    assert details["Config"]["OpenStdin"] is False
    assert details["Config"]["Tty"] is False
    assert host["NetworkMode"] == "none"
    assert host["ReadonlyRootfs"] is True
    assert host["Privileged"] is False
    assert {entry.lower() for entry in host["CapDrop"]} == {"all"}
    assert host["CapAdd"] in (None, [])
    assert "no-new-privileges" in host["SecurityOpt"] or "no-new-privileges:true" in host["SecurityOpt"]
    assert host["Memory"] == 1024 * 1024 * 1024
    assert host["MemorySwap"] == host["Memory"]
    assert host["NanoCpus"] == 2_000_000_000
    assert host["PidsLimit"] == 64
    assert host["Binds"] in (None, [])
    assert host["Devices"] in (None, [])
    assert host["PortBindings"] in (None, {})
    assert set(host["Tmpfs"]) == {"/scratch"}
    for flag in ("rw", "nosuid", "nodev", "noexec", "size=134217728"):
        assert flag in host["Tmpfs"]["/scratch"].split(",")
    assert all(mount["Type"] == "tmpfs" and mount["Destination"] == "/scratch"
               for mount in details["Mounts"])
    assert details["Config"]["Labels"].get("numera.forge.activation.owner")


def verify(image_name: str, work: Path) -> None:
    owner = Owner(work)
    name = f"forge-host-activation-{uuid.uuid4().hex}"
    identifier: str | None = None
    cleanup_errors = []
    cancelled = lambda signum, frame: (_ for _ in ()).throw(TimeoutError("Host activation cancelled"))
    handlers = {sig: signal.signal(sig, cancelled) for sig in (signal.SIGTERM, signal.SIGINT)}
    try:
        # Consume a locally built image only; runtime acceptance never pulls or installs.
        _, output, _ = owner.run(["image", "inspect", image_name])
        images = json.loads(output.decode("utf-8"))
        assert len(images) == 1
        image = images[0]
        assert image["Os"] == "linux" and image["Architecture"] == "amd64"
        assert image["Config"]["Volumes"] in (None, {})
        image_id = image["Id"]
        assert re.fullmatch(r"sha256:[a-f0-9]{64}", image_id)
        create = ["create", "--pull=never", "--name", name,
                  "--label", f"numera.forge.activation.owner={name}", "--network=none", "--read-only",
                  "--cap-drop=ALL", "--security-opt=no-new-privileges", "--pids-limit=64",
                  "--memory=1g", "--memory-swap=1g", "--cpus=2", "--user=1000:1000",
                  "--ulimit=nofile=256:256", "--ulimit=fsize=16777216:16777216",
                  "--log-driver=local", "--log-opt=max-size=1m", "--log-opt=max-file=1",
                  "--tmpfs=/scratch:rw,nosuid,nodev,noexec,size=134217728,mode=1777", image_id]
        _, output, _ = owner.run(create)
        identifier = output.decode("ascii").strip()
        assert re.fullmatch(r"[a-f0-9]{64}", identifier)
        (work / "container.json").write_text(json.dumps({"name": name, "id": identifier, "image": image_id}),
                                              encoding="utf-8")
        _, output, _ = owner.run(["inspect", identifier])
        details = json.loads(output.decode("utf-8"))[0]
        assert details["Config"]["Labels"]["numera.forge.activation.owner"] == name
        container_contract(details, image_id)
        logging.info("Independent Docker preflight passed; owned_container=%s image and limits verified before activation", name)
        code, output, stderr = owner.run(["start", "--attach", identifier], seconds=WALL_SECONDS,
                                         expected=None, progress=True)
        prefix = b"FORGE_ACTIVATION_RECORD "
        records = [json.loads(line.removeprefix(prefix).decode("utf-8"))
                   for line in output.splitlines() if line.startswith(prefix)]
        other_output = b"\n".join(line for line in output.splitlines() if not line.startswith(prefix))
        (work / "result.json").write_text(json.dumps({"surface": "nonmodel-sdk-core", "exit": code,
                                                     "records": records, "other_stdout_bytes": len(other_output),
                                                     "other_stdout_digest": hashlib.sha256(other_output).hexdigest(),
                                                     "stderr_bytes": len(stderr),
                                                     "stderr_digest": hashlib.sha256(stderr).hexdigest()}, indent=2),
                                             encoding="utf-8")
        if stderr:
            logging.warning("Native dependencies emitted stderr bytes=%s digest=%s", len(stderr),
                            hashlib.sha256(stderr).hexdigest())
        assert code == 0, "Native probe failed; inspect phase in result.json"
        expected = ["isolation-passed", "pi-sdk-start", "pi-sdk-passed", "gemini-core-start",
                    "gemini-core-passed", "success"]
        assert [record["stage"] for record in records] == expected
        assert records[-1]["modelInvocation"] is False and records[-1]["fullCliCoverage"] is False
        _, output, _ = owner.run(["inspect", identifier])
        finished = json.loads(output.decode("utf-8"))[0]
        assert finished["State"]["Running"] is False
        assert finished["State"]["ExitCode"] == 0
        assert finished["State"]["OOMKilled"] is False
        assert finished["State"]["Pid"] == 0
        container_contract(finished, image_id)
        logging.info("Original native callback/body consumption and negative controls passed; not all-host CLI evidence")
    finally:
        # Creation may have succeeded before its client failed; the unique owned name still permits cleanup.
        for sig in handlers:
            signal.signal(sig, signal.SIG_IGN)
        try:
            code, output, _ = owner.run(["container", "ls", "--all", "--filter", f"name=^/{name}$",
                                         "--format", "{{.ID}}"], cleanup=True, seconds=5)
            if output.strip():
                _, output, _ = owner.run(["inspect", name], cleanup=True, seconds=5)
                found = json.loads(output.decode("utf-8"))[0]
                if found["Config"]["Labels"].get("numera.forge.activation.owner") != name:
                    raise RuntimeError("Container ownership mismatch; cleanup refused")
                owned_id = found["Id"]
                if found["State"]["Running"]:
                    owner.run(["kill", owned_id], cleanup=True, seconds=5)
                owner.run(["rm", "--force", owned_id], cleanup=True, seconds=5)
            code, output, _ = owner.run(["container", "ls", "--all", "--filter", f"name=^/{name}$",
                                         "--format", "{{.ID}}"], cleanup=True, seconds=5)
            if code != 0 or output.strip():
                raise RuntimeError("Owned container absence was not verified")
            logging.info("Owned container removed and exact-name absence verified; scratch tmpfs destroyed")
        except Exception as error:
            cleanup_errors.append(type(error).__name__)
            logging.error("Owned container cleanup remains incomplete kind=%s", type(error).__name__)
        finally:
            owner.close()
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
        if cleanup_errors:
            raise RuntimeError("Host activation cleanup failed; owned container name is in the diagnostic log")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    if (sys.platform != "linux" or os.environ.get("GITHUB_ACTIONS") != "true" or
            os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"):
        raise RuntimeError("Host activation requires a disposable GitHub-hosted Linux runner")
    if not re.fullmatch(r"[a-z0-9][a-z0-9._/:@-]{0,200}", args.image):
        raise ValueError("Invalid local Docker image identifier")
    project = Path(__file__).resolve().parent.parent
    work = args.work.resolve()
    work.relative_to(project / ".ci-work")
    if work.exists():
        raise RuntimeError("Host activation requires a new empty diagnostic directory")
    work.mkdir(parents=True, mode=0o700)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    handler = logging.FileHandler(work / f"verify-host-activation_{stamp}.log", encoding="utf-8", mode="x")
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [host-activation] %(message)s",
                                  "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        logging.info("Hosted SDK/core activation started; wall=%ss idle=%ss worker=1", WALL_SECONDS, IDLE_SECONDS)
        verify(args.image, work)
        print('{"status":"success","surface":"pi-sdk-command-and-gemini-core-skill","modelInvocation":false}')
    except Exception as error:
        logging.error("Host activation failed kind=%s; no runtime pass implied", type(error).__name__)
        raise
    finally:
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    main()
