#!/usr/bin/env bash
# Guard the public-site look from the Kimi site-updater PR (#218–#226).
# Feature branches that truncated `site/` still pass cargo tests; Cloudflare
# Git integration then deploys that truncated tree over raynuv.com.
# Fail CI / Worker / Pages deploy if the updater chrome is missing.
#
# Lived copy (Everest closed, M8 residual, COM2 snippets) MAY change.
# Chrome landmarks MUST NOT. Do not require "Not Everest".
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

# Homepage chrome — never delete these while "updating status"
need_text "index.html" 'nav class="site-nav' "fixed site nav"
need_text "index.html" "CIO View" "CIO View nav"
need_text "index.html" "fork__card" "CIO/engineer fork cards"
need_text "index.html" 'id="status"' "status section"
need_text "index.html" 'id="start-here"' "audience fork section"
need_text "index.html" 'id="console"' "console section"
need_text "index.html" 'id="buddies"' "buddies / COM2 section"
need_text "index.html" "family=Syne" "Syne brand face"
need_text "index.html" "REQUIRED SITE CHROME" "chrome do-not-delete comment"

need_text "ciospeak.html" 'nav class="site-nav' "CIO page nav"
need_text "ciospeak.html" "cio-status-strip" "CIO status strip"
need_text "ciospeak.html" "Your PowerEdge fleet has years left" "CIO mast title"

need_text "stories.html" 'nav class="site-nav' "Stories page nav"
need_text "stories.html" "stories-mast" "Stories mast"
need_text "stories.html" "Field journal" "Stories kicker"

need_text "com2.html" "Mr. COM2" "COM2 buddy page"
need_text "com2.html" 'nav class="site-nav' "COM2 page nav"

need_text "hda.html" "Honest Distance Assessment" "HDA mast"
need_text "hda.html" "hda.js" "HDA JSON loader"
need_text "hda.html" 'nav class="site-nav' "HDA page nav"

need_text "paper.html" 'nav class="site-nav' "Paper page nav"

need_text "styles.css" "--font-brand" "brand token"
need_text "styles.css" "--teal:" "teal token"
need_text "styles.css" ".site-nav {" "nav CSS"
need_text "styles.css" ".fork__card" "fork card CSS"
need_text "styles.css" ".statband" "statband CSS"

need_text "main.js" 'querySelector(".site-nav")' "nav solid-on-scroll"

if [[ ! -f "${ROOT}/wrangler.jsonc" ]]; then
  echo "error: missing wrangler.jsonc" >&2
  fail=1
elif ! grep -q '"directory": "site"' "${ROOT}/wrangler.jsonc"; then
  echo "error: wrangler.jsonc must publish assets.directory = site" >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  echo "site chrome check FAILED — restore chrome from origin/main; patch lived strings in place." >&2
  echo "Do not replace site/index.html wholesale. Do not deploy a truncated site/ tree." >&2
  exit 1
fi

echo "site chrome OK — CIO View + Status + Stories + HDA updater look present"
exit 0
