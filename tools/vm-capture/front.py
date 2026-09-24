#!/usr/bin/env python3
"""Print which GearBox window is in front: 'main' or 'hw'.

Reads one screendump. The
check is a single pixel that differs between the two windows at the
1280x1024 layout. Crude, but it spares a screenshot check before every
click.
"""
import os, sys, tempfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vmctl

q = vmctl.QMP(sys.argv[1])
fd, tmp = tempfile.mkstemp(suffix=".ppm"); os.close(fd)
w, h, px = q.screendump(tmp); os.unlink(tmp)
def rgb(x, y):
    i = (y * w + x) * 3
    return px[i], px[i + 1], px[i + 2]
# (1000, 13) is on the Hardware Memory window's title bar, which spans the
# whole screen width at 1280x1024. Win7 draws it saturated blue when that
# window is active, and washed out when the main GearBox window is.
r, g, b = rgb(1000, 13)
tan = (b - r) > 40
print("hw" if tan else "main", *([(r, g, b)] if "-v" in sys.argv else []))
