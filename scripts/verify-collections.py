"""Verify copied collection bytes/modes and native catalogs without executing payloads."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parent.parent
lock = json.loads((root / "catalogs/collections.lock.json").read_text(encoding="utf-8"))
registry = json.loads((root / "registry/collections.json").read_text(encoding="utf-8"))
market = json.loads((root / ".claude-plugin/marketplace.json").read_text(encoding="utf-8"))
registered = {f"{c['id']}/{s['id']}": s for c in registry["collections"] for s in c["skills"]}
entries = lock["entries"]
assert set(entries) == set(registered), "Selected roots not completely packaged"
assert len(market["plugins"]) == len(entries), "Marketplace count mismatch"
index = subprocess.run(["git", "ls-files", "--stage", "-z"], cwd=root, check=True, capture_output=True).stdout.decode("utf-8")
modes = {row.split("\t", 1)[1]: row.split(" ", 1)[0] for row in index.split("\0") if row}
count = 0
for key, entry in entries.items():
    skill = registered[key]
    assert entry["root"] == skill["root"] and entry["license"] == skill["license"]
    prefix = f"packages/claude-code/{entry['id']}/"
    owned = entry["generated"] | entry.get("delivery", {})
    assert not set(entry["generated"]) & set(entry.get("delivery", {})), "Ownership overlap"
    expected = set(owned)
    actual = {str(path.relative_to(root)).replace("\\", "/") for path in (root / prefix).rglob("*") if path.is_file()}
    assert actual == expected, f"Unexpected plugin files: {entry['id']}"
    for name, digest in owned.items():
        assert name.startswith(prefix) and ".." not in Path(name).parts
        assert not (root / name).is_symlink()
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
    for file in entry["files"]:
        name = file["output_path"]
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == file["digest"], name
        assert file["digest"] == entry["generated"][name]
        assert modes.get(name) == file["mode"], f"Git mode mismatch: {name}"
        if sys.platform != "win32":
            assert bool((root / name).stat().st_mode & 0o111) == (file["mode"] == "100755"), name
        count += 1
    design = skill.get("reviewed_design")
    assert entry.get("reviewed_design") == design, "Design approval changed"
    if design:
        assert design["adapter_version"] == 1 and not skill.get("reviewed_name")
        name = design["name"]
        assert 1 <= len(name) <= 64 and all(c in "abcdefghijklmnopqrstuvwxyz0123456789-" for c in name)
        assert not name.startswith("-") and not name.endswith("-") and "--" not in name
        source = next(f for f in entry["files"] if f["path"] == skill["root"] + "/DESIGN.md")
        assert source["digest"] == design["source_digest"]
        assert source["output_path"] == prefix + "skills/" + name + "/DESIGN.md"
        assert not any(f["path"] == skill["root"] + "/SKILL.md" for f in entry["files"])
        generated_entry = prefix + "skills/" + name + "/SKILL.md"
        text = (root / generated_entry).read_text(encoding="utf-8")
        assert text.splitlines()[:2] == ["---", "name: " + name]
        assert text.splitlines()[2].startswith("description: ")
        assert "[DESIGN.md](DESIGN.md)" in text and "Numera Skill Forge" in text and "third-party" in text
        assert "adapter-version: '1'" in text
        assert generated_entry in entry["generated"]
    adapted = [f for f in entry["files"] if f.get("source_digest")]
    approval = skill.get("reviewed_name")
    if approval:
        original = (root / prefix / "upstream-notices/source-entry.md").read_bytes()
        assert hashlib.sha256(original).hexdigest() == approval["source_digest"]
        text = original.decode("utf-8")
        front = text.removeprefix("---\n").split("\n---", 1)[0]
        line = "name: " + approval["source_name"]
        assert [s for s in front.splitlines() if s.startswith("name:")] == [line]
        offset = 4 + sum(len(s.encode("utf-8")) for s in front.splitlines(keepends=True)[:front.splitlines().index(line)])
        expected = original[:offset] + ("name: " + approval["name"]).encode("utf-8") + original[offset + len(line.encode("utf-8")):]
        assert len(adapted) == 1 and adapted[0]["path"] == skill["root"] + "/SKILL.md"
        assert adapted[0]["source_digest"] == approval["source_digest"]
        assert adapted[0]["output_path"] == prefix + "skills/" + approval["name"] + "/SKILL.md"
        assert (root / adapted[0]["output_path"]).read_bytes() == expected
        change = json.loads((root / prefix / "upstream-notices/adaptation.json").read_text(encoding="utf-8"))
        assert change["approval"] == approval and change["output_digest"] == hashlib.sha256(expected).hexdigest()
    else:
        assert not adapted, "Unapproved name adaptation"
    if skill["license_path"]:
        evidence = next(file for file in entry["files"] if file["path"] == skill["license_path"])
        assert evidence["digest"] == skill["license_digest"], "Reviewed license changed"
    else:
        assert skill.get("notices"), "Original grant missing"
        assert skill["notices"][0]["digest"] == skill["license_digest"]
    for name, digest in skill.get("reviewed_documents", {}).items():
        assert ((skill["root"].startswith("skills/") and len(Path(skill["root"]).parts) == 2)
                or (skill["root"].startswith("cli-tool/components/skills/") and len(Path(skill["root"]).parts) == 5))
        assert name == skill["root"] + "/references/secrets.md"
        evidence = next(file for file in entry["files"] if file["path"] == name)
        assert evidence["digest"] == digest, "Reviewed public document changed"
    for index, notice in enumerate(skill.get("notices", [])):
        original = root / notice["path"]
        copied = root / prefix / f"upstream-notices/original-{index}.txt"
        assert hashlib.sha256(original.read_bytes()).hexdigest() == notice["digest"]
        assert copied.read_bytes() == original.read_bytes(), "Original notice changed during packaging"
    manifest = json.loads((root / prefix / ".claude-plugin/plugin.json").read_text(encoding="utf-8"))
    assert manifest["name"] == entry["id"] and manifest["version"] == entry["version"]
    assert manifest["license"] == entry["license"]
    assert not any(field in manifest for field in ("hooks", "mcpServers", "lspServers", "workflows")), "Unapproved native runtime declaration"
    catalog_entry = next(plugin for plugin in market["plugins"] if plugin["name"] == entry["id"])
    assert catalog_entry["source"] == "./" + prefix.rstrip("/")
    for descriptor in (".codex-plugin/plugin.json", ".cursor-plugin/plugin.json", "plugin.json"):
        native = json.loads((root / prefix / descriptor).read_text(encoding="utf-8"))
        assert native["name"] == entry["id"] and native["version"] == entry["version"]
        assert native["license"] == entry["license"]
        assert not any(field in native for field in ("hooks", "mcpServers", "lspServers", "workflows"))
        if descriptor != "plugin.json":
            assert native["skills"] == "./skills/"
        assert prefix + descriptor in entry["delivery"]
for path in (".claude-plugin/marketplace.json", ".cursor-plugin/marketplace.json"):
    native = json.loads((root / path).read_text(encoding="utf-8"))
    assert {p["name"] for p in native["plugins"]} == {e["id"] for e in entries.values()}
    assert len(native["plugins"]) == len(entries)
    for plugin in native["plugins"]:
        expected = f"packages/claude-code/{plugin['name']}"
        assert plugin["source"] == ("./" + expected if path.startswith(".claude-plugin") else expected)
print(f"Verified {len(entries)} plugins and {count} exact payload files/modes plus native catalogs; upstream code not executed")
