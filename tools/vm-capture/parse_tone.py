#!/usr/bin/env python3
"""Parse POD X3 EffectDump tone blocks (see docs/PROTOCOL.md).

Usage: parse_tone.py <dump.bin> [...]   (4096-byte EffectDump = 2 tones)
"""
import struct
import sys

HEADER = 0xE4
BLOCK = 0x8C
NBLOCKS = 12
# (slot, group) -> block name, from the live int-set captures.
NAMES = {(0, 2): "gate", (2, 2): "wah", (3, 2): "stomp", (0, 3): "amp",
         (1, 3): "cab", (4, 3): "eq", (0, 5): "comp", (3, 5): "mod",
         (4, 5): "delay", (5, 5): "reverb"}


def fmt_value(kind, raw):
    if kind == 0x3F10:
        return f"{struct.unpack('<f', raw)[0]:.4f}"
    return raw.hex()


def parse_tone(t):
    name = t[:16].decode("ascii", "replace").rstrip()
    blocks = []
    for n in range(NBLOCKS):
        b = t[HEADER + n * BLOCK: HEADER + (n + 1) * BLOCK]
        model, kind, slot, group = struct.unpack_from("<HHHH", b, 0)
        enabled, count = b[8], b[11]
        params = []
        for p in range(count):
            idx, ptype = struct.unpack_from("<HH", b, 12 + p * 8)
            params.append((idx, ptype, b[16 + p * 8: 20 + p * 8]))
        tail = b[12 + count * 8:]
        blocks.append(dict(n=n, model=model, kind=kind, slot=slot, group=group,
                           enabled=enabled, pad=b[9:11], params=params,
                           tail_nonzero=any(tail)))
    return name, blocks


def main():
    for path in sys.argv[1:]:
        data = open(path, "rb").read()
        for tone in (0, 1):
            name, blocks = parse_tone(data[tone * 2048:(tone + 1) * 2048])
            print(f"== {path} tone {tone + 1}: {name!r}")
            for b in blocks:
                label = NAMES.get((b["slot"], b["group"]), "?")
                ps = " ".join(f"{i}:{fmt_value(t, v)}" + ("" if t == 0x3F10 else f"[{t:04x}]")
                              for i, t, v in b["params"])
                flag = " TAIL!" if b["tail_nonzero"] or any(b["pad"]) else ""
                print(f"  [{b['n']:2}] {label:6} slot={b['slot']:2} grp={b['group']} "
                      f"model={b['model']:3} kind={b['kind']:04x} on={b['enabled']}{flag}  {ps}")


if __name__ == "__main__":
    main()
