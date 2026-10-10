"""Record unsigned native bundle integrity, provenance and locked dependency licenses."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import logging
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--runner", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    log_dir = root / ".ci-work/distribution-logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d_%H-%M-%S_UTC")
    log_file = log_dir / f"distribution-manifest_{stamp}_{os.getpid()}.log"
    if log_file.exists():
        raise FileExistsError("A diagnostic run identity already exists")
    logging.basicConfig(filename=log_file, encoding="utf-8", level=logging.INFO,
                        format="[%(asctime)s UTC] [%(levelname)s] [DISTRIBUTION] %(message)s",
                        datefmt="%Y-%m-%d %H:%M:%S")
    logging.Formatter.converter = time.gmtime
    try:
        bundle = args.bundle.resolve(strict=True)
        bundle.relative_to(root / "target")
        if not bundle.is_dir() or args.bundle.is_symlink():
            raise ValueError("Select a real project-owned bundle directory")
        outputs = {"distribution-manifest.json", "SHA256SUMS.txt"}
        files = []
        links = []
        for path in sorted(bundle.rglob("*")):
            if path.is_symlink():
                path.resolve(strict=True).relative_to(bundle)
                link = os.readlink(path)
                if Path(link).is_absolute():
                    raise ValueError("Bundle link is not portable")
                links.append({"path": path.relative_to(bundle).as_posix(), "target": link})
                continue
            if not path.is_file() or path.name in outputs:
                continue
            relative = path.relative_to(bundle).as_posix()
            if any(part.lower() in {".ai", ".claude", ".specify", ".ignoreme", "secrets.md", "explain-ai.md"} or part.lower().startswith(".env") for part in path.relative_to(bundle).parts):
                raise ValueError("Bundle contains protected local content")
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            files.append({"path": relative, "bytes": path.stat().st_size, "sha256": digest.hexdigest()})
        if not files:
            raise ValueError("Bundle contains no distribution files")
        sha = os.environ.get("GITHUB_SHA")
        if not sha or len(sha) != 40 or any(c not in "0123456789abcdef" for c in sha):
            raise ValueError("Verified workflow commit identity is required")
        # Read existing locked metadata only; this never installs or builds a package.
        rust = subprocess.run(["rustc", "-vV"], cwd=root, stdin=subprocess.DEVNULL, capture_output=True,
                              timeout=10, check=True, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        host = next(line.removeprefix("host: ") for line in rust.stdout.decode("utf-8").splitlines() if line.startswith("host: "))
        command = subprocess.run(["cargo", "metadata", "--locked", "--offline", "--filter-platform", host, "--format-version", "1"],
                                 cwd=root, stdin=subprocess.DEVNULL, capture_output=True, timeout=30, check=True,
                                 creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        cargo = json.loads(command.stdout.decode("utf-8"))
        dependencies = [{"name": p["name"], "version": p["version"], "license": p.get("license"), "license_file": Path(p["license_file"]).name if p.get("license_file") else None, "source": p.get("source")} for p in cargo["packages"]]
        npm_lock = json.loads((root / "apps/desktop/package-lock.json").read_text(encoding="utf-8"))
        npm_dependencies = [{"package": name, "version": p.get("version"), "license": p.get("license"), "development": p.get("dev", False)}
                            for name, p in npm_lock["packages"].items() if name]
        manifest = {"schema_version": 1, "commit": sha, "runner": args.runner,
                    "architecture": platform.machine(), "signing": "unsigned; no signing or notarization asserted",
                    "native_runtime": "Linux smoke verified separately; Windows/macOS startup not certified here",
                    "files": files, "internal_links": links, "cargo_dependencies": sorted(dependencies, key=lambda p: (p["name"], p["version"])),
                    "frontend_locked_dependencies": sorted(npm_dependencies, key=lambda p: p["package"])}
        (bundle / "distribution-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        (bundle / "SHA256SUMS.txt").write_text("".join(f"{file['sha256']}  {file['path']}\n" for file in files), encoding="utf-8")
        logging.info("Recorded unsigned bundle files=%s dependencies=%s commit=%s", len(files), len(dependencies), sha)
        print(json.dumps({"status": "success", "files": len(files), "commit": sha, "signing": "unsigned"}))
    except BaseException:
        logging.exception("Distribution manifest failed")
        raise
    finally:
        logging.info("Manifest operation ended")
        logging.shutdown()


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
