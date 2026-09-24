#!/usr/bin/env bash
# Run several scripted actions back to back, one capture per action.
#
# Usage: ./run-suite.sh <vmid> actions/foo.steps [actions/bar.steps ...]
#
# The action name is the steps file's basename; its description is the
# first line of the file if that line is a "# " comment.
set -euo pipefail
cd "$(dirname "$0")"
VMID=${1:?usage: run-suite.sh <vmid> <steps-file>...}
shift
for f in "$@"; do
    name=$(basename "$f" .steps)
    desc=$(head -1 "$f" | sed -n 's/^# //p')
    echo "=== $name"
    ./run-action.sh "$VMID" "$name" "$desc" --steps "$f"
    sleep "${GAP:-1}"
done
