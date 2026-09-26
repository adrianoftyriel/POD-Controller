#!/usr/bin/env python3
"""Build the UI's model/parameter catalog (crates/pod-cli/ui/catalog.json).

Combines:
  docs/MODELS.md                model names per (category, table, model)
  docs/knobs.json               knob names per model (Gearbox Tweak menus)
  tools/l6t/preset_models.json  parameter IDs + value ranges per model,
                                from the 558 factory presets
  <dumps dir> (optional)        EffectDumps (e.g. a GET ALL of the unit),
                                for parameter IDs of models in use

Usage: build_catalog.py [--dumps DIR] [--out FILE]
"""
import argparse
import glob
import json
import os
import re
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Record order in a tone block is fixed (docs/PROTOCOL.md "Block record").
BLOCKS = ["amp", "cab", "stomp", "mod", "delay", "reverb", "gate", "comp",
          "eq", "wah", "volume", "loop"]
LABELS = {"amp": "Amp", "cab": "Cab", "stomp": "Stomp", "mod": "Mod",
          "delay": "Delay", "reverb": "Reverb", "gate": "Gate",
          "comp": "Comp", "eq": "EQ", "wah": "Wah", "volume": "Volume",
          "loop": "FX Loop"}
# Which (category, table[, model]) a block's model menu draws from.
SOURCES = {
    "amp": [(0, 2), (0, 3), (0, 4)],
    "cab": [(1, 2), (1, 3)],
    "stomp": [(2, 0), (2, 5), (2, 10)],
    "mod": [(2, 3)], "delay": [(2, 2)], "reverb": [(2, 4)],
    "gate": [(2, 11, 0)], "comp": [(2, 11, 1)], "eq": [(2, 12)],
    "wah": [(2, 6)], "volume": [(2, 7)], "loop": [(2, 11, 10)],
}
# Blocks that can sit before or after the amp: (pre slot/group, post).
MOVABLE = {"volume": [[1, 2], [2, 5]], "mod": [[4, 2], [3, 5]],
           "delay": [[5, 2], [4, 5]], "reverb": [[6, 2], [5, 5]],
           "loop": [[12, 2], [9, 5]]}
# Knob names documented in PROTOCOL.md for fixed blocks, by param ID.
FIXED_NAMES = {
    "amp": {"3f100000": "Bass", "3f100001": "Middle", "3f100002": "Treble",
            "3f100003": "Drive", "3f100004": "Presence", "3f100005": "Volume"},
    "gate": {"3f000000": "Threshold", "3f000003": "Decay"},
    "comp": {"3f100000": "Threshold", "3f100001": "Gain"},
    "wah": {"3f000001": "Position"},
    "volume": {"3f000004": "Min", "3f000005": "Max"},
    "loop": {"3f000001": "Send", "3f000002": "Return", "3f010001": "Mix"},
    "eq": {f"3f10000{2*n:x}": f"Band {n+1} Freq" for n in range(4)}
          | {f"3f10000{2*n+1:x}": f"Band {n+1} Gain" for n in range(4)},
}
UNITS = {("gate", "3f000000"): ("dB", -96.0, 0.0)}

SECTION = re.compile(r"^## (?:amp \(category 00\)|cab \(category 01\)|type ([0-9A-F]{2}) )", re.I)
ROW = re.compile(r"^\| (\d+) \(`[0-9A-F]+`\) \| ([^|]+?) \|")


def model_names():
    names = {}
    key = None
    for line in open(os.path.join(ROOT, "docs/MODELS.md")):
        m = SECTION.match(line)
        if line.startswith("## "):
            key = None
        if m:
            if "amp (category" in line:
                key = (0, 2)
            elif "cab (category" in line:
                key = (1, 2)
            else:
                key = (2, int(m.group(1), 16))
            continue
        r = ROW.match(line)
        if key and r:
            names[key + (int(r.group(1)),)] = r.group(2).strip()
    return names


def parse_dump_params(dumps_dir):
    """(cat, table, model) -> {param id: [values]} from EffectDumps."""
    seen = {}
    for path in sorted(glob.glob(os.path.join(dumps_dir, "*.bin"))):
        data = open(path, "rb").read()
        if len(data) != 4096:
            continue
        for tone in (0, 1):
            t = data[tone * 2048:(tone + 1) * 2048]
            for n in range(12):
                b = t[0xE4 + n * 0x8C:0xE4 + (n + 1) * 0x8C]
                model, = struct.unpack_from("<H", b, 0)
                key = (b[3], b[2], model)
                for p in range(b[11]):
                    pid, = struct.unpack_from("<I", b, 12 + p * 8)
                    val, = struct.unpack_from("<f", b, 16 + p * 8)
                    seen.setdefault(key, {}).setdefault(f"{pid:08x}", []).append(val)
    return seen


# Index of the documented Mix knob (namespace 3F01) per block.
MIX_INDEX = {"mod": 1, "delay": 1, "loop": 1, "reverb": 2}


