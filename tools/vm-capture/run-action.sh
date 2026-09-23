#!/usr/bin/env bash
# Capture one deliberate, logged action against the POD X3 running under
# Gearbox in the Windows VM: before/after screenshots plus a labeled
# usbmon capture, so traffic can be attributed to a specific UI action.
# Mirrors the black-box diffing methodology in docs/PROTOCOL.md, but now
# with the actual USB traffic instead of just EffectDump diffs.
#
# Usage:
#   ./run-action.sh <vmid> <action-name> ["<description>"]
#
# This does not click anything for you. It brackets a capture window
# around one manual action; scripted clicking (via vm_click in lib.sh) can
# replace the manual step once real Gearbox screen coordinates are known
# from a captured screenshot.
set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

VMID=${1:?usage: run-action.sh <vmid> <action-name> [description]}
NAME=${2:?usage: run-action.sh <vmid> <action-name> [description]}
DESC=${3:-}
VIDPID="0e41:414b"

mkdir -p captures
SEQ=$(printf '%03d' "$(ls -1 captures 2>/dev/null | grep -c '^[0-9]\{3\}-' || true)")
OUTDIR="captures/${SEQ}-${NAME}"
mkdir -p "$OUTDIR"

echo "before screenshot..."
vm_screenshot_png "$VMID" "$OUTDIR/before.png"

echo "starting usb capture..."
./usb-capture.sh start "$VIDPID" "$OUTDIR/traffic.pcapng"

echo
echo "Perform exactly ONE action in Gearbox now (e.g. load one patch,"
echo "tweak one knob, rename one patch). Press Enter when done."
read -r

./usb-capture.sh stop "$OUTDIR/traffic.pcapng.pid"

echo "after screenshot..."
vm_screenshot_png "$VMID" "$OUTDIR/after.png"

{
    echo "action: $NAME"
    echo "description: $DESC"
    echo "timestamp: $(date -Iseconds)"
    echo "vmid: $VMID"
    echo "dir: $OUTDIR"
    echo
} >> captures/actions.log

echo "saved to $OUTDIR"
