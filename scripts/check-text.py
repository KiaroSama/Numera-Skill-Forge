"""Validate tracked UTF-8 text and absolute publication exclusions."""
from pathlib import Path
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parent.parent
paths = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True, check=True).stdout.decode("utf-8").split("\0")
blocked = {".ai", ".specify", "specs", ".claude", ".codex", ".ignoreme", "graphify-out", ".codebase-memory"}
binary = {".png", ".ico", ".icns", ".jpg", ".zip"}
for name in filter(None, paths):
    path = Path(name)
    if path.parts[0] in blocked or path.name in {"secrets.md", "explain-AI.md"}:
        raise SystemExit("Protected content was included in tracked output")
    if path.suffix.lower() not in binary:
        (root / path).read_text(encoding="utf-8", errors="strict")
print("Tracked text is UTF-8; publication exclusions passed")
