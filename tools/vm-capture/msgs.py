#!/usr/bin/env python3
"""Reassemble a capture's bulk.txt (from decode.sh) into POD X3 messages.

Usage: msgs.py <capture-dir> [--dump-dir DIR]

Prints one line per message per direction. With --dump-dir, every
EffectDump reply (type 01) is written there as <slot>.bin (the 4096-byte
blob, header stripped), where the slot comes from the preceding request.
"""
import os
import sys


def reassemble(lines, direction):
    stream = b"".join(bytes.fromhex(l[2]) for l in lines if l[1] == direction)
    msgs, i, cur, t = [], 0, None, None
    # Timestamps are per-URB; take the first URB's time for each message.
    times = [float(l[0]) for l in lines if l[1] == direction]
    lens = [len(bytes.fromhex(l[2])) for l in lines if l[1] == direction]
    urb_starts, pos = [], 0
    for n in lens:
        urb_starts.append(pos)
        pos += n
    ui = 0
    while i < len(stream):
        while ui + 1 < len(urb_starts) and urb_starts[ui + 1] <= i:
            ui += 1
        n = stream[i]
        hdr, body = stream[i:i + 4], stream[i + 4:i + 4 + n]
        i += 4 + n
        if hdr[2] & 0x01:
            if cur is not None:
                msgs.append((t, bytes(cur)))
            cur, t = bytearray(body), times[ui]
        elif cur is not None:
            cur += body
    if cur is not None:
        msgs.append((t, bytes(cur)))
    return msgs


def describe(m):
    text = bytes(c if 32 <= c < 127 else 46 for c in m[8:24]).decode()
    return (f"len={len(m):5} type={m[0]:02x} route={m[2:6].hex()} "
            f"sub={m[7]:02x} head={m[:24].hex()}  {text}")


def main():
    d = sys.argv[1]
    dump_dir = sys.argv[sys.argv.index("--dump-dir") + 1] if "--dump-dir" in sys.argv else None
    lines = [l.split() for l in open(os.path.join(d, "bulk.txt"))]
    out, inn = reassemble(lines, "OUT"), reassemble(lines, "IN")
    for name, msgs in (("OUT", out), ("IN", inn)):
        print(f"== {name} ({len(msgs)} msgs)")
        for t, m in msgs:
            print(f"  {t:8.3f} {describe(m)}")
    if dump_dir:
        os.makedirs(dump_dir, exist_ok=True)
        reqs = [(t, int.from_bytes(m[8:12], "little")) for t, m in out
                if m[0] == 0x02 and m[7] == 0x00]
        for t, m in inn:
            if m[0] == 0x01 and m[7] == 0x01:
                slot = max((r for r in reqs if r[0] <= t), default=(0, -1))[1]
                with open(os.path.join(dump_dir, f"{slot:02d}.bin"), "wb") as f:
                    f.write(m[8:])


if __name__ == "__main__":
    main()
