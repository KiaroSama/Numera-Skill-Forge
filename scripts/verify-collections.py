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
    expected = set(entry["generated"])
    actual = {str(path.relative_to(root)).replace("\\", "/") for path in (root / prefix).rglob("*") if path.is_file()}
    assert actual == expected, f"Unexpected plugin files: {entry['id']}"
    for name, digest in entry["generated"].items():
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
    evidence = next(file for file in entry["files"] if file["path"] == skill["license_path"])
    assert evidence["digest"] == skill["license_digest"], "Reviewed license changed"
    manifest = json.loads((root / prefix / ".claude-plugin/plugin.json").read_text(encoding="utf-8"))
    assert manifest["name"] == entry["id"] and manifest["version"] == entry["version"]
    assert manifest["license"] == entry["license"]
    assert not any(field in manifest for field in ("hooks", "mcpServers", "lspServers", "workflows")), "Unapproved native runtime declaration"
    catalog_entry = next(plugin for plugin in market["plugins"] if plugin["name"] == entry["id"])
    assert catalog_entry["source"] == "./" + prefix.rstrip("/")
print(f"Verified {len(entries)} plugins and {count} exact payload files/modes; upstream code not executed")
