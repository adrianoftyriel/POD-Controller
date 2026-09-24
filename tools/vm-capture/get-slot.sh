#!/usr/bin/env bash
# Read one patch slot from the POD via Gearbox's GET SELECTED, as a
# captured action, and save the 4096-byte EffectDump.
#
# Usage: ./get-slot.sh <vmid> <bank 1-16> <A-D>
# Output: captures/dumps/<slot>.bin (slot = (bank-1)*4 + channel)
#
# Gearbox only enables GET/PUT when its copy of the ACTIVE patch differs
# from the hardware, and then only acts on that patch. So: load the slot,
# toggle the gate block twice (marks it edited without changing it), then
# GET it. Assumes the 1280x1024 layout with both GearBox windows open, and
# that the gate is the first block in the chain (true for every patch seen
# so far).
set -euo pipefail
cd "$(dirname "$0")"
VMID=${1:?usage: get-slot.sh <vmid> <bank> <A-D>}
BANK=${2:?bank}
CH=${3:?channel A-D}
ci=$(( $(printf '%d' "'${CH^^}") - 65 ))
SLOT=$(( (BANK - 1) * 4 + ci ))
# Hardware Memory window geometry at 1280x1024: banks 1-8 left column,
# 9-16 right; 82px per bank, 18px per channel row.
if (( BANK <= 8 )); then X=57; B=$((BANK - 1)); else X=476; B=$((BANK - 9)); fi
Y=$(( 95 + B * 82 + ci * 18 ))

./focus.sh "$VMID" hw
./vmctl.py "$VMID" dclick "$X" "$Y"; sleep 3            # load slot
./focus.sh "$VMID" main
./vmctl.py "$VMID" click 229 282; sleep 0.5              # gate x2 = edited
./vmctl.py "$VMID" click 229 282; sleep 0.8
./focus.sh "$VMID" hw
./vmctl.py "$VMID" click "$X" "$Y"; sleep 0.5            # select row
./vmctl.py "$VMID" move 1000 900; sleep 0.3
./vmctl.py "$VMID" click 330 55; sleep 1.5               # GET SELECTED
SETTLE=4 ./run-action.sh "$VMID" "get-slot-$SLOT" "GET slot $SLOT ($BANK${CH^^})" \
    --steps actions/hw-get-confirm-yes-1280.steps | tail -1
dir=$(ls -d captures/[0-9][0-9][0-9]-get-slot-$SLOT | tail -1)
mkdir -p captures/dumps
./msgs.py "$dir" --dump-dir captures/dumps >/dev/null
f=captures/dumps/$(printf '%02d' "$SLOT").bin
[[ -s "$f" && "$f" -nt "$dir/before.png" ]] || { echo "no dump for slot $SLOT" >&2; exit 1; }
echo "slot $SLOT -> $f"
