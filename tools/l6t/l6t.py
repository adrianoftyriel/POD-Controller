#!/usr/bin/env python3
"""Dump the chunk tree of a Line 6 .l6t tone file (IFF, big-endian).

Usage: l6t.py <file.l6t> [...]

Layout seen in GearBox 3.x files: FORM L6P2 { HEAD, LIST BANK { BINF,
FORM L6PA { PINF, LIST PATC { MODL { MINF, PARM... }... } }... } }.
PARM = <id u32> <kind u32: 0 int, 1 float> <value u32/f32>.
MINF = <model id u32> <u16> <u16> <u32> (see docs/L6T.md).
"""
import struct
import sys

CONTAINERS = {b"FORM", b"LIST"}


def utf16_name(b):
    return b.decode("utf-16-be", "replace").split("\0")[0]


def walk(data, off, end, depth, out):
    while off + 8 <= end:
        cid = data[off:off + 4]
        size = struct.unpack(">I", data[off + 4:off + 8])[0]
        body = data[off + 8:off + 8 + size]
        pad = " " * (depth * 2)
        if cid in CONTAINERS:
            out.append(f"{pad}{cid.decode()} {body[:4].decode()} ({size})")
            walk(data, off + 12, off + 8 + size, depth + 1, out)
        elif cid == b"PARM":
            pid, kind, raw = struct.unpack(">II4s", body[:12])
            val = struct.unpack(">f", raw)[0] if kind == 1 else struct.unpack(">I", raw)[0]
            out.append(f"{pad}PARM {pid:08x} {'f' if kind == 1 else 'i'} {val:.6g}" if kind == 1
                       else f"{pad}PARM {pid:08x} i {val}")
        elif cid == b"MINF":
            out.append(f"{pad}MINF {body.hex()}")
        elif cid in (b"PINF", b"BINF"):
            name = utf16_name(body[8:]) if len(body) > 8 else ""
            out.append(f"{pad}{cid.decode()} {body[:8].hex()} name={name!r} tail={body[-8:].hex()}")
        else:
            out.append(f"{pad}{cid.decode(errors='replace')} ({size}) {body[:16].hex()}")
        off += 8 + size + (size & 1)


def main():
    for path in sys.argv[1:]:
        data = open(path, "rb").read()
        out = []
        walk(data, 0, len(data), 0, out)
        print(f"== {path}")
        print("\n".join(out))


if __name__ == "__main__":
    main()
