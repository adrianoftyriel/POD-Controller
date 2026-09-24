#!/usr/bin/env bash
# focus.sh <vmid> <main|hw>: Alt-Tab until that GearBox window is in front.
set -euo pipefail
cd "$(dirname "$0")"
for _ in 1 2 3; do
    [[ "$(./front.py "$1")" == "$2" ]] && exit 0
    ./vmctl.py "$1" key alt+tab
    sleep 1
done
echo "focus.sh: could not bring '$2' to front" >&2
exit 1