def default_name(block, pid):
    ns, idx = int(pid[:4], 16), int(pid[4:], 16)
    if ns == 0x3F01:
        return "Mix" if MIX_INDEX.get(block) == idx else f"Aux {idx}"
    if ns == 0x3F00:
        return f"Level {idx}"
    return f"Param {idx}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dumps")
    ap.add_argument("--out", default=os.path.join(ROOT, "crates/pod-cli/ui/catalog.json"))
    args = ap.parse_args()

    names = model_names()
    knobs = json.load(open(os.path.join(ROOT, "docs/knobs.json")))
    presets = json.load(open(os.path.join(ROOT, "tools/l6t/preset_models.json")))
    dumps = parse_dump_params(args.dumps) if args.dumps else {}

    def pkey(c, t, m):
        return f"{c:02x}/{t:02x}/{m}"

    blocks = []
    for index, block in enumerate(BLOCKS):
        models = []
        for src in SOURCES[block]:
            cat, table = src[0], src[1]
            only = src[2] if len(src) > 2 else None
            ids = {k[2] for k in names if k[:2] == (cat, table)}
            ids |= {int(k.split("/")[2]) for k in presets
                    if k.startswith(f"{cat:02x}/{table:02x}/")}
            ids |= {k[2] for k in dumps if k[:2] == (cat, table)}
            if only is not None:
                ids = {only}
            for model in sorted(ids):
                key = pkey(cat, table, model)
                preset = presets.get(key, {})
                name = (names.get((cat, table, model)) if only is None else None) \
                    or preset.get("name") or LABELS[block]
                if only is None and (cat, table) not in {(0, 2), (1, 2)} and (cat, table, model) not in names:
                    name = preset.get("name") or f"{LABELS[block]} {table:02X}/{model}"
                if block == "amp" and table != 2:
                    name = preset.get("name") or f"Amp pack {table:02X} #{model}"
                pids = dict(preset.get("params", {}))
                for pid, vals in dumps.get((cat, table, model), {}).items():
                    pids.setdefault(pid, [min(vals), max(vals)])
                knob_names = knobs.get(key, {}).get("params", {})
                params = []
                for pid in sorted(pids):
                    lo, hi = pids[pid]
                    if not isinstance(lo, float) and not isinstance(hi, float):
                        lo, hi = float(lo), float(hi)
                    unit, rmin, rmax = UNITS.get((block, pid), ("", None, None))
                    if rmin is None:
                        rmin, rmax = min(0.0, lo), max(1.0, hi)
                    params.append({
                        "id": pid,
                        "name": knob_names.get(pid) or FIXED_NAMES.get(block, {}).get(pid)
                        or default_name(block, pid),
                        "min": rmin, "max": rmax, "unit": unit,
                        "default": round((lo + hi) / 2, 4),
                        # 3F20xxxx IDs are stored in the patch but never
                        # offered as knobs by Gearbox; meaning unknown.
                        "hidden": pid.startswith("3f20"),
                    })
                models.append({"category": cat, "table": table, "model": model,
                               "name": name, "params": params,
                               "params_known": bool(params)})
        blocks.append({"index": index, "key": block, "label": LABELS[block],
                       "positions": MOVABLE.get(block), "models": models})

    variax = [{"value": 0, "name": "Don't Control"}]
    for g, family in enumerate(["Custom 1", "T-Model", "Spank", "Lester", "Special",
                                "R-Billy", "Chime", "Semi", "Jazzbox", "Acoustic",
                                "Reso", "Custom 2"]):
        for i in range(5):
            variax.append({"value": 1 + g * 5 + i, "name": f"{family} {i + 1}"})
    settings = [
        {"key": "variax_model", "label": "Variax Model", "section": "variax",
         "type": "enum", "options": variax},
        {"key": "variax_tone", "label": "Variax Tone", "section": "variax",
         "type": "int", "min": 0, "max": 127},
        {"key": "mic", "label": "Mic", "section": "cab", "type": "enum",
         "options": [{"value": i, "name": n} for i, n in enumerate(
             ["57 On Axis", "57 Off Axis", "421 Dynamic", "67 Condenser"])]},
        {"key": "room", "label": "Room", "section": "cab", "type": "float",
         "min": 0.0, "max": 1.0},
        {"key": "input", "label": "Input", "section": "setup", "type": "enum",
         "options": [{"value": i, "name": n} for i, n in enumerate(
             ["Same", "Guitar", "Mic", "Aux", "Variax", "Guitar+Aux",
              "Guitar+Variax", "Gtr+Aux+Var"])]},
    ]
    out = {"_generated_by": "tools/catalog/build_catalog.py",
           "blocks": blocks, "settings": settings}
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(out, f, indent=1)
    n = sum(len(b["models"]) for b in blocks)
    print(f"wrote {args.out}: {n} models")


if __name__ == "__main__":
    main()
