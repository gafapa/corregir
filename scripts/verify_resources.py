"""Verify the checked-in native binary, OCR models, and embedded font offline."""

import hashlib
import json
from pathlib import Path


def main() -> None:
    root = Path(__file__).resolve().parents[1] / "src-tauri" / "resources"
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    for resource in manifest["resources"]:
        path = (root / resource["path"]).resolve()
        if not path.is_relative_to(root.resolve()):
            raise SystemExit("Resource path escapes the resource directory")
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        if path.stat().st_size != resource["size_bytes"] or digest.hexdigest() != resource["sha256"]:
            raise SystemExit(f"Resource integrity check failed: {resource['path']}")
        print(f"Verified {resource['path']} ({resource['version']})")


if __name__ == "__main__":
    main()
