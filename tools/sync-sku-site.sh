#!/usr/bin/env bash
# Sync the ADR-021 catalog (docs/adr/ADR-021.md, "## Catalog") -> site/sku.json.
# The public SKU page (site/sku.html + site/sku.js) renders that JSON.
# Usage:
#   ./tools/sync-sku-site.sh           # write site/sku.json
#   ./tools/sync-sku-site.sh --check   # exit 1 if site/sku.json is stale
#   ./tools/sync-sku-site.sh --next    # print the current wave and what to pick up
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${ROOT}/docs/adr/ADR-021.md"
OUT="${ROOT}/site/sku.json"
CHECK=0
NEXT=0

if [[ "${1:-}" == "--check" ]]; then
  CHECK=1
elif [[ "${1:-}" == "--next" ]]; then
  NEXT=1
fi

if [[ ! -f "$SRC" ]]; then
  echo "error: missing $SRC" >&2
  exit 1
fi

export SKU_SRC="$SRC"
export SKU_OUT="$OUT"
export SKU_CHECK="$CHECK"
export SKU_NEXT="$NEXT"

python3 <<'ENDPYTHON'
import json, os, re, sys
from pathlib import Path

src_path = Path(os.environ["SKU_SRC"])
out_path = Path(os.environ["SKU_OUT"])
check = os.environ["SKU_CHECK"] == "1"
want_next = os.environ["SKU_NEXT"] == "1"
text = src_path.read_text(encoding="utf-8")

STATUSES = ("lived", "in flight", "planned")
SHIPS = ("S1", "Final")
COLUMNS = ["ID", "Capability", "Status", "Ships", "Ref", "Note"]

m = re.search(r"^## Catalog\s*$(.*?)^## Consequences\s*$", text, re.S | re.M)
if not m:
    print(f"error: {src_path} has no '## Catalog' .. '## Consequences' section", file=sys.stderr)
    sys.exit(1)
catalog = m.group(1)

date_m = re.search(r"^- \*\*Date:\*\*\s*(\d{4}-\d{2}-\d{2})", text, re.M)
adr_date = date_m.group(1) if date_m else ""

def strip_md(s: str) -> str:
    s = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", s)
    s = s.replace("**", "")
    return s.strip()

def split_row(line: str):
    line = line.strip()
    if not (line.startswith("|") and line.endswith("|")):
        return None
    return [c.strip() for c in line[1:-1].split("|")]

areas = []
cur = None
errors = []
seen_ids = set()
for raw in catalog.splitlines():
    h = re.match(r"^### ([A-Z])\. (.+?)\s*$", raw)
    if h:
        title = h.group(2)
        pillars = re.findall(r"\[([VZDA])\]", title)
        name = re.sub(r"\s*(\[[VZDA]\]\s*)+$", "", title).strip()
        cur = {"id": h.group(1), "name": name, "pillars": pillars, "items": []}
        areas.append(cur)
        continue
    cells = split_row(raw)
    if cells is None or cur is None:
        continue
    if cells == COLUMNS:
        continue
    if all(re.fullmatch(r":?-+:?", c) for c in cells):
        continue
    if len(cells) != len(COLUMNS):
        errors.append(f"{cur['id']}: row has {len(cells)} cells: {raw}")
        continue
    rid, cap, status, ships, ref, note = cells
    status = strip_md(status).lower()
    ships = strip_md(ships)
    if not re.fullmatch(rf"{cur['id']}\d+", rid):
        errors.append(f"{rid}: id does not belong to area {cur['id']}")
    if rid in seen_ids:
        errors.append(f"{rid}: duplicate id")
    seen_ids.add(rid)
    if status not in STATUSES:
        errors.append(f"{rid}: status '{status}' not in {STATUSES}")
    if ships not in SHIPS:
        errors.append(f"{rid}: ships '{ships}' not in {SHIPS}")
    if status == "lived" and not re.search(r"`[0-9a-f]{7,12}`|RAYNU-V-[A-Z0-9-]+-OK|\./tools/|docs/|ADR-", note):
        errors.append(f"{rid}: lived rows need evidence (SHA, marker, tool, or doc) in Note")
    cur["items"].append({
        "id": rid,
        "capability": strip_md(cap),
        "status": status,
        "ships": ships,
        "ref": strip_md(ref),
        "note": strip_md(note),
    })

# ---- Build order (waves) ----
WAVE_COLUMNS = ["Wave", "Theme", "Rows", "Iron thread", "Why here"]
bm = re.search(r"^## Build order.*?$(.*?)^## ", text, re.S | re.M)
if not bm:
    errors.append("no '## Build order' section")
    waves = []
else:
    waves = []
    for raw in bm.group(1).splitlines():
        cells = split_row(raw)
        if cells is None or cells == WAVE_COLUMNS:
            continue
        if all(re.fullmatch(r":?-+:?", c) for c in cells):
            continue
        if len(cells) != len(WAVE_COLUMNS):
            errors.append(f"build order row has {len(cells)} cells: {raw}")
            continue
        wid, theme, rows, iron, why = (strip_md(c) for c in cells)
        if not re.fullmatch(r"\d+|V", wid):
            errors.append(f"wave '{wid}': id must be a number or V")
        ids = [r.strip() for r in rows.split(",") if r.strip()]
        iron = "" if iron in ("—", "-", "") else iron
        if iron and iron not in ids:
            errors.append(f"wave {wid}: iron thread {iron} is not one of its rows")
        waves.append({"id": wid, "theme": theme, "rows": ids, "iron": iron, "why": why})

wave_of = {}
for w in waves:
    for rid in w["rows"]:
        if rid in wave_of:
            errors.append(f"{rid}: in wave {wave_of[rid]} and wave {w['id']}")
        wave_of[rid] = w["id"]
