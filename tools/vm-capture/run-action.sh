#!/usr/bin/env bash
# Capture one deliberate, logged action against the POD X3 running under
# Gearbox in the Windows VM: before/after screenshots plus a labeled
# usbmon capture, so traffic can be attributed to a specific UI action.
# Mirrors the black-box diffing methodology in docs/PROTOCOL.md, but now
# with the actual USB traffic instead of just EffectDump diffs.
#
# Usage:
#   ./run-action.sh <vmid> <action-name> ["<description>"] [--steps <file>]
#
# Without --steps it waits for you to perform the action by hand on the VM
# console. With --steps it plays a vmctl.py steps file (see actions/)
# instead, so a run needs no human at all.
set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

VMID=${1:?usage: run-action.sh <vmid> <action-name> [description]}
NAME=${2:?usage: run-action.sh <vmid> <action-name> [description]}
DESC=""
STEPS=""
shift 2
while (( $# )); do
    case "$1" in
        --steps) STEPS=${2:?--steps needs a file}; shift 2 ;;
        *) DESC=$1; shift ;;
    esac
done
VIDPID="0e41:414b"

mkdir -p captures
SEQ=$(printf '%03d' "$(ls -1 captures 2>/dev/null | grep -c '^[0-9]\{3\}-' || true)")
OUTDIR="captures/${SEQ}-${NAME}"
mkdir -p "$OUTDIR"

echo "before screenshot..."
./vmctl.py "$VMID" screenshot "$OUTDIR/before.png"

echo "starting usb capture..."
./usb-capture.sh start "$VIDPID" "$OUTDIR/traffic.pcapng"

if [[ -n "$STEPS" ]]; then
    cp "$STEPS" "$OUTDIR/action.steps"
    # Give dumpcap a moment to open the interface, then play the action
    # and let the device answer before the capture window closes.
    sleep 1
    echo "playing $STEPS..."
    ./vmctl.py "$VMID" steps "$STEPS"
    sleep "${SETTLE:-2}"
else
    echo
    echo "Perform exactly ONE action in Gearbox now (e.g. load one patch,"
    echo "tweak one knob, rename one patch). Press Enter when done."
    read -r
fi

./usb-capture.sh stop "$OUTDIR/traffic.pcapng.pid"

echo "after screenshot..."
./vmctl.py "$VMID" screenshot "$OUTDIR/after.png"

{
    echo "action: $NAME"
    echo "description: $DESC"
    echo "timestamp: $(date -Iseconds)"
    echo "vmid: $VMID"
    [[ -n "$STEPS" ]] && echo "steps: $STEPS"
    echo "dir: $OUTDIR"
    echo
} >> captures/actions.log

./decode.sh "$OUTDIR/traffic.pcapng" > "$OUTDIR/bulk.txt" || true
echo "saved to $OUTDIR ($(grep -c . "$OUTDIR/bulk.txt" || true) bulk messages)"
