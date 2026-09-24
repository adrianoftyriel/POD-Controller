#!/usr/bin/env bash
# put8d.sh <vmid> <name> <prev.bin> : PUT the edited active patch to slot 8D
# (bank 8 is cleared for overwriting ONLY), then diff the written blob
# against prev.bin. Saves the new blob to captures/_scratch/<name>.bin.
set -euo pipefail
cd "$(dirname "$0")"
VMID=$1 NAME=$2 PREV=$3
./focus.sh "$VMID" hw
./vmctl.py "$VMID" click 57 723; sleep 0.5          # select 8D row
./vmctl.py "$VMID" move 1000 900; sleep 0.3
./vmctl.py "$VMID" click 213 55; sleep 1.5          # PUT SELECTED
SETTLE=4 ./run-action.sh "$VMID" "$NAME" "PUT 8D ($NAME)" \
    --steps actions/hw-put-confirm-yes-1280.steps | tail -1
python3 - "$NAME" "$PREV" <<'PY'
import glob, struct, sys
name, prev = sys.argv[1], sys.argv[2]
sys.argv = ['x']
exec(open('msgs.py').read().split("def main")[0])
d = sorted(glob.glob(f'captures/*-{name}'))[-1]
ms = [m for t, m in reassemble([l.split() for l in open(d + '/bulk.txt')], 'OUT')]
w = [m for m in ms if m[0] == 2 and m[7] == 2]
if not w:
    sys.exit("no write message captured (was the patch edited and 8D selected?)")
w = w[0]
assert struct.unpack_from('<I', w, 8)[0] == 31, "write was not to slot 31 (8D)!"
new, old = w[12:], open(prev, 'rb').read()
open(f'captures/_scratch/{name}.bin', 'wb').write(new)
for i in range(4096):
    if new[i] != old[i]:
        off = i % 2048
        rec = 'hdr' if off < 0xE4 else f'blk{(off-0xE4)//0x8C}+{(off-0xE4)%0x8C:#04x}'
        print(f'tone{i//2048+1} 0x{off:03x} {rec:12} {old[i]:02x} -> {new[i]:02x}')
PY
