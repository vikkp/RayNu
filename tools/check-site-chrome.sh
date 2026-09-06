#!/usr/bin/env bash
# Guard the public-site look from the Kimi site-updater PR (#218–#226).
# Feature branches that truncated `site/` still pass cargo tests; Cloudflare
# Git integration then deploys that truncated tree over raynuv.com.
# Fail CI / Worker deploy if the updater chrome is missing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="${ROOT}/site"
fail=0

need_file() {
  local f="$1"
  if [[ ! -f "$SITE/$f" ]]; then
    echo "error: missing site/$f (updater chrome)" >&2
    fail=1
  fi
}

need_text() {
  local f="$1"
  local pat="$2"
  local label="$3"
  if [[ ! -f "$SITE/$f" ]]; then
    return
  fi
  if ! grep -q -F -- "$pat" "$SITE/$f"; then
    echo "error: site/$f missing ${label}: ${pat}" >&2
    fail=1
  fi
}

need_file "index.html"
need_file "stories.html"
need_file "ciospeak.html"
need_file "com2.html"
need_file "hda.html"
need_file "hda.js"
need_file "hda.json"
need_file "styles.css"
need_file "main.js"
need_file "404.html"
need_file "robots.txt"
need_file "sitemap.xml"
need_file "assets/favicon.svg"
need_file "assets/hero-800.webp"
need_file "assets/spa-800.webp"
need_file "assets/r640-boot-ok-com2-800.webp"

need_text "index.html" "CIO View" "CIO View nav"
need_text "index.html" "fork__card" "CIO/engineer fork cards"
need_text "index.html" "family=Syne" "Syne brand face"
need_text "ciospeak.html" "cio-status-strip" "CIO status strip"
need_text "ciospeak.html" "Your PowerEdge fleet has years left" "CIO mast title"
need_text "stories.html" "stories-mast" "Stories mast"
need_text "stories.html" "Field journal" "Stories kicker"
need_text "com2.html" "Mr. COM2" "COM2 buddy page"
need_text "styles.css" "--font-brand" "brand token"
need_text "styles.css" "--teal:" "teal token"
need_text "hda.html" "Honest Distance Assessment" "HDA mast"
need_text "hda.html" "hda.js" "HDA JSON loader"

if [[ ! -f "${ROOT}/wrangler.jsonc" ]]; then
  echo "error: missing wrangler.jsonc" >&2
  fail=1
elif ! grep -q '"directory": "site"' "${ROOT}/wrangler.jsonc"; then
  echo "error: wrangler.jsonc must publish assets.directory = site" >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  echo "site chrome check FAILED — restore site/ from origin/main (Kimi updater look)." >&2
  echo "Do not deploy a truncated site/ tree to the raynuv Worker." >&2
  exit 1
fi

echo "site chrome OK — CIO View + Stories + HDA updater look present"
exit 0
