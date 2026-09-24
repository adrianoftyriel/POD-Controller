#!/usr/bin/env bash
# Capture raw USB traffic for a specific vendor:product device on the
# Proxmox host via usbmon.
#
# This only works if the POD X3 is attached to the VM via per-device
# passthrough (`qm set <vmid> -usbN host=<vendor>:<product>`), which uses
# QEMU's usb-host backend on top of the host's normal USB stack. Traffic is
# still visible to the host kernel (and therefore usbmon) even though
# QEMU is forwarding it into the guest. Full PCI passthrough of the USB
# controller instead hands the controller to the guest entirely and the
# host will see nothing - don't use that mode for this.
set -euo pipefail

usage() {
    echo "usage: $0 start <vendor:product> <output.pcapng>" >&2
    echo "       $0 stop <pidfile>" >&2
    exit 1
}

find_bus() {
    # find_bus <vendor:product> -> prints usb bus number (no leading zeros)
    local vidpid=$1
    lsusb -d "$vidpid" | awk '{print $2}' | sed 's/^0*//'
}

start_capture() {
    local vidpid=$1 outfile=$2
    modprobe usbmon 2>/dev/null || true
    local bus
    bus=$(find_bus "$vidpid")
    if [[ -z "$bus" ]]; then
        echo "device $vidpid not found via lsusb - is it plugged into the host?" >&2
        exit 1
    fi
    local iface="usbmon${bus}"
    [[ -e "/dev/${iface}" ]] || iface="usbmon0"
    dumpcap -q -i "$iface" -w "$outfile" 2>/dev/null &
    echo $! > "${outfile}.pid"
    echo "capturing on $iface (bus $bus, device $vidpid) -> $outfile (pid $(cat "${outfile}.pid"))"
}

stop_capture() {
    local pidfile=$1
    local pid
    pid=$(cat "$pidfile")
    kill -INT "$pid" 2>/dev/null || true
    # dumpcap was started by a different invocation of this script, so it
    # is not our child and `wait` can't see it - poll until it has exited
    # and flushed the pcapng.
    local i
    for (( i=0; i<50; i++ )); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    rm -f "$pidfile"
}

case "${1:-}" in
    start) start_capture "$2" "$3" ;;
    stop) stop_capture "$2" ;;
    *) usage ;;
esac
