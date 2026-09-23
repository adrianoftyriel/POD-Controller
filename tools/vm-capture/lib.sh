#!/usr/bin/env bash
# QEMU HMP monitor helpers for scripted control of a Proxmox VM, run on the
# Proxmox host itself (uses the `qm` CLI). No agent software needed inside
# the guest.
#
# Requires the VM to have an absolute pointing device (tablet) as its
# active pointer, so mouse_move coordinates map to screen position instead
# of relative deltas:
#   qm set <vmid> --tablet 1
set -euo pipefail

vm_monitor() {
    # vm_monitor <vmid> <hmp-command>
    # Never pipe "quit" through this - that command powers off the VM, it
    # does not just close the monitor session.
    local vmid=$1 cmd=$2
    echo "$cmd" | qm monitor "$vmid" >/dev/null
}

vm_screenshot() {
    # vm_screenshot <vmid> <host-ppm-path>
    # screendump writes the file from the QEMU process itself, so the path
    # must be writable by whatever user runs `qm` (root, typically).
    local vmid=$1 outfile=$2
    vm_monitor "$vmid" "screendump $outfile"
    sleep 0.3
}

vm_screenshot_png() {
    # vm_screenshot_png <vmid> <host-png-path>
    local vmid=$1 outfile=$2
    local ppm
    ppm=$(mktemp --suffix=.ppm)
    vm_screenshot "$vmid" "$ppm"
    convert "$ppm" "$outfile"
    rm -f "$ppm"
}

vm_click() {
    # vm_click <vmid> <pixel_x> <pixel_y> <screen_w> <screen_h> [button]
    # button: 1=left, 2=middle, 4=right (default left).
    # QEMU maps mouse_move to the 0-32767 axis range across the full
    # screen when an absolute (tablet) pointer is active - get screen_w/h
    # from vm_resolution against a screenshot taken moments before.
    local vmid=$1 px=$2 py=$3 sw=$4 sh=$5 button=${6:-1}
    local ax ay
    ax=$(( px * 32767 / sw ))
    ay=$(( py * 32767 / sh ))
    vm_monitor "$vmid" "mouse_move $ax $ay"
    vm_monitor "$vmid" "mouse_button $button"
    sleep 0.05
    vm_monitor "$vmid" "mouse_button 0"
}

vm_resolution() {
    # vm_resolution <png-path> -> prints "WIDTHxHEIGHT"
    identify -format "%wx%h" "$1"
}

vm_key() {
    # vm_key <vmid> <qemu-key-name-or-combo>
    # e.g. vm_key 101 tab / vm_key 101 ret / vm_key 101 ctrl-alt-delete
    local vmid=$1 key=$2
    vm_monitor "$vmid" "sendkey $key"
}

vm_type() {
    # vm_type <vmid> <text>
    # Best-effort: only lowercase letters, digits, and spaces are mapped.
    # Extend the case statement below once real inputs (e.g. patch rename
    # dialogs) are known - QEMU key names for punctuation/shift vary.
    local vmid=$1 text=$2
    local i c key
    for (( i=0; i<${#text}; i++ )); do
        c=${text:$i:1}
        case "$c" in
            [a-z0-9]) key="$c" ;;
            " ") key="spc" ;;
            *)
                echo "vm_type: unmapped character '$c', skipping" >&2
                continue
                ;;
        esac
        vm_key "$vmid" "$key"
        sleep 0.05
    done
}
