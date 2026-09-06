#!/usr/bin/env bash
# Pin Cloudflare Workers Builds so raynuv.com production comes from `main`
# and non-production Git branches cannot `wrangler deploy` over it.
#
# Preview builds, if enabled, must use `wrangler versions upload` (a version,
# not the production Worker). Feature branches often carry a truncated
# `site/` tree from before the site-updater PRs (#218–#226).
set -euo pipefail

TOKEN="${CLOUDFLARE_API_TOKEN:-}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:-}"
WORKER="${CLOUDFLARE_WORKER_NAME:-raynuv}"
API="https://api.cloudflare.com/client/v4/accounts/${ACCOUNT}"

if [[ -z "$TOKEN" || -z "$ACCOUNT" ]]; then
  echo "lock-cloudflare: skip (need CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID)"
  exit 0
fi

export CF_TOKEN="$TOKEN"
export CF_API="$API"
export CF_WORKER="$WORKER"

python3 <<'PY'
import json, os, sys, urllib.error, urllib.request

token = os.environ["CF_TOKEN"]
api = os.environ["CF_API"]
worker = os.environ["CF_WORKER"]


def req(method, url, body=None):
    data = None if body is None else json.dumps(body).encode()
    r = urllib.request.Request(
        url,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            return json.load(resp)
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")
        print(f"lock-cloudflare: {method} {url} -> HTTP {e.code}\n{detail}", file=sys.stderr)
        sys.exit(1)


scripts = req("GET", f"{api}/workers/scripts")
tag = None
for item in scripts.get("result") or []:
    if item.get("id") == worker:
        tag = item.get("tag")
        break
if not tag:
    print(f"lock-cloudflare: worker {worker!r} not found; skip", file=sys.stderr)
    sys.exit(0)

print(f"lock-cloudflare: worker {worker} tag={tag}")
triggers = req("GET", f"{api}/builds/workers/{tag}/triggers")
rows = triggers.get("result") or []
if not rows:
    print("lock-cloudflare: no Workers Builds triggers (Git integration may be off)")
    sys.exit(0)

changed = 0
for t in rows:
    uuid = t.get("trigger_uuid") or t.get("id")
    name = (t.get("trigger_name") or "").lower()
    includes = list(t.get("branch_includes") or [])
    deploy = t.get("deploy_command") or ""
    is_preview = (
        "non-production" in name
        or "preview" in name
        or "*" in includes
        or any(x not in ("main",) for x in includes if x)
    ) and "production" not in name

    patch = {}
    if is_preview:
        wanted = "npx wrangler versions upload"
        if deploy.strip() != wanted:
            patch["deploy_command"] = wanted
        if "main" not in (t.get("branch_excludes") or []):
            patch["branch_excludes"] = sorted(set((t.get("branch_excludes") or []) + ["main"]))
    else:
        if includes != ["main"]:
            patch["branch_includes"] = ["main"]
        if "wrangler deploy" not in deploy:
            patch["deploy_command"] = "npx wrangler deploy"

    if not patch:
        print(f"lock-cloudflare: ok {t.get('trigger_name')} includes={includes} deploy={deploy!r}")
        continue
    print(f"lock-cloudflare: patch {t.get('trigger_name')} {patch}")
    req("PATCH", f"{api}/builds/triggers/{uuid}", patch)
    changed += 1

print(f"lock-cloudflare: production branch pinned to main ({changed} trigger(s) updated)")
PY
