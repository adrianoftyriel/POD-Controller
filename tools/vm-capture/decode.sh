#!/usr/bin/env bash
# Print the POD X3's bulk-endpoint payloads from a usbmon pcapng, one per
# line: <seconds> <OUT|IN> <hex>. OUT = host->POD (endpoint 0x01),
# IN = POD->host (endpoint 0x81). Control transfers and other devices on
# the bus are dropped.
#
# Usage: ./decode.sh <traffic.pcapng> [vendor:product]
set -euo pipefail
PCAP=${1:?usage: decode.sh <traffic.pcapng> [vendor:product]}
VIDPID=${2:-0e41:414b}

# usbmon numbers devices per bus; the address changes on replug, so look
# it up now rather than hardcoding it.
ADDR=$(lsusb -d "$VIDPID" | awk '{print $4}' | tr -d : | sed 's/^0*//')
[[ -n "$ADDR" ]] || { echo "device $VIDPID not found" >&2; exit 1; }

# OUT data is on the submit ('S') record, IN data on the completion ('C').
tshark -r "$PCAP" \
    -Y "usb.device_address==$ADDR && usb.transfer_type==0x03 && usb.capdata && ((usb.endpoint_address==0x01 && usb.urb_type==83) || (usb.endpoint_address==0x81 && usb.urb_type==67))" \
    -T fields -e frame.time_relative -e usb.endpoint_address -e usb.capdata 2>/dev/null |
awk -F'\t' '{ printf "%.3f %s %s\n", $1, ($2=="0x01" ? "OUT" : "IN "), $3 }'
