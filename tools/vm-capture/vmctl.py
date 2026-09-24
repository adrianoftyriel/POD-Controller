#!/usr/bin/env python3
"""Fast scripted control of a Proxmox VM over its QMP socket.

`qm monitor` costs ~0.8s per command (it spawns a Perl process each time),
which turns a click into a ~1s press-and-hold and makes knob drags
impossible. This talks QMP directly on /var/run/qemu-server/<vmid>.qmp
and sends input via `input-send-event`, so event timing is under our
control. Stdlib only - runs on a stock Proxmox host.

Coordinates are guest screen pixels; the screen size is read from a fresh
screendump, so they stay correct if the guest resolution changes.

Usage:
  vmctl.py <vmid> screenshot <out.png|out.ppm>
  vmctl.py <vmid> click <x> <y> [left|right|middle]
  vmctl.py <vmid> dclick <x> <y>
  vmctl.py <vmid> drag <x1> <y1> <x2> <y2> [steps]
  vmctl.py <vmid> wheel <x> <y> <up|down> [count]
  vmctl.py <vmid> move <x> <y>
  vmctl.py <vmid> key <qcode>[+<qcode>...]     e.g. key ret, key ctrl+s
  vmctl.py <vmid> steps <file>                 one command per line, see below

A steps file holds one of the commands above per line (without the vmid),
plus `sleep <seconds>`. Blank lines and `#` comments are ignored.
"""
import json
import os
import socket
import struct
import sys
import tempfile
import time
import zlib

AXIS_MAX = 32767


class QMP:
    def __init__(self, vmid):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(f"/var/run/qemu-server/{vmid}.qmp")
        self.buf = b""
        self._read()  # greeting
        self.cmd("qmp_capabilities")
        self._size = None

    def _read(self):
        while b"\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("QMP socket closed")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def cmd(self, execute, **arguments):
        msg = {"execute": execute}
        if arguments:
            msg["arguments"] = arguments
        self.sock.sendall(json.dumps(msg).encode() + b"\n")
        while True:
            resp = self._read()
            if "event" in resp:
                continue
            if "error" in resp:
                raise RuntimeError(f"{execute}: {resp['error']}")
            return resp.get("return")

    # --- screen -------------------------------------------------------
    def screendump(self, path):
        self.cmd("screendump", filename=path)
        with open(path, "rb") as f:
            data = f.read()
        w, h, pixels = parse_ppm(data)
        self._size = (w, h)
        return w, h, pixels

    def size(self):
        if self._size is None:
            fd, tmp = tempfile.mkstemp(suffix=".ppm")
            os.close(fd)
            try:
                self.screendump(tmp)
            finally:
                os.unlink(tmp)
        return self._size

    # --- input --------------------------------------------------------
    def _abs(self, x, y):
        w, h = self.size()
        return [
            {"type": "abs", "data": {"axis": "x", "value": x * AXIS_MAX // (w - 1)}},
            {"type": "abs", "data": {"axis": "y", "value": y * AXIS_MAX // (h - 1)}},
        ]

    def move(self, x, y):
        self.cmd("input-send-event", events=self._abs(x, y))

    def button(self, btn, down):
        self.cmd("input-send-event",
                 events=[{"type": "btn", "data": {"button": btn, "down": down}}])

    def click(self, x, y, btn="left", hold=0.08):
        self.move(x, y)
        time.sleep(0.05)
        self.button(btn, True)
        time.sleep(hold)
        self.button(btn, False)

    def dclick(self, x, y):
        self.click(x, y)
        time.sleep(0.08)
        self.click(x, y)

    def drag(self, x1, y1, x2, y2, steps=20):
        self.move(x1, y1)
        time.sleep(0.1)
        self.button("left", True)
        time.sleep(0.1)
        for i in range(1, steps + 1):
            self.move(x1 + (x2 - x1) * i // steps, y1 + (y2 - y1) * i // steps)
            time.sleep(0.03)
        time.sleep(0.1)
        self.button("left", False)

    def wheel(self, x, y, direction, count=1):
        self.move(x, y)
        time.sleep(0.05)
        btn = "wheel-up" if direction == "up" else "wheel-down"
        for _ in range(count):
            self.button(btn, True)
            self.button(btn, False)
            time.sleep(0.08)

    def key(self, combo):
        keys = [{"type": "qcode", "data": k} for k in combo.split("+")]
        self.cmd("send-key", keys=keys)


def parse_ppm(data):
    # P6 binary PPM as written by QEMU's screendump.
    fields, pos = [], 0
    while len(fields) < 4:
        while data[pos:pos + 1].isspace():
            pos += 1
        if data[pos:pos + 1] == b"#":
            pos = data.index(b"\n", pos) + 1
            continue
        end = pos
        while not data[end:end + 1].isspace():
            end += 1
        fields.append(data[pos:end])
        pos = end
    if fields[0] != b"P6":
        raise ValueError("not a P6 PPM")
    w, h = int(fields[1]), int(fields[2])
    return w, h, data[pos + 1:pos + 1 + w * h * 3]


def write_png(path, w, h, rgb):
    # Minimal PNG writer - pve has no PIL, and this avoids needing
    # imagemagick just to view screenshots.
    raw = b"".join(b"\x00" + rgb[y * w * 3:(y + 1) * w * 3] for y in range(h))

    def chunk(tag, body):
        return (struct.pack(">I", len(body)) + tag + body
                + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 6)))
        f.write(chunk(b"IEND", b""))


def screenshot(q, out):
    # screendump is written by the QEMU process, so dump to a temp file it
    # can definitely write, then convert/move to the requested path.
    fd, tmp = tempfile.mkstemp(suffix=".ppm")
    os.close(fd)
    try:
        w, h, pixels = q.screendump(tmp)
        if out.endswith(".ppm"):
            os.replace(tmp, out)
        else:
            write_png(out, w, h, pixels)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def run(q, argv):
    op, args = argv[0], argv[1:]
    ints = lambda n: [int(a) for a in args[:n]]
    if op == "screenshot":
        screenshot(q, args[0])
    elif op == "click":
        x, y = ints(2)
        q.click(x, y, args[2] if len(args) > 2 else "left")
    elif op == "dclick":
        q.dclick(*ints(2))
    elif op == "drag":
        q.drag(*ints(4), steps=int(args[4]) if len(args) > 4 else 20)
    elif op == "wheel":
        x, y = ints(2)
        q.wheel(x, y, args[2], int(args[3]) if len(args) > 3 else 1)
    elif op == "move":
        q.move(*ints(2))
    elif op == "key":
        q.key(args[0])
    elif op == "sleep":
        time.sleep(float(args[0]))
    elif op == "steps":
        with open(args[0]) as f:
            for line in f:
                line = line.split("#", 1)[0].strip()
                if line:
                    run(q, line.split())
    else:
        sys.exit(f"unknown command: {op}\n\n{__doc__}")


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    run(QMP(sys.argv[1]), sys.argv[2:])


if __name__ == "__main__":
    main()
