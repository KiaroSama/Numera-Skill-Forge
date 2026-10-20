"""Real release CLI acceptance using original packages and inert Git update fixtures."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import logging
from pathlib import Path
import shutil
import signal
import sys

from source_transport_fixture import Processes


SELECTED = [
    ("davila7-templates", "skill-business-marketing-seo-optimizer", "claude-code", ".claude/skills"),
    ("jcodesmore-website-cloner", "skill-jcodesmore-clone-website", "hermes", ".hermes/skills"),
    ("morluto-rea", "skill-morluto-reverse-engineer-anything", "opencode", ".opencode/skills"),
    ("newliver666-apk-reverse", "skill-newliver666-apk-reverse", "cursor", ".cursor/skills"),
    ("kdense-scientific-agent-skills", "skill-kdense-statistical-analysis", "claude-code", ".claude/skills"),
    ("voltagent-design", "skill-voltagent-design-voltagent", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-pocketbase-pb-api-rules", "claude-code", ".claude/skills"),
    ("microsoft-skills", "skill-web-development-zustand-store-ts", "cursor", ".cursor/skills"),
    ("n8n-skills", "skill-workflow-automation-n8n-workflow-patterns", "opencode", ".opencode/skills"),
    ("openai-skills", "skill-productivity-notion-knowledge-capture", "claude-code", ".claude/skills"),
    ("wshobson-guides", "skill-development-c-pro", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-creative-design-ui-ux-pro-max", "claude-code", ".claude/skills"),
    ("wshobson-guides", "skill-development-grafana-dashboards", "claude-code", ".claude/skills"),
    ("wshobson-guides", "skill-development-prometheus-configuration", "claude-code", ".claude/skills"),
    ("wshobson-guides", "skill-workflow-automation-github-actions-templates", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-productivity-nowait", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-development-android-cicd", "claude-code", ".claude/skills"),
    ("clinpgx-historical-originals", "skill-scientific-clinpgx-database", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-development-fastmcp-server", "claude-code", ".claude/skills"),
    ("davila7-templates", "skill-doordash-doordash-allergy-shield", "claude-code", ".claude/skills"),
]


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def subset(project, workspace):
    registry = json.loads((project / "registry/collections.json").read_text(encoding="utf-8"))
    lock = json.loads((project / "catalogs/collections.lock.json").read_text(encoding="utf-8"))
    selected = {package for _, package, _, _ in SELECTED}
    # The second same-name original is a real independent source, not a fake duplicate main file.
    selected.add("skill-scientific-statistical-analysis")
    entries = {k: e for k, e in lock["entries"].items() if e["id"] in selected}
    # Use a guaranteed registered same-name alternative if the older collection has another ID.
    if len(entries) == len(SELECTED):
        old = next(e for e in lock["entries"].values() if e["collection"] == "davila7-templates" and e["root"].endswith("/statistical-analysis"))
        entries[f"{old['collection']}/{old['id']}"] = old
    for entry in entries.values():
        shutil.copytree(project / "packages/claude-code" / entry["id"], workspace / "packages/claude-code" / entry["id"])
    collections = []
    for collection in registry["collections"]:
        skills = [s for s in collection["skills"] if s["id"] in {e["id"] for e in entries.values()}]
        if skills:
            collections.append(collection | {"skills": skills})
    write(workspace / "registry/collections.json", {"schema_version": 1, "collections": collections})
    write(workspace / "catalogs/collections.lock.json", {"schema_version": 1, "entries": entries})
    for path in [".claude-plugin/marketplace.json", ".cursor-plugin/marketplace.json"]:
        catalog = json.loads((project / path).read_text(encoding="utf-8"))
        catalog["plugins"] = [p for p in catalog["plugins"] if p["name"] in {e["id"] for e in entries.values()}]
        write(workspace / path, catalog)
    for collection in collections:
        for skill in collection["skills"]:
            for path in skill.get("reviewed_distribution", {}).get("supplements", {}):
                target = workspace / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(project / path, target)
            for notice in skill.get("notices", []):
                target = workspace / notice["path"]
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(project / notice["path"], target)
    return entries


def verify(application, project, work):
    fixture = work / "fixture"
    fixture.mkdir()
    home = fixture / "home"
    home.mkdir()
    owner = Processes(home, seconds=180)
    record = {"schema": 1, "outcome": "failed", "original_execution": "not-performed", "packages": [], "checks": []}
    previous = signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(TimeoutError("CLI fixture cancelled")))
    try:
        workspace = fixture / "original distribution"
        workspace.mkdir()
        entries = subset(project, workspace)

        def cli(root, *args, error=None, exit_code=0, offline=True):
            argv = [str(application), "--repository", str(root), "--json", "--non-interactive", "--timeout", "20"]
            if offline:
                argv.append("--offline")
            code, output, _ = owner.run(argv + list(args), expected=None, seconds=25)
            if code != (3 if error is not None else exit_code):
                logging.error("Unexpected CLI outcome command=%s exit=%d", args[0], code)
                raise AssertionError("Actual CLI command status mismatch")
            value = json.loads(output.decode("utf-8"))
            if error is not None:
                assert value["status"] == "error" and value["code"] == error
            logging.info("CLI command=%s exit=%d output_digest=%s", args[0], code, hashlib.sha256(output).hexdigest())
            return value

        catalog = cli(project, "catalog", "list")
        assert len(catalog) == len(json.loads((project / "catalogs/collections.lock.json").read_text(encoding="utf-8"))["entries"])
        assert not any("error" in p for p in catalog)
        record["checks"].append("full-catalog")
        for collection, package, target, prefix in SELECTED:
            entry = entries[f"{collection}/{package}"]
            adapter = entry.get("reviewed_guide") or entry.get("reviewed_design")
            name = adapter["name"] if adapter else next(
                f["output_path"].split("/skills/", 1)[1].split("/")[0]
                for f in entry["files"] if f["path"] == entry["root"] + "/SKILL.md")
            destination = fixture / (package + " consumer")
            destination.mkdir()
            unrelated = destination / "unrelated.txt"
            unrelated.write_text("Keep consumer content\n", encoding="utf-8")
            cli(workspace, "package", "inspect", package)
            cli(workspace, "build", package, "--target", target, "--locked")
            native_preview = cli(workspace, "install", package, "--target", target, "--destination", str(destination), "--dry-run")
            assert not (destination / prefix).exists()
            native_required = bool(entry.get("reviewed_distribution", {}).get("native"))
            cli(workspace, "install", package, "--target", "hermes-native", "--destination", str(destination), error="unsupported-required" if native_required else "native-install-required")
            assert not (destination / prefix).exists()
            approval = ["--native-approval", native_preview["native_approval"]] if native_required else []
            receipt = cli(workspace, "install", package, "--target", target, "--destination", str(destination), *approval)
            receipt_id = receipt["id"]
            if native_required:
                native = entry["reviewed_distribution"]["native"]
                for source, path in native["files"].items():
                    expected = next(f["digest"] for f in entry["files"] if f["path"] == source)
                    assert digest(destination / path) == expected
                settings = json.loads((destination / ".claude/settings.json").read_text(encoding="utf-8"))
                assert len(settings["hooks"]["PreToolUse"]) == 3 and len(settings["hooks"]["PostToolUse"]) == 1
                cli(workspace, "install", package, "--target", "codex", "--destination", str(destination), error="unsupported-required")
            installed = destination / prefix / name
            for file in entry["files"]:
                skill_path = file["output_path"].split(f"packages/claude-code/{package}/skills/{name}/", 1)
                if len(skill_path) == 2:
                    copied = installed / skill_path[1]
                    assert digest(copied) == file["digest"]
                    assert bool(copied.stat().st_mode & 0o111) == (file["mode"] == "100755")
            for path, expected in receipt["files"].items():
                assert digest(destination / path) == expected
            preview = cli(workspace, "install", package, "--target", target, "--destination", str(destination), "--dry-run")
            assert preview["changes"] == 0
            original = (installed / "SKILL.md").read_bytes()
            cli(workspace, "disable", receipt_id)
            assert not (installed / "SKILL.md").exists()
            cli(workspace, "enable", receipt_id)
            assert (installed / "SKILL.md").read_bytes() == original
            (installed / "SKILL.md").write_text("Consumer edit\n", encoding="utf-8")
            cli(workspace, "uninstall", receipt_id, error="conflict")
            assert (installed / "SKILL.md").read_text(encoding="utf-8") == "Consumer edit\n"
            (installed / "SKILL.md").write_bytes(original)
            if package == "skill-kdense-statistical-analysis":
                other = next(e for e in entries.values() if e["collection"] == "davila7-templates" and e["root"].endswith("/statistical-analysis"))
                cli(workspace, "package", "inspect", other["id"])
                cli(workspace, "build", other["id"], "--target", target, "--locked")
                cli(workspace, "install", other["id"], "--target", target, "--destination", str(destination), error="skill-name-conflict")
                assert (installed / "SKILL.md").read_bytes() == original
                record["checks"].append("distinct-source-name-collision")
            removed = cli(workspace, "uninstall", receipt_id)
            assert not (installed / "SKILL.md").exists()
            cli(workspace, "rollback", removed["operation"])
            assert (installed / "SKILL.md").read_bytes() == original
            cli(workspace, "uninstall", receipt_id)
            assert not (installed / "SKILL.md").exists()
            assert unrelated.read_text(encoding="utf-8") == "Keep consumer content\n"
            record["packages"].append({"id": package, "target": target, "files": len(receipt["files"]), "lifecycle": "passed"})
            print("Real CLI original lifecycle passed: " + package, flush=True)
        updates(fixture, owner, cli, record)
        record["outcome"] = "succeeded"
    finally:
        try:
            owner.close()
            record["owned_processes"] = len(owner.children)
            shutil.rmtree(fixture)
            record["fixture_removed"] = not fixture.exists()
        except Exception:
            record["outcome"] = "failed"
            raise
        finally:
            signal.signal(signal.SIGTERM, previous)
            write(work / "result.json", record)
            logging.info("Cleanup children=%d fixture_removed=%s", len(owner.children), record.get("fixture_removed", False))


def updates(fixture, owner, cli, record):
    upstream = fixture / "update source"
    (upstream / "skills/demo/scripts").mkdir(parents=True)
    prompt = upstream / "skills/demo/SKILL.md"
    prompt.write_text("---\nname: demo\ndescription: First-party actual CLI fixture\n---\nOriginal prompt.\n", encoding="utf-8")
    script = upstream / "skills/demo/scripts/helper.py"
    script.write_text("raise SystemExit('never executed')\n", encoding="utf-8")
    license = upstream / "LICENSE"
    license.write_text("MIT License\nPermission is hereby granted, free of charge\n", encoding="utf-8")

    def git(*args):
        return owner.run(["git", "-c", "core.hooksPath=", "-C", str(upstream), *args])[1].decode("utf-8").strip()

    git("init", "-b", "main")
    git("add", ".")
    git("commit", "-m", "fixture")
    workspace = fixture / "update workspace"
    workspace.mkdir()
    write(workspace / "registry/collections.json", {"schema_version": 1, "collections": [{"id": "fixture", "repository": "local:" + str(upstream), "branch": "main", "reviewed_revision": git("rev-parse", "HEAD"), "skills": [{"id": "fixture-demo", "root": "skills/demo", "license": "MIT", "license_path": "LICENSE", "license_digest": digest(license)}]}]})
    assert cli(workspace, "collection", "sync", "--apply")["status"] == "success"
    destination = fixture / "update consumer"
    destination.mkdir()
    receipt = cli(workspace, "install", "fixture-demo", "--target", "claude-code", "--destination", str(destination))
    prompt.write_text(prompt.read_text(encoding="utf-8").replace("Original prompt.", "Reviewed prompt update."), encoding="utf-8")
    git("add", "."); git("commit", "-m", "prompt")
    assert cli(workspace, "collection", "sync", "--apply")["status"] == "success"
    cli(workspace, "update", "--installed")
    installed = destination / ".claude/skills/demo"
    assert (installed / "SKILL.md").read_bytes() == prompt.read_bytes()
    accepted = (workspace / "catalogs/collections.lock.json").read_bytes()
    original_script = (installed / "scripts/helper.py").read_bytes()
    repeated = cli(workspace, "collection", "sync", "--apply")
    assert all(s["status"] == "unchanged" for s in repeated["skills"])
    assert (workspace / "catalogs/collections.lock.json").read_bytes() == accepted
    cli(workspace, "update", "--installed")
    assert (installed / "SKILL.md").read_bytes() == prompt.read_bytes()
    assert (installed / "scripts/helper.py").read_bytes() == original_script
    script.write_text("raise SystemExit('changed, never executed')\n", encoding="utf-8")
    git("add", "."); git("commit", "-m", "script")
    held = cli(workspace, "collection", "sync", "--apply", exit_code=2)
    assert held["status"] == "partial"
    assert len(held["skills"]) == 1 and held["skills"][0]["status"] == "review-required"
    assert (workspace / "catalogs/collections.lock.json").read_bytes() == accepted
    cli(workspace, "update", "--installed")
    assert (installed / "scripts/helper.py").read_bytes() == original_script
    cli(workspace, "uninstall", receipt["id"])
    assert not (installed / "SKILL.md").exists()
    record["checks"].extend(["actual-git-safe-update", "installed-update", "script-review-retention", "offline-repeat"])
    design = upstream / "design-md/sample/DESIGN.md"
    design.parent.mkdir(parents=True)
    design.write_text("# Original design\nReadable spacing.\n", encoding="utf-8")
    git("add", "."); git("commit", "-m", "design")
    design_workspace = fixture / "design workspace"
    design_workspace.mkdir()
    write(design_workspace / "registry/collections.json", {"schema_version": 1, "collections": [{"id": "design", "repository": "local:" + str(upstream), "branch": "main", "reviewed_revision": git("rev-parse", "HEAD"), "skills": [{"id": "skill-design-sample", "root": "design-md/sample", "license": "MIT", "license_path": "LICENSE", "license_digest": digest(license), "reviewed_design": {"name": "design-sample", "source_digest": digest(design), "adapter_version": 1}}]}]})
    assert cli(design_workspace, "collection", "sync", "--apply")["status"] == "success"
    inventory = cli(design_workspace, "collection", "design-list", "design")
    assert inventory["designs"][0]["status"] == "registered"
    design.write_text("# Updated design\nAccessible contrast.\n", encoding="utf-8")
    git("add", "."); git("commit", "-m", "design update")
    review = cli(design_workspace, "collection", "design-review", "design", "design-md/sample")
    assert review["status"] == "review-required" and "Accessible contrast" in review["diff"]
    cli(design_workspace, "collection", "design-accept", "design", "design-md/sample", "--revision", review["revision"], "--digest", "0" * 64, error="approval-stale")
    args = ("collection", "design-accept", "design", "design-md/sample", "--revision", review["revision"], "--digest", review["digest"])
    assert cli(design_workspace, *args)["version"] == "1.0.1"
    assert cli(design_workspace, *args)["status"] == "unchanged"
    cli(design_workspace, "collection", "verify")
    assert (design_workspace / "packages/claude-code/skill-design-sample/skills/design-sample/DESIGN.md").read_bytes() == design.read_bytes()
    record["checks"].append("actual-design-review-accept-repeat")


def main():
    if sys.platform != "linux":
        raise RuntimeError("Owned practical CLI tier requires Linux")
    parser = argparse.ArgumentParser()
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S")
    handler = logging.FileHandler(work / f"verify-cli-lifecycle_{stamp}_UTC.log", encoding="utf-8")
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [CLI] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = __import__("time").gmtime
    handler.setFormatter(formatter)
    logging.getLogger().addHandler(handler)
    logging.getLogger().setLevel(logging.INFO)
    try:
        verify(args.application.resolve(), project, work)
    finally:
        handler.close()
        logging.getLogger().removeHandler(handler)


if __name__ == "__main__":
    main()
