"""Account-free isolated Linux checks for exact original DoorDash hooks."""
import argparse
from datetime import datetime, timezone
import json
import logging
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    try:
        handler = logging.FileHandler(work / ("verify-doordash-hooks_" + stamp + ".log"), encoding="utf-8")
    except OSError:
        handler = logging.StreamHandler()
        handler.setLevel(logging.INFO)
        print("File logging unavailable; diagnostics use stderr", file=sys.stderr)
    logging.basicConfig(level=logging.INFO, handlers=[handler], format="[%(asctime)s UTC] [%(levelname)s] [DOORDASH] %(message)s")
    logging.Formatter.converter = time.gmtime
    fixture = work / "fixture"
    fixture.mkdir()
    result = {"outcome": "failed", "account_access": "not-performed", "checks": [], "owned_processes": 0}
    deadline = time.monotonic() + 45
    child = None
    try:
        home = fixture / "home"
        home.mkdir()
        tools = fixture / "bin"
        tools.mkdir()
        fake = tools / "dd-cli"
        fake.write_text("#!/bin/sh\nprintf '%s\\n' 'ramen bowl: peanut sauce'\n", encoding="utf-8")
        fake.chmod(0o755)
        env = {"PATH": str(tools) + ":/usr/bin:/bin", "HOME": str(home), "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "PYTHONSAFEPATH": "1", "PYTHONUTF8": "1"}
        scripts = project / "packages/claude-code/skill-doordash-doordash-allergy-shield/native-originals/hooks/doordash"

        def run(name, command, expected):
            nonlocal child
            payload = json.dumps({"tool_input": {"command": command}, "cwd": str(fixture), "tool_response": {"exit_code": 0}}).encode("utf-8")
            child = subprocess.Popen(["/bin/bash", str(scripts / (name + ".sh"))], cwd=fixture, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                out, err = child.communicate(payload, timeout=min(8, max(0.1, deadline - time.monotonic())))
                assert child.returncode == expected, (name, child.returncode, err.decode("utf-8"))
                assert len(out) + len(err) < 65536
                result["checks"].append(name + ":" + str(expected))
                logging.info("Original hook=%s exit=%d", name, child.returncode)
            finally:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait(timeout=3)
                child = None

        run("doordash-spend-guard", "dd-cli order checkout-url --cart-uuid cart1", 2)
        run("doordash-spend-guard", "printf harmless", 0)
        profile = home / ".claude/doordash-profile"
        profile.mkdir(parents=True)
        (profile / "dietary.json").write_text(json.dumps({"people": {"fixture": {"allergens": [{"name": "peanut", "severity": "anaphylaxis"}]}}}), encoding="utf-8")
        run("doordash-allergy-checkout-gate", "dd-cli order checkout-url --cart-uuid cart1", 2)
        (profile / "vetted").mkdir()
        (profile / "vetted/cart1.json").write_text('{"items":["plain rice"]}', encoding="utf-8")
        run("doordash-allergy-checkout-gate", "dd-cli order checkout-url --cart-uuid cart1", 0)
        (fixture / ".dd").mkdir()
        (fixture / ".dd/round-1.json").write_text(json.dumps({"cart_uuid": "cart1", "orders": {"fixture": [{"item": "ramen bowl"}]}}), encoding="utf-8")
        (fixture / "team-food.json").write_text(json.dumps({"members": {"fixture": {"allergens": [{"name": "peanut", "severity": "anaphylaxis"}]}}}), encoding="utf-8")
        run("doordash-group-checkout-gate", "dd-cli order checkout-url --cart-uuid cart1", 2)
        run("doordash-audit-log", "dd-cli search --token SYNTHETIC_FIXTURE_SECRET", 0)
        audit = (home / ".claude/dd-guard/audit.jsonl").read_text(encoding="utf-8")
        assert "SYNTHETIC_FIXTURE_SECRET" not in audit and json.loads(audit)["event_type"] == "search"
        result["outcome"] = "succeeded"
    except Exception:
        logging.exception("Isolated original-hook verification failed")
        raise
    finally:
        if child is not None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=3)
        shutil.rmtree(fixture)
        result["fixture_removed"] = not fixture.exists()
        (work / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        logging.info("Outcome=%s owned_processes=0 fixture_removed=%s", result["outcome"], result["fixture_removed"])
        handler.close()
    print("Original DoorDash isolated hook checks passed; no account or order access")


if __name__ == "__main__":
    main()
