#!/usr/bin/env python3
"""Pick a GitHub Actions run that uploaded r640-hypervisor.efi.

Prefer a successful pull_request run (full CI including QEMU) over push.
Push-only QEMU #DF flakes must not hide a green PR artifact.

Pillar: [Z] [D]. Outside Proven Core.
"""
from __future__ import annotations

import argparse
import json
import sys
from typing import Any, Iterable, Optional

PENDING_STATUSES = frozenset(
    {"queued", "in_progress", "requested", "waiting", "pending", "action_required"}
)


def _sha(run: dict[str, Any]) -> str:
    return str(run.get("headSha") or "").lower()


def _newest(runs: Iterable[dict[str, Any]]) -> Optional[dict[str, Any]]:
    items = list(runs)
    if not items:
        return None
    return max(items, key=lambda r: str(r.get("createdAt") or ""))


def is_success(run: dict[str, Any]) -> bool:
    return run.get("status") == "completed" and run.get("conclusion") == "success"


def is_pending(run: dict[str, Any]) -> bool:
    status = str(run.get("status") or "")
    return status in PENDING_STATUSES


def is_uefi_fallback(run: dict[str, Any]) -> bool:
    """Completed run that may still have a UEFI artifact (overall job failed)."""
    if run.get("status") != "completed":
        return False
    return run.get("conclusion") in ("failure", "timed_out", "cancelled")


def pick_run(
    runs: list[dict[str, Any]],
    head_sha: str,
    *,
    allow_uefi_only: bool = False,
    allow_branch_fallback: bool = True,
) -> dict[str, Any]:
    """Return a pick dict: {run, match, pending}.

    match is 'head-pr', 'head-push', 'head-uefi-only', or 'branch-fallback'.
    """
    want = (head_sha or "").lower()
    head_runs = [r for r in runs if _sha(r) == want] if want else []

    pr = [r for r in head_runs if is_success(r) and r.get("event") == "pull_request"]
    push = [r for r in head_runs if is_success(r) and r.get("event") == "push"]
    chosen = _newest(pr)
    if chosen is not None:
        return {"run": chosen, "match": "head-pr", "pending": []}
    chosen = _newest(push)
    if chosen is not None:
        return {"run": chosen, "match": "head-push", "pending": []}

    pending = [r for r in head_runs if is_pending(r)]
    if allow_uefi_only:
        uefi = [r for r in head_runs if is_uefi_fallback(r)]
        chosen = _newest(uefi)
        if chosen is not None:
            return {"run": chosen, "match": "head-uefi-only", "pending": pending}

    if pending:
        return {"run": None, "match": "pending", "pending": pending}

    if allow_branch_fallback:
        pr_all = [r for r in runs if is_success(r) and r.get("event") == "pull_request"]
        chosen = _newest(pr_all)
        if chosen is None:
            chosen = _newest(r for r in runs if is_success(r))
        if chosen is not None:
            return {"run": chosen, "match": "branch-fallback", "pending": []}

    return {"run": None, "match": "none", "pending": []}


def format_pick(result: dict[str, Any]) -> str:
    run = result.get("run")
    if not run:
        return ""
    return "{id}\t{event}\t{sha}\t{match}\t{url}".format(
        id=run.get("databaseId"),
        event=run.get("event"),
        sha=run.get("headSha"),
        match=result.get("match"),
        url=run.get("url"),
    )


def _self_test() -> None:
    runs = [
        {
            "databaseId": 111,
            "event": "push",
            "headSha": "aaa",
            "status": "completed",
            "conclusion": "success",
            "createdAt": "2026-08-21T10:00:00Z",
            "url": "https://example/111",
        },
        {
            "databaseId": 222,
            "event": "pull_request",
            "headSha": "aaa",
            "status": "completed",
            "conclusion": "success",
            "createdAt": "2026-08-21T10:01:00Z",
            "url": "https://example/222",
        },
        {
            "databaseId": 333,
            "event": "push",
            "headSha": "aaa",
            "status": "completed",
            "conclusion": "failure",
            "createdAt": "2026-08-21T10:02:00Z",
            "url": "https://example/333",
        },
        {
            "databaseId": 444,
            "event": "pull_request",
            "headSha": "bbb",
            "status": "completed",
            "conclusion": "success",
            "createdAt": "2026-08-21T09:00:00Z",
            "url": "https://example/444",
        },
        {
            "databaseId": 555,
            "event": "push",
            "headSha": "ccc",
            "status": "in_progress",
            "conclusion": "",
            "createdAt": "2026-08-21T11:00:00Z",
            "url": "https://example/555",
        },
    ]
    r = pick_run(runs, "aaa")
    assert r["match"] == "head-pr" and r["run"]["databaseId"] == 222, r
    r = pick_run(runs, "ccc")
    assert r["match"] == "pending" and r["run"] is None, r
    r = pick_run(runs, "ccc", allow_branch_fallback=True)
    assert r["match"] == "pending", r
    r = pick_run(runs, "ddd")
    assert r["match"] == "branch-fallback" and r["run"]["databaseId"] == 222, r
    r = pick_run(runs, "ddd", allow_branch_fallback=False)
    assert r["match"] == "none", r
    r = pick_run(runs, "aaa", allow_uefi_only=True)
    assert r["match"] == "head-pr", r
    only_fail = [runs[2]]
    r = pick_run(only_fail, "aaa", allow_uefi_only=True, allow_branch_fallback=False)
    assert r["match"] == "head-uefi-only" and r["run"]["databaseId"] == 333, r
    print("RAYNU-V-FLASHCRUZER-PICK-SELFTEST-OK")


def main(argv: Optional[list[str]] = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--json-file", help="gh run list JSON (array)")
    p.add_argument("--head", default="", help="git HEAD sha to match")
    p.add_argument("--allow-uefi-only", action="store_true")
    p.add_argument("--no-branch-fallback", action="store_true")
    args = p.parse_args(argv)

    if args.self_test:
        _self_test()
        return 0

    if not args.json_file:
        print("error: --json-file is required (or pass --self-test)", file=sys.stderr)
        return 2

    with open(args.json_file, encoding="utf-8") as f:
        runs = json.load(f)
    if not isinstance(runs, list):
        print("error: JSON must be an array of runs", file=sys.stderr)
        return 2

    result = pick_run(
        runs,
        args.head,
        allow_uefi_only=args.allow_uefi_only,
        allow_branch_fallback=not args.no_branch_fallback,
    )
    match = result["match"]
    if match == "pending":
        pending = result["pending"]
        ids = ",".join(str(r.get("databaseId")) for r in pending)
        print(f"PENDING\t{ids}", file=sys.stderr)
        return 3
    if match == "none" or result["run"] is None:
        print("error: no CI run with r640-hypervisor.efi", file=sys.stderr)
        return 1
    print(format_pick(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
