#!/usr/bin/env python3
"""Parse POD X3 EffectDump tone blocks (see docs/PROTOCOL.md).

Usage: parse_tone.py <dump.bin> [...]   (4096-byte EffectDump = 2 tones)
"""
import struct
import sys

HEADER = 0xE4
BLOCK = 0x8C
NBLOCKS = 12
# Record +2 (model table) -> block name. Slot/group can't be used: they
# are the chain position and move when a block is switched pre/post.
TABLES = {0x02: "delay", 0x03: "mod", 0x04: "reverb", 0x05: "stomp",
          0x06: "wah", 0x07: "volume", 0x0A: "stomp", 0x0C: "eq"}
MISC = {0: "gate", 1: "comp", 10: "loop"}   # table 0x0B, by model


def block_name(b):
    cat, table = b["kind"] >> 8, b["kind"] & 0xFF
    if cat == 0:
        return "amp"
    if cat == 1:
        return "cab"
    if table == 0x0B:
        return MISC.get(b["model"], "0B?")
    return TABLES.get(table, "?")


# Parameter namespaces (the u16 after idx). A parameter is identified by
# (idx, namespace); every value seen so far is a little-endian f32.
NS = {0x3F10: "", 0x3F01: "m", 0x3F00: "u"}   # knob 0-1, mix 0-1, real units


def fmt_value(kind, raw):
    return f"{struct.unpack('<f', raw)[0]:.4g}"


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
                label = block_name(b)
                ps = " ".join(f"{NS.get(t, f'[{t:04x}]')}{i}={fmt_value(t, v)}"
                              for i, t, v in b["params"])
                flag = " TAIL!" if b["tail_nonzero"] or any(b["pad"]) else ""
                print(f"  [{b['n']:2}] {label:6} slot={b['slot']:2} grp={b['group']} "
                      f"model={b['model']:3} kind={b['kind']:04x} on={b['enabled']}{flag}  {ps}")


if __name__ == "__main__":
    main()