by_id = {it["id"]: it for a in areas for it in a["items"]}
for rid in by_id:
    if rid not in wave_of:
        errors.append(f"{rid}: not in any wave")
for rid in wave_of:
    if rid not in by_id:
        errors.append(f"wave row {rid}: not in the catalog")
for rid, it in by_id.items():
    w = wave_of.get(rid)
    if w is None:
        continue
    it["wave"] = w
    numeric = w != "V"
    if it["ships"] == "S1" and (not numeric or int(w) > 9):
        errors.append(f"{rid}: S1 row must be in waves 0-9, is in {w}")
    if it["ships"] == "Final" and numeric and int(w) <= 9:
        errors.append(f"{rid}: Final row must not be in waves 0-9, is in {w}")
    if w == "0" and it["status"] != "lived":
        errors.append(f"{rid}: wave 0 is the lived base; this row is {it['status']}")

if not areas:
    errors.append("no '### X. Area' headings under Catalog")
for a in areas:
    if not a["items"]:
        errors.append(f"area {a['id']} has no rows")
if errors:
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    sys.exit(1)

def count(items, ships=None):
    c = {s: 0 for s in STATUSES}
    for it in items:
        if ships is None or it["ships"] == ships:
            c[it["status"]] += 1
    c["total"] = sum(c[s] for s in STATUSES)
    return c

all_items = [it for a in areas for it in a["items"]]

def wave_key(w):
    return (1, 0) if w["id"] == "V" else (0, int(w["id"]))

waves.sort(key=wave_key)
for w in waves:
    items = [by_id[r] for r in w["rows"]]
    w["counts"] = count(items)
    w["closed"] = all(it["status"] == "lived" for it in items)
current = next((w for w in waves if w["id"] != "V" and not w["closed"]), None)
next_up = []
if current is not None:
    pending = [by_id[r] for r in current["rows"] if by_id[r]["status"] != "lived"]
    iron = [it for it in pending if it["id"] == current["iron"]]
    host = [it for it in pending if it["id"] != current["iron"]]
    next_up = [
        {"id": it["id"], "capability": it["capability"], "status": it["status"],
         "role": "iron thread" if it["id"] == current["iron"] else "host-first"}
        for it in iron + host
    ]
# Filler: host-first rows in the next unclosed wave after the current one.
filler = []
if current is not None:
    later = [w for w in waves if w["id"] != "V" and not w["closed"] and wave_key(w) > wave_key(current)]
    if later:
        nw = later[0]
        filler = [
            {"id": r, "capability": by_id[r]["capability"], "wave": nw["id"]}
            for r in nw["rows"] if by_id[r]["status"] != "lived" and r != nw["iron"]
        ]

data = {
    "generated_from": "docs/adr/ADR-021.md",
    "adr": "ADR-021",
    "adr_date": adr_date,
    "statuses": list(STATUSES),
    "shipments": {
        "S1": {"name": "Shipment 1", "lede": "The dedicated-box letter: one R640, one Ubuntu guest, no RayNu-V engineer in the room."},
        "Final": {"name": "Final SKU", "lede": "Everything a professional operator expects from a PowerEdge hypervisor, through cluster and VMware migration. If it is not a row here, it is not the product."},
    },
    "counts": {
        "S1": count(all_items, "S1"),
        "Final": count(all_items),
    },
    "areas": [
        {**a, "counts": {"S1": count(a["items"], "S1"), "Final": count(a["items"])}}
        for a in areas
    ],
    "waves": waves,
    "current_wave": None if current is None else current["id"],
    "next_up": next_up,
    "filler": filler,
}

rendered = json.dumps(data, indent=2, ensure_ascii=False) + "\n"

if want_next:
    if current is None:
        print("All numbered waves are closed. Shipment 1 and the final SKU are lived. Wave V (verification) continues.")
        sys.exit(0)
    c = current["counts"]
    print(f"Current wave {current['id']}: {current['theme']}  ({c['lived']} lived / {c['in flight']} in flight / {c['planned']} planned)")
    print(f"  why: {current['why']}")
    print("Pick up, in order:")
    for n in next_up:
        print(f"  {n['id']:<4} [{n['role']:<11}] {n['status']:<9} {n['capability']}")
    if filler:
        print(f"While the rack is busy (host-first, wave {filler[0]['wave']}):")
        for f in filler:
            print(f"  {f['id']:<4} {f['capability']}")
    closed = [w["id"] for w in waves if w["closed"] and w["id"] != "V"]
    print(f"Closed waves: {', '.join(closed) if closed else 'none'}. One iron thread at a time. Edit docs/adr/ADR-021.md, then ./tools/sync-sku-site.sh.")
    sys.exit(0)

if check:
    if not out_path.exists():
        print(f"error: {out_path} missing; run ./tools/sync-sku-site.sh", file=sys.stderr)
        sys.exit(1)
    if out_path.read_text(encoding="utf-8") != rendered:
        print(f"error: {out_path} is stale vs {src_path}; run ./tools/sync-sku-site.sh", file=sys.stderr)
        sys.exit(1)
    c = data["counts"]
    print(f"SKU site sync OK - site/sku.json matches ADR-021 ({c['Final']['total']} rows, S1 {c['S1']['total']})")
    sys.exit(0)

out_path.write_text(rendered, encoding="utf-8")
c = data["counts"]
print(f"wrote {out_path}")
print(f"SKU: {c['Final']['total']} rows · S1 {c['S1']['total']} "
      f"(lived {c['S1']['lived']}, in flight {c['S1']['in flight']}, planned {c['S1']['planned']}) · "
      f"Final lived {c['Final']['lived']}, in flight {c['Final']['in flight']}, planned {c['Final']['planned']}")
ENDPYTHON
