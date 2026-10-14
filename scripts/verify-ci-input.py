"""Owned Linux-only negative controls; no original skill payload is executed."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import uuid


def command(argv, *, expected=0, timeout=30):
    result = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout,
                            env={"PATH": "/usr/local/bin:/usr/bin:/bin", "LANG": "C.UTF-8"})
    if (result.returncode == 0) != (expected == 0):
        raise RuntimeError("Control command returned unexpected status")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--producer", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir()
    results = []
    docker = ["/usr/bin/docker", "--host", "unix:///var/run/docker.sock"]
    writes = ["/scratch", "/tmp", "/input/.ci-work", "/input/.forge-state",
              "/input/apps/desktop/dist", "/input/apps/desktop/coverage",
              "/input/apps/desktop/node_modules/.vite", "/input/apps/desktop/node_modules/.vite-temp"]
    env = ["PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin", "LANG=C.UTF-8", "LC_ALL=C.UTF-8",
           "HOME=/scratch/home", "TMPDIR=/tmp", "CARGO_HOME=/scratch/cargo", "RUSTUP_HOME=/usr/local/rustup",
           "CARGO_TARGET_DIR=/scratch/target", "CARGO_BUILD_JOBS=2", "RUST_TEST_THREADS=2",
           "CARGO_PROFILE_DEV_DEBUG=0", "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright", "CI=1"]
    sentinel = args.work / "host-private-input-sentinel"
    sentinel.write_text("non-sensitive control\n", encoding="utf-8")
    assert sentinel.read_text(encoding="utf-8") == "non-sensitive control\n"

    def container(overrides, argv, expected):
        name = "forge-input-negative-" + uuid.uuid4().hex
        flags = {"--network": "none", "--read-only": None, "--cap-drop": "ALL",
                 "--security-opt": "no-new-privileges", "--ipc": "private", "--pids-limit": "256",
                 "--memory": "4g", "--memory-swap": "4g", "--cpus": "2", "--user": "1000:1000"}
        flags.update(overrides)
        create = docker + ["create", "--pull=never", "--name", name, "--label", "numera.forge.input.test=" + name]
        for key, value in flags.items():
            if value is False:
                continue
            create += [key] if value is None else [key, value]
        for path in writes:
            create += ["--tmpfs", path + ":rw,exec,nosuid,nodev,size=2147483648,mode=1777"]
        create += ["--entrypoint=/usr/bin/env", args.image, "-i"] + env + argv
        try:
            command(create)
            result = command(docker + ["start", "--attach", name], expected=expected)
            if expected != 0:
                assert b"FORGE_INPUT_RESULT" not in result.stdout
        finally:
            command(docker + ["rm", "--force", name])
            assert not command(docker + ["ps", "-aq", "--filter", "label=numera.forge.input.test=" + name]).stdout

    # Same UID and image: writable-root positive distinguishes EROFS from file permissions.
    container({"--read-only": False}, ["python3", "-I", "-c",
              "open('/input/control-write-probe','w').close(); open('/deps/control-write-probe','w').close()"], 0)
    container({"--mount": "type=bind,source=" + str(sentinel.resolve()) + ",target=/host-private-input-sentinel,readonly"},
              ["python3", "-I", "-c", "from pathlib import Path; assert Path('/host-private-input-sentinel').read_text(encoding='utf-8') == 'non-sensitive control\\n'"], 0)
    container({}, ["python3", "-I", "-c",
              "from pathlib import Path; assert not Path('/host-private-input-sentinel').exists()"], 0)
    worker = ["/usr/local/bin/forge-ci-input", "worker", "probe"]
    for label, flags, argv in [
        ("readonly", {"--read-only": False}, worker),
        ("privilege", {"--user": "0:0"}, worker),
        ("resource", {"--pids-limit": "257"}, worker),
        ("network", {"--network": "bridge"}, worker),
        ("ambient-environment", {}, ["AMBIENT_CONTROL=1"] + worker),
        ("ambient-descriptor", {}, ["/bin/sh", "-c", "exec 3</input/Cargo.toml; exec /usr/local/bin/forge-ci-input worker probe"]),
    ]:
        container(flags, argv, 1)
        results.append(label)

    base = [str(args.producer), "run", "--image", args.image, "--source", args.source]
    output = args.work / "publication-conflict"
    output.mkdir()
    old = b'{"original":true}'
    (output / "result.json").write_bytes(old)
    command(base + ["--mode", "probe", "--output", str(output)], expected=1)
    assert (output / "result.json").read_bytes() == old
    results.append("existing-output-refusal")

    output = args.work / "command-failure"
    command(base + ["--mode", "failure-probe", "--output", str(output)], expected=1)
    failed = json.loads((output / "result.json").read_text(encoding="utf-8"))
    assert failed["outcome"] == "failed" and failed["cleanup_verified"]
    assert len(failed["manifest_digest"]) == 64
    assert failed["worker"]["commands"][0]["argv"] == ["/usr/bin/false"]
    assert failed["worker"]["commands"][0]["exit"] != 0
    results.append("failed-command-evidence")

    output = args.work / "cancel"
    child = subprocess.Popen(base + ["--mode", "timeout-probe", "--output", str(output)],
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if list(output.glob("*.log")) and any("preflight" in p.read_text(encoding="utf-8") for p in output.glob("*.log")):
                break
            if child.poll() is not None:
                raise RuntimeError("Cancellation worker exited before cancellation")
            time.sleep(0.02)
        else:
            raise RuntimeError("Cancellation readiness deadline exceeded")
        child.send_signal(signal.SIGTERM)
        child.communicate(timeout=30)
        assert child.returncode != 0
        record = json.loads((output / "result.json").read_text(encoding="utf-8"))
        assert record["outcome"] == "terminated" and record["cleanup_verified"]
        assert record["reason"] == "cancelled" and len(record["manifest_digest"]) == 64
        assert not command(docker + ["ps", "-aq", "--filter", "label=numera.forge.input.owner=" + record["run_id"]]).stdout
    finally:
        if child.poll() is None:
            child.send_signal(signal.SIGTERM)
            try:
                child.communicate(timeout=65)
            except subprocess.TimeoutExpired:
                child.kill()
                child.communicate(timeout=5)
                # The original test still fails; sweep exact labelled ownership below.
                assert child.returncode != 0
        for log in output.glob("*.log"):
            for line in log.read_text(encoding="utf-8").splitlines():
                if "[ownership] container=" in line:
                    name = line.split("container=", 1)[1]
                    assert name.startswith("forge-input-") and len(name) == 44
                    remaining = command(docker + ["ps", "-aq", "--filter", "label=numera.forge.input.owner=" + name]).stdout
                    if remaining:
                        command(docker + ["rm", "--force", name])
                    assert not command(docker + ["ps", "-aq", "--filter", "label=numera.forge.input.owner=" + name]).stdout
    results.append("cancellation")
    sentinel.unlink()
    (args.work / "result.json").write_text(json.dumps({"schema": 1, "outcome": "succeeded",
          "controls": results, "source_revision": args.source, "image": args.image}), encoding="utf-8")
    print("Controlled negative tests passed")


if __name__ == "__main__":
    main()
