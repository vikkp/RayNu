#!/usr/bin/env bash
# Restore (or check) public site chrome from a ref so iron/HDA merges do not
# overwrite layout/CSS/assets that landed on main.
#
# ./tools/sync-hda-site.sh writes ONLY site/hda.json.
# This script owns everything else under site/ except hda.json.
# site/hda.html is chrome + summit notes: --restore copies chrome from --from,
# then you re-apply summit notes from docs/hda.md (do not rewrite CSS).
#
# Usage:
#   ./tools/preserve-site-chrome.sh --restore [--from origin/main]
#   ./tools/preserve-site-chrome.sh --check   [--from origin/main]
#   ./tools/preserve-site-chrome.sh --self-test
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FROM="origin/main"
MODE=""

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --from) FROM="${2:-}"; shift 2 ;;
    --restore) MODE=restore; shift ;;
    --check) MODE=check; shift ;;
    --self-test) MODE=selftest; shift ;;
    *) echo "error: unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

if [[ -z "$MODE" ]]; then
  usage
  exit 2
fi

# Layout/CSS/assets/pages. Never hda.json (HDA numbers). hda.html is mixed.
CHROME_PATHS=(
  site/404.html
  site/robots.txt
  site/sitemap.xml
  site/styles.css
  site/index.html
  site/main.js
  site/stories.html
  site/paper.html
  site/ciospeak.html
  site/com2.html
  site/assets
)

cd "$ROOT"

if [[ "$MODE" == "selftest" ]]; then
  grep -q 'site/styles.css' "$0"
  grep -q 'hda.json' "$0"
  grep -q -- '--restore' "$0"
  grep -q -- '--check' "$0"
  echo "RAYNU-V-SITE-CHROME-OK"
  exit 0
fi

if ! git rev-parse --verify "$FROM^{commit}" >/dev/null 2>&1; then
  echo "error: missing ref $FROM (git fetch origin main?)" >&2
  exit 1
fi

if [[ "$MODE" == "restore" ]]; then
  echo "==> restore site chrome from $FROM (not hda.json)"
  git checkout "$FROM" -- "${CHROME_PATHS[@]}"
  echo "==> left site/hda.json and site/hda.html alone"
  echo "    re-apply summit notes on site/hda.html from docs/hda.md; keep nav/og/favicon from $FROM"
  echo "    then: ./tools/sync-hda-site.sh && ./tools/sync-hda-site.sh --check"
  echo "RAYNU-V-SITE-CHROME-OK"
  exit 0
fi

# --check: chrome files on disk must match FROM. hda.html / hda.json may differ.
fail=0
while IFS= read -r -d '' path; do
  rel="${path#./}"
  if git cat-file -e "$FROM:$rel" 2>/dev/null; then
    if ! git diff --quiet "$FROM" -- "$rel"; then
      echo "mismatch: $rel differs from $FROM" >&2
      fail=1
    fi
  fi
done < <(git ls-tree -r --name-only -z "$FROM" -- "${CHROME_PATHS[@]}")

if [[ "$fail" -ne 0 ]]; then
  echo "error: site chrome drifted from $FROM" >&2
  echo "hint: ./tools/preserve-site-chrome.sh --restore --from $FROM" >&2
  exit 1
fi
echo "site chrome matches $FROM (hda.html / hda.json not compared)"
echo "RAYNU-V-SITE-CHROME-OK"
