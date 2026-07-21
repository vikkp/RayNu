#!/usr/bin/env bash
# Sync docs/hda.md YAML frontmatter → site/hda.json (public Mount Everest tracker).
# Usage:
#   ./tools/sync-hda-site.sh           # write site/hda.json
#   ./tools/sync-hda-site.sh --check   # exit 1 if site/hda.json is stale
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${ROOT}/docs/hda.md"
OUT="${ROOT}/site/hda.json"
CHECK=0

if [[ "${1:-}" == "--check" ]]; then
  CHECK=1
fi

if [[ ! -f "$SRC" ]]; then
  echo "error: missing $SRC" >&2
  exit 1
fi

python3 - "$SRC" "$OUT" "$CHECK" <<'PY'
import json, re, sys
from pathlib import Path

src_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
check = sys.argv[3] == "1"
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
    "hda_version",
    "last_updated",
    "months_to_everest",
    "months_to_everest_prev",
    "overall_pct",
    "everest_eta_month",
    "mount_everest_target",
    "confidence",
]
missing = [k for k in required if k not in data]
if missing:
    print(f"error: frontmatter missing keys: {', '.join(missing)}", file=sys.stderr)
    sys.exit(1)

payload = {
    "hda_version": data.get("hda_version", 1),
    "last_updated": data["last_updated"],
    "last_commit": data.get("last_commit", "PENDING"),
    "last_commit_short": data.get("last_commit_short", "PENDING"),
    "updated_by": data.get("updated_by", "cursor"),
    "mount_everest_target": data["mount_everest_target"],
    "mount_everest_path": [
        "Ship EFI",
        "boot real R640",
        "network vSphere-like UI",
        "deploy Linux ISO",
        "prod bar (M6.9)",
    ],
    "months_to_everest": data["months_to_everest"],
    "months_to_everest_prev": data["months_to_everest_prev"],
    "overall_pct": data["overall_pct"],
    "confidence": data["confidence"],
    "baseline_date": data.get("baseline_date"),
    "baseline_months": data.get("baseline_months"),
    "everest_eta_month": data["everest_eta_month"],
    "velocity_commits_30d": data.get("velocity_commits_30d"),
    "velocity_gates_30d": data.get("velocity_gates_30d"),
    "summits": {
        "core": data.get("summit_core_pct", 78),
        "efi": data.get("summit_efi_pct", 85),
        "r640": data.get("summit_r640_pct", 25),
        "ui": data.get("summit_ui_pct", 12),
        "iso": data.get("summit_iso_pct", 8),
        "prod": data.get("summit_prod_pct", 100),
    },
    "source": "docs/hda.md",
    "docs_url": "https://github.com/vikkp/RayNu/blob/main/docs/hda.md",
}

encoded = json.dumps(payload, indent=2, sort_keys=True) + "\n"

if check:
    if not out_path.is_file():
        print(f"error: missing {out_path}; run ./tools/sync-hda-site.sh", file=sys.stderr)
        sys.exit(1)
    if out_path.read_text(encoding="utf-8") != encoded:
        print("error: site/hda.json is stale vs docs/hda.md frontmatter", file=sys.stderr)
        print("run: ./tools/sync-hda-site.sh", file=sys.stderr)
        sys.exit(1)
    print("HDA site sync OK — site/hda.json matches docs/hda.md")
    sys.exit(0)

out_path.write_text(encoded, encoding="utf-8")
print(f"wrote {out_path}")
print(
    f"HDA: months_to_everest {payload['months_to_everest']} · "
    f"overall {payload['overall_pct']}% · ETA {payload['everest_eta_month']}"
)
PY
