#!/usr/bin/env python3
"""Map knob names to parameter IDs using Gearbox's Tweak menu.

The Tweak 1/2 menus list every knob of every block in the current tone as
text ("Dly:Flutter"), and picking an entry sends the knob's block index and
parameter key (05/16 params 0x1E and 0x1F). Loading a patch makes Gearbox
push both tone blocks, which says which model sits in each block. Together
that gives (table, model, param ID) -> knob name for every model in the
patch, without writing anything to the POD (only the edit buffer changes).

Both tones are swept (Tweak 1 = tone 1, Tweak 2 = tone 2).

Usage: tweak_sweep.py <vmid> <bank 1-16> <A-D> [--out results.jsonl]

Afterwards every swept patch shows as edited in Gearbox (its Tweak
assignment changed). Run GET ALL to restore Gearbox's copies from the POD
before doing anything with PUT.

Assumes the 1280x1024 layout (see README) with both Gearbox windows open.
Needs tesseract-ocr and ImageMagick on the host.
"""
import json
import os
import struct
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import vmctl  # noqa: E402

exec(open(os.path.join(HERE, "msgs.py")).read().split("def main")[0])  # reassemble()
exec(open(os.path.join(HERE, "parse_tone.py")).read().split("def main")[0])  # parse_tone()

TWEAK1 = (60, 344)            # Tweak 1 button
FIRST_ROW_Y = 362             # first menu entry
ROW_H = 13
PREFIXES = {"stp": "stomp", "mod": "mod", "dly": "delay", "rvb": "reverb",
            "cmp": "comp", "wah": "wah", "fx": "loop", "vol": "volume",
            "gat": "gate", "amp": "amp", "eq": "eq"}


def sh(*args, **kw):
    return subprocess.run(args, check=True, cwd=HERE, **kw)


def capture_start(path):
    sh("./usb-capture.sh", "start", "0e41:414b", path, stdout=subprocess.DEVNULL)
    time.sleep(1)


def capture_stop(path):
    time.sleep(1.5)
    sh("./usb-capture.sh", "stop", path + ".pid")
    bulk = subprocess.run(["./decode.sh", path], cwd=HERE, check=True,
                          capture_output=True, text=True).stdout
    return [l.split() for l in bulk.splitlines()]


def ocr_menu(q, png, top):
    vmctl.screenshot(q, png)
    crop = png.replace(".png", "-menu.png")
    sh("convert", png, "-crop", f"150x640+15+{top}", "+repage", "-filter", "Lanczos",
       "-resize", "300%", "-colorspace", "gray", "-level", "20%,80%", crop)
    text = subprocess.run(["tesseract", crop, "-", "--psm", "6"], capture_output=True,
                          text=True).stdout
    return [l.strip().rstrip("|").strip() for l in text.splitlines() if ":" in l]


def normalize(label):
    pre, _, name = label.partition(":")
    pre = pre.strip().lower().replace("i", "l").replace("y", "l")
    # tesseract confuses l/i/y in "Dly"/"Rvb"; map by closest known prefix
    best = min(PREFIXES, key=lambda p: sum(a != b for a, b in zip(p.ljust(3), pre.ljust(3)[:3])))
    return PREFIXES[best], name.strip()


def main():
    vmid, bank, ch = sys.argv[1], int(sys.argv[2]), sys.argv[3].upper()
    out = sys.argv[sys.argv.index("--out") + 1] if "--out" in sys.argv else \
        os.path.join(HERE, "captures", "tweak_map.jsonl")
    ci = ord(ch) - 65
    x = 57 if bank <= 8 else 476
    y = 95 + ((bank - 1) % 8) * 82 + ci * 18
    tmp = tempfile.mkdtemp(prefix="tweak-", dir=os.path.join(HERE, "captures", "_scratch"))

    # 1. load the patch, capturing the tone pushes. QEMU allows one QMP
    # client at a time, so every QMP connection here is closed before
    # focus.sh (which opens its own) runs.
    sh("./focus.sh", vmid, "hw")
    q = vmctl.QMP(vmid)
    cap = os.path.join(tmp, "load.pcapng")
    capture_start(cap)
    q.dclick(x, y)
    q.close()
    time.sleep(3)
    lines = capture_stop(cap)
    tones = {}
    for t, m in reassemble(lines, "OUT"):
        if m[0] == 0x02 and m[7] == 0x04:
            tones[m[8]] = parse_tone(m[12:])
    if 0 not in tones:
        sys.exit("no tone push captured - did the patch load?")
    for tone in (0, 1):
        if tone in tones:
            sweep_tone(vmid, tone, tones[tone], f"{bank}{ch}", tmp, out)


def sweep_tone(vmid, tone, parsed, patch, tmp, out):
    """Sweep Tweak <tone+1>. Tweak 2 sits 24px below Tweak 1."""
    name, blocks = parsed
    button = (TWEAK1[0], TWEAK1[1] + 24 * tone)
    first_row = FIRST_ROW_Y + 24 * tone

    # read the menu
    sh("./focus.sh", vmid, "main")
    q = vmctl.QMP(vmid)
    q.click(*button)
    time.sleep(0.8)
    q.move(button[0], button[1] - 20)   # park the pointer off the entries
    time.sleep(0.3)
    labels = ocr_menu(q, os.path.join(tmp, f"menu{tone}.png"), first_row - 10)
    q.key("esc")
    time.sleep(0.4)
    q.close()

    # pick every entry, capturing the keys
    cap = os.path.join(tmp, f"sweep{tone}.pcapng")
    capture_start(cap)
    q = vmctl.QMP(vmid)
    for i in range(len(labels)):
        q.click(*button)
        time.sleep(0.6)
        q.click(70, first_row + i * ROW_H)
        time.sleep(0.5)
    q.close()
    lines = capture_stop(cap)
    picks, block_idx = [], None
    for t, m in reassemble(lines, "OUT"):
        if m[0] == 0x05 and m[7] == 0x16 and m[8] == tone:
            param = struct.unpack_from("<I", m, 16)[0]
            if param == 0x1E:
                block_idx = struct.unpack_from("<I", m, 20)[0]
            elif param == 0x1F:
                picks.append((block_idx, struct.unpack_from("<I", m, 20)[0]))

    # pair and check
    if len(picks) != len(labels):
        print(f"WARNING tone {tone + 1}: {len(labels)} labels but {len(picks)} picks; "
              "pairing by order", file=sys.stderr)
    with open(out, "a") as f:
        for label, (bi, key) in zip(labels, picks):
            blk_type, knob = normalize(label)
            b = blocks[bi] if bi is not None and bi < len(blocks) else None
            rec = {"patch": patch, "tone_index": tone, "tone": name, "label": label,
                   "knob": knob, "block_index": bi, "block": block_name(b) if b else None,
                   "prefix_block": blk_type, "table": (b["kind"] & 0xFF) if b else None,
                   "category": (b["kind"] >> 8) if b else None,
                   "model": b["model"] if b else None, "param": f"{key:08x}"}
            ok = rec["block"] == blk_type and (("mix" in knob.lower()) == (key >> 16 == 0x3F01))
            rec["consistent"] = ok
            f.write(json.dumps(rec) + "\n")
            print(f"{'ok ' if ok else '?? '}t{tone + 1} {label:22} block[{bi}] "
                  f"{str(rec['block']):7} model {rec['model']} param {key:08x}")

if __name__ == "__main__":
    main()
