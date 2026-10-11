#!/usr/bin/env python3
"""Metadata-only installed-host probes using owned first-party fixtures, never model sessions."""

import argparse
from datetime import datetime, timezone
import json
import logging
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

PROBE = "---\nname: probe\ndescription: Harmless first-party metadata probe\n---\nRead-only fixture; do not execute commands.\n"


def write(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def run(arguments: list[str], cwd: Path, environment: dict[str, str], expected: int) -> tuple[bytes, bytes]:
    options: dict[str, object] = {}
    if sys.platform == "win32":
        options["creationflags"] = subprocess.CREATE_NO_WINDOW
    else:
        options["start_new_session"] = True
    with subprocess.Popen(arguments, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, **options) as child:
        try:
            stdout, stderr = child.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            if sys.platform != "win32":
                import signal
                os.killpg(child.pid, signal.SIGKILL)
            else:
                child.kill()
            child.communicate(timeout=5)
            raise
        if len(stdout) + len(stderr) > 1024 * 1024:
            raise RuntimeError("Host metadata output exceeded one MiB")
        if child.returncode != expected:
            raise RuntimeError(f"Host metadata command returned {child.returncode}, expected {expected}")
        return stdout, stderr


def verify(work: Path, claude: Path, node: Path, gemini: Path) -> None:
    fixture = work / "run"
    if fixture.exists():
        raise RuntimeError("Host fixture requires a fresh owned directory")
    fixture.mkdir()
    try:
        cwd = fixture / "cwd"
        cwd.mkdir()
        (cwd / ".env").write_text("", encoding="utf-8")
        environment = {key: os.environ[key] for key in ("SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT") if key in os.environ}
        environment["PATH"] = os.pathsep.join({str(claude.parent), str(node.parent), str(Path(os.environ.get("SYSTEMROOT", "/usr")) / "System32")})
        for key in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "TEMP", "TMP", "TMPDIR"):
            path = fixture / key.lower()
            path.mkdir()
            environment[key] = str(path)
        environment.update({
            "NO_COLOR": "1", "DISABLE_AUTOUPDATER": "1", "DISABLE_TELEMETRY": "1",
            "DISABLE_ERROR_REPORTING": "1", "CLAUDE_CONFIG_DIR": str(fixture / "claude-config"),
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL": "1",
        })
        plugin = fixture / "plugin"
        write(plugin / ".claude-plugin/plugin.json", {"name": "forge-host-probe", "version": "1.0.0", "description": "Harmless first-party discovery fixture"})
        skill = plugin / "skills/probe/SKILL.md"
        skill.parent.mkdir(parents=True)
        skill.write_text(PROBE, encoding="utf-8")
        stdout, stderr = run([str(claude), "plugin", "validate", str(plugin), "--json"], cwd, environment, 0)
        result = json.loads(stdout.decode("utf-8"))
        assert result["success"] is True and result["strict"] is False
        assert result["manifest"]["type"] == "plugin" and not result["manifest"]["errors"]
        assert not any(item["errors"] for item in result["contents"])
        warnings = len(result["manifest"]["warnings"]) + sum(len(item["warnings"]) for item in result["contents"])
        # Clean skills are omitted from diagnostic contents; a malformed skill proves traversal.
        skill.write_text(PROBE.replace("description: Harmless first-party metadata probe", "description: []"), encoding="utf-8")
        stdout, _ = run([str(claude), "plugin", "validate", str(plugin), "--json"], cwd, environment, 1)
        malformed = json.loads(stdout.decode("utf-8"))
        assert malformed["success"] is False and any(
            Path(item["file"]).resolve() == skill.resolve() and item["type"] == "skill" and
            any(error["path"] == "description" for error in item["errors"])
            for item in malformed["contents"])
        invalid = fixture / "invalid-plugin"
        write(invalid / ".claude-plugin/plugin.json", {})
        stdout, _ = run([str(claude), "plugin", "validate", str(invalid), "--json"], cwd, environment, 1)
        rejected = json.loads(stdout.decode("utf-8"))
        assert rejected["success"] is False and rejected["manifest"]["errors"]
        logging.info("Installed Claude positive/negative metadata validation passed warnings=%d", warnings)

        home = fixture / "gemini-home"
        settings = home / ".gemini/settings.json"
        write(settings, {"experimental": {"extensionConfig": False}, "telemetry": {"enabled": False}})
        empty = fixture / "system-settings.json"
        write(empty, {})
        environment.update({"GEMINI_CLI_HOME": str(home), "GEMINI_CLI_NO_RELAUNCH": "1",
                            "GEMINI_CLI_SYSTEM_SETTINGS_PATH": str(empty), "GEMINI_CLI_SYSTEM_DEFAULTS_PATH": str(empty)})
        extensions = home / ".gemini/extensions"
        extension = extensions / "forge-host-probe"
        manifest = {"name": "forge-host-probe", "version": "1.0.0"}
        write(extension / "gemini-extension.json", manifest)
        skill = extension / "skills/probe/SKILL.md"
        skill.parent.mkdir(parents=True)
        skill.write_text(PROBE, encoding="utf-8")
        command = [str(node), str(gemini), "extensions", "list", "--output-format", "json"]
        stdout, stderr = run(command, cwd, environment, 0)
        text = (stdout or stderr).decode("utf-8")
        lines = text.splitlines(keepends=True)
        starts = [index for index, line in enumerate(lines) if line.strip() in ("[", "[]")]
        assert len(starts) == 1, "Expected exactly one Gemini JSON array document"
        prefix = lines[:starts[0]]
        assert all(not line.strip() or line.startswith("Security Warning: Skipping system ") for line in prefix)
        rows = json.loads("".join(lines[starts[0]:]))
        logging.info("Gemini JSON channel=%s leading_security_warnings=%d", "stdout" if stdout else "stderr", len(prefix))
        logging.info("Gemini metadata diagnostics bytes=%d warning_lines=%d skip_lines=%d", len(stderr),
                     sum(b"warning" in line.lower() for line in stderr.splitlines()),
                     sum(b"skip" in line.lower() for line in stderr.splitlines()))
        assert len(rows) == 1 and rows[0]["name"] == manifest["name"] and rows[0]["version"] == "1.0.0"
        assert Path(rows[0]["path"]).resolve() == extension.resolve()
        assert rows[0]["contextFiles"] == [] and rows[0]["resolvedSettings"] == []
        assert any(item["name"] == "probe" and item["extensionName"] == manifest["name"] for item in rows[0]["skills"])
        write(extensions / "duplicate/gemini-extension.json", manifest)
        _, stderr = run(command, cwd, environment, 1)
        assert b"already was loaded" in stderr
        logging.info("Installed Gemini real skill discovery and duplicate-name refusal passed; no extension started")
    finally:
        shutil.rmtree(fixture)
        if fixture.exists():
            raise RuntimeError("Owned host profile fixture was not removed")
        logging.info("Owned first-party host profiles removed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--claude", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--gemini", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    if not os.environ.get("HOOKMAKER_MAX_TEST_WORKERS"):
        raise RuntimeError("Installed-host probe requires an outer owned bounded runner")
    project = Path(__file__).resolve().parent.parent
    work = args.work.resolve()
    work.relative_to(project / ".ci-work")
    work.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    handler = logging.FileHandler(work / f"verify-host-discovery_{stamp}.log", encoding="utf-8", mode="x")
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [host-discovery] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        logging.info("Installed-host metadata probe started; no models or collected payloads")
        verify(work, args.claude.resolve(strict=True), args.node.resolve(strict=True), args.gemini.resolve(strict=True))
        print('{"status":"success","surface":"claude-validation-gemini-discovery","model_invocation":false}')
    except Exception:
        logging.error("Installed-host metadata probe failed", exc_info=True)
        raise
    finally:
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    main()
