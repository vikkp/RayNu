#!/usr/bin/env bash
# Sync docs/loihda.md YAML frontmatter -> site/loi.json (public LOI tracker).
# Usage:
#   ./tools/sync-loihda-site.sh           # write site/loi.json
#   ./tools/sync-loihda-site.sh --check   # exit 1 if site/loi.json is stale
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${ROOT}/docs/loihda.md"
OUT="${ROOT}/site/loi.json"
CHECK=0

if [[ "${1:-}" == "--check" ]]; then
  CHECK=1
fi

if [[ ! -f "$SRC" ]]; then
  echo "error: missing $SRC" >&2
  exit 1
fi

export LOI_SRC="$SRC"
export LOI_OUT="$OUT"
export LOI_CHECK="$CHECK"

python3 <<'ENDPYTHON'
import json, os, re, sys
from pathlib import Path

src_path = Path(os.environ["LOI_SRC"])
out_path = Path(os.environ["LOI_OUT"])
check = os.environ["LOI_CHECK"] == "1"
text = src_path.read_text(encoding="utf-8")

if not text.startswith("---"):
    print(f"error: {src_path} missing YAML frontmatter", file=sys.stderr)
    sys.exit(1)

parts = text.split("---", 2)
if len(parts) < 3:
    print(f"error: {src_path} frontmatter not closed", file=sys.stderr)
    sys.exit(1)
fm = parts[1]

def parse_scalar(raw: str):
    raw = raw.strip()
    if not raw:
        return ""
    if (raw.startswith('"') and raw.endswith('"')) or (
        raw.startswith("'") and raw.endswith("'")
    ):
        return raw[1:-1]
    if re.fullmatch(r"-?\d+", raw):
        return int(raw)
    if re.fullmatch(r"-?\d+\.\d+", raw):
        return float(raw)
    if raw in ("true", "false"):
        return raw == "true"
    return raw

data = {}
for line in fm.splitlines():
    line = line.strip()
    if not line or line.startswith("#") or ":" not in line:
        continue
    key, _, val = line.partition(":")
    data[key.strip()] = parse_scalar(val)

required = [
    "loihda_version",
    "last_updated",
    "months_to_loi_a",
    "months_to_loi_a_prev",
    "months_to_loi_b",
    "overall_pct",
    "loi_a_eta_month",
    "loi_b_eta_month",
    "loi_target",
    "confidence",
    "bar_a_pct",
    "bar_b_pct",
]
missing = [k for k in required if k not in data]
if missing:
    print(f"error: frontmatter missing keys: {', '.join(missing)}", file=sys.stderr)
    sys.exit(1)

payload = {
    "loihda_version": data.get("loihda_version", 1),
    "last_updated": data["last_updated"],
    "last_commit": data.get("last_commit", "PENDING"),
    "last_commit_short": data.get("last_commit_short", "PENDING"),
    "updated_by": data.get("updated_by", "cursor"),
    "loi_target": data["loi_target"],
    "loi_path": [
        "Everest loop (closed)",
        "persist across HV reboot",
        "SKU honesty",
        "TLS for InfoSec",
        "standing SPA during Alpine",
        "Bar A dedicated-box LOI",
        "PERC RAID I/O",
        "Bar B fleet LOI",
    ],
    "months_to_loi_a": data["months_to_loi_a"],
    "months_to_loi_a_prev": data["months_to_loi_a_prev"],
    "months_to_loi_b": data["months_to_loi_b"],
    "months_to_loi_b_prev": data.get("months_to_loi_b_prev", data["months_to_loi_b"]),
    "overall_pct": data["overall_pct"],
    "confidence": data["confidence"],
    "baseline_date": data.get("baseline_date"),
    "baseline_months": data.get("baseline_months"),
    "loi_a_eta_month": data["loi_a_eta_month"],
    "loi_b_eta_month": data["loi_b_eta_month"],
    "bars": {
        "a": data["bar_a_pct"],
        "b": data["bar_b_pct"],
    },
    "pieces": {
        "everest": data.get("piece_everest_pct", 100),
        "persist": data.get("piece_persist_pct", 0),
        "sku": data.get("piece_sku_pct", 0),
        "tls": data.get("piece_tls_pct", 0),
        "auth": data.get("piece_auth_pct", 0),
        "console": data.get("piece_console_pct", 0),
        "perc": data.get("piece_perc_pct", 0),
        "unmodified": data.get("piece_unmodified_pct", 0),
    },
    "source": "docs/loihda.md",
    "docs_url": "https://github.com/vikkp/RayNu/blob/main/docs/loihda.md",
}

encoded = json.dumps(payload, indent=2, sort_keys=True) + "\n"

if check:
    if not out_path.is_file():
        print(f"error: missing {out_path}; run ./tools/sync-loihda-site.sh", file=sys.stderr)
        sys.exit(1)
    if out_path.read_text(encoding="utf-8") != encoded:
        print("error: site/loi.json is stale vs docs/loihda.md frontmatter", file=sys.stderr)
        print("run: ./tools/sync-loihda-site.sh", file=sys.stderr)
        sys.exit(1)
    print("LOIHDA site sync OK - site/loi.json matches docs/loihda.md")
    sys.exit(0)

out_path.write_text(encoded, encoding="utf-8")
print(f"wrote {out_path}")
print(
    f"LOIHDA: Bar A {payload['bars']['a']}% / Bar B {payload['bars']['b']}% / "
    f"overall {payload['overall_pct']}% / months A {payload['months_to_loi_a']}"
)
ENDPYTHON
