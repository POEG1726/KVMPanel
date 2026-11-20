#!/usr/bin/env python3
"""Fetch and verify pinned source snapshots; never build or flash hardware."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "manifests/sources.lock.json"


def git(args: list[str], cwd: Path | None = None) -> str:
    result = subprocess.run(["git", *args], cwd=cwd, text=True,
                            encoding="utf-8", errors="replace",
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout.strip()


def mirror_path(source: dict) -> Path:
    path = (ROOT / source["mirror"]).resolve()
    if not path.is_relative_to((ROOT / "sources/repos").resolve()):
        raise ValueError("Source destination escapes sources/repos")
    return path


def verify(source: dict, full: bool = False) -> dict:
    path = mirror_path(source)
    if not path.is_dir():
        raise FileNotFoundError(f"Missing mirror: {path}")
    if git(["rev-parse", "--is-bare-repository"], path) != "true":
        raise RuntimeError(f"Expected bare source repository: {path}")
    commit = git(["rev-parse", "HEAD"], path)
    if commit != source["expected_commit"]:
        raise RuntimeError(f"Commit mismatch: {commit} != {source['expected_commit']}")
    if git(["remote", "get-url", "upstream"], path) != source["url"]:
        raise RuntimeError("Source remote URL differs from lock file")
    promisor = subprocess.run(["git", "config", "--get-regexp", r"remote\..*\.promisor"],
                             cwd=path, capture_output=True, text=True)
    if promisor.stdout.strip():
        raise RuntimeError("Partial/promisor mirror is not a complete source snapshot")
    for name in source["required"]:
        git(["cat-file", "-e", f"{commit}:{name}"], path)
    git(["fsck", "--full" if full else "--connectivity-only", "--no-dangling"], path)
    return {"id": source["id"], "status": "verified", "commit": commit,
            "mirror": source["mirror"], "coverage": source["coverage"],
            "shallow": git(["rev-parse", "--is-shallow-repository"], path) == "true",
            "integrity": "fsck-full" if full else "fsck-connectivity"}


def fetch(source: dict) -> dict:
    path = mirror_path(source)
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        args = ["clone", "--bare", "--depth", "1", "--single-branch", "--no-tags",
                "--origin", "upstream"]
        if source["ref"] != "HEAD":
            args.extend(["--branch", source["ref"]])
        print(f"Fetching {source['id']} ({source['ref']})", flush=True)
        git([*args, source["url"], str(path)])
    # A moving public branch must still match the explicitly pinned commit.
    try:
        actual = git(["rev-parse", "HEAD"], path)
        if actual != source["expected_commit"]:
            git(["fetch", "--depth", "1", "upstream", source["expected_commit"]], path)
            git(["update-ref", "--no-deref", "HEAD", source["expected_commit"]], path)
        result = verify(source, full=True)
        print(f"Verified {source['id']}: {result['commit']}", flush=True)
        return result
    except Exception as exc:
        raise RuntimeError(f"{source['id']}: {exc}") from exc


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["fetch", "verify"])
    parser.add_argument("ids", nargs="*", help="Default: all pinned sources")
    parser.add_argument("--jobs", type=int, default=3)
    parser.add_argument("--full", action="store_true", help="Full object integrity check")
    args = parser.parse_args()
    manifest = json.loads(LOCK.read_text(encoding="utf-8"))
    sources = manifest["sources"]
    known = {s["id"] for s in sources}
    if set(args.ids) - known:
        parser.error("Unknown source IDs: " + ", ".join(sorted(set(args.ids) - known)))
    selected = [s for s in sources if not args.ids or s["id"] in args.ids]
    results = []
    with ThreadPoolExecutor(max_workers=max(1, min(args.jobs, 5))) as executor:
        tasks = {executor.submit(fetch if args.action == "fetch" else verify,
                                 s, *([] if args.action == "fetch" else [args.full])): s
                 for s in selected}
        for future in as_completed(tasks):
            source = tasks[future]
            try:
                results.append(future.result())
            except Exception as exc:
                results.append({"id": source["id"], "status": "error", "error": str(exc)})
                print(f"ERROR {source['id']}: {exc}", file=sys.stderr, flush=True)
    state = {"checked_at_utc": datetime.now(timezone.utc).isoformat(),
             "action": args.action, "results": sorted(results, key=lambda x: x["id"]),
             "missing_vendor_packages": manifest["missing"]}
    state_path = ROOT / "sources/state.json"
    state_path.write_text(json.dumps(state, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(state, indent=2, ensure_ascii=False))
    return int(any(r["status"] != "verified" for r in results))


if __name__ == "__main__":
    raise SystemExit(main())
