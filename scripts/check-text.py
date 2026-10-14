"""Validate tracked UTF-8 text and absolute publication exclusions."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parent.parent
paths = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True, check=True).stdout.decode("utf-8").split("\0")
blocked = {".ai", ".specify", "specs", ".claude", ".codex", ".ignoreme", "graphify-out", ".codebase-memory"}
binary = {".png", ".ico", ".icns", ".jpg", ".zip"}
registry = json.loads((root / "registry/collections.json").read_text(encoding="utf-8"))
lock = json.loads((root / "catalogs/collections.lock.json").read_text(encoding="utf-8"))
reviewed = {}
for collection in registry["collections"]:
    for skill in collection["skills"]:
        for source, digest in skill.get("reviewed_documents", {}).items():
            if (not skill["root"].startswith("skills/") or len(Path(skill["root"]).parts) != 2
                    or source != skill["root"] + "/references/secrets.md"
                    or any(p in blocked for p in Path(source).parts[:-1])):
                raise SystemExit("Invalid reviewed public document path")
            entry = lock["entries"][collection["id"] + "/" + skill["id"]]
            output = f"packages/claude-code/{skill['id']}/{source}"
            if not any(f["path"] == source and f["output_path"] == output and f["digest"] == digest
                       for f in entry["files"]):
                raise SystemExit("Reviewed document differs from accepted identity")
            reviewed[output] = digest
for name in filter(None, paths):
    path = Path(name)
    if name in reviewed:
        if (root / path).is_symlink() or hashlib.sha256((root / path).read_bytes()).hexdigest() != reviewed[name]:
            raise SystemExit("Reviewed public document bytes changed")
        (root / path).read_text(encoding="utf-8", errors="strict")
        continue
    if (path.parts[0] in blocked or path.name in {"secrets.md", "explain-AI.md"}
            or path.parts[0] == ".agents"):
        raise SystemExit("Protected content was included in tracked output")
    # Exact upstream payloads are byte-verified separately, including fonts/media and legacy text.
    if path.parts[0] == "packages":
        continue
    if path.suffix.lower() not in binary:
        (root / path).read_text(encoding="utf-8", errors="strict")
print("Tracked text is UTF-8; publication exclusions passed")
