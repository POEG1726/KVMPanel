#!/usr/bin/env python3
"""Create full Linux source checkouts from the project's local bare mirrors."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(args: list[str], cwd: Path | None = None) -> str:
    r = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if r.returncode:
        raise RuntimeError(r.stderr.strip() or r.stdout.strip())
    return r.stdout.strip()


def main() -> int:
    if sys.platform != "linux":
        raise SystemExit("Run this tool inside Ubuntu WSL / Linux, not native Windows.")
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--workspace", type=Path, default=Path.home() / "work/KVMPanel")
    p.add_argument("ids", nargs="*")
    args = p.parse_args()
    workspace = args.workspace.expanduser().resolve()
    if str(workspace).startswith("/mnt/"):
        raise SystemExit("Use the Linux filesystem for source checkouts, not /mnt/<drive>.")
    lock = json.loads((ROOT / "manifests/sources.lock.json").read_text())
    known = {s["id"] for s in lock["sources"]}
    if set(args.ids) - known:
        p.error("Unknown source IDs")
    results = []
    for source in lock["sources"]:
        if args.ids and source["id"] not in args.ids:
            continue
        mirror = (ROOT / source["mirror"]).resolve()
        if not mirror.is_relative_to((ROOT / "sources/repos").resolve()):
            raise RuntimeError("Source path escapes the mirror directory")
        if run(["git", "rev-parse", "HEAD"], mirror) != source["expected_commit"]:
            raise RuntimeError("Mirror commit mismatch: " + source["id"])
        dest = workspace / "upstream" / source["id"]
        if not dest.exists():
            dest.parent.mkdir(parents=True, exist_ok=True)
            run(["git", "clone", "--no-hardlinks", str(mirror), str(dest)])
            run(["git", "remote", "rename", "origin", "upstream"], dest)
            run(["git", "remote", "set-url", "upstream", source["url"]], dest)
            run(["git", "switch", "--detach", source["expected_commit"]], dest)
        if run(["git", "status", "--porcelain"], dest):
            raise RuntimeError("Existing checkout has changes; preserved: " + str(dest))
        commit = run(["git", "rev-parse", "HEAD"], dest)
        if commit != source["expected_commit"]:
            raise RuntimeError("Existing checkout differs; preserved: " + str(dest))
        if run(["git", "remote", "get-url", "upstream"], dest) != source["url"]:
            raise RuntimeError("Existing remote differs; preserved: " + str(dest))
        tree_count = len(run(["git", "ls-tree", "-r", "--name-only", "HEAD"], dest).splitlines())
        index_count = len(run(["git", "ls-files"], dest).splitlines())
        if tree_count != index_count:
            raise RuntimeError("Incomplete checkout: " + source["id"])
        results.append({"id": source["id"], "path": str(dest), "commit": commit,
                        "tracked_files": tree_count, "status": "clean-complete-checkout"})
        print(f"Ready {source['id']}: {dest} ({tree_count} files)", flush=True)
    (workspace / "build").mkdir(exist_ok=True)
    state = {"checked_at_utc": datetime.now(timezone.utc).isoformat(),
             "workspace": str(workspace), "sources": results}
    (ROOT / "sources/workspaces.json").write_text(json.dumps(state, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
