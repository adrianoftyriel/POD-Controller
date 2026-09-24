# VM capture toolkit

Host-side (Proxmox) tooling to correlate Gearbox UI actions with POD X3 USB
traffic, without installing any capture/automation software inside the
Windows 7 guest. Everything here runs on the Proxmox host and drives the
VM through QEMU's monitor (`qm monitor`) and captures traffic via the
host's `usbmon`.

## Prerequisites (one-time, on the Proxmox host)

1. Pass the POD X3 through to the VM by **device**, not by PCI controller:
   ```
   qm set <vmid> -usb0 host=0e41:414b
   ```
   Controller-level PCI passthrough hands the whole USB controller to the
   guest, and the host loses visibility into the traffic - `usbmon` would
   see nothing. Device-level `usb-host` passthrough keeps the host kernel
   in the loop (via libusb), which is what makes host-side capture and
   `lsusb` visibility both work.

2. Enable an absolute pointing device so scripted clicks land where
   expected:
   ```
   qm set <vmid> --tablet 1
   ```
   Without this, `mouse_move` is relative and reliable clicking from a
   screenshot's pixel coordinates isn't really possible.

3. Load `usbmon` and install capture tools:
   ```
   modprobe usbmon
   apt install wireshark-common tshark imagemagick   # dumpcap, decode.sh, lib.sh
   ```

## Usage

### Scripted (no human needed)

```
tools/vm-capture/run-suite.sh <vmid> actions/gate-on.steps actions/drive-down.steps ...
tools/vm-capture/run-action.sh <vmid> <name> "<description>" --steps actions/foo.steps
```

A steps file is a list of `vmctl.py` commands (`click x y`, `drag x1 y1 x2 y2
[steps]`, `wheel x y up|down [n]`, `key ret`, `sleep 0.5`, ...) in guest
screen pixels. Each run writes `before.png`, `after.png`, `traffic.pcapng`,
the `action.steps` it played, and `bulk.txt` (the decoded bulk payloads, via
`decode.sh`) to `captures/<NNN-name>/`. `SETTLE=<s>` sets how long to keep
capturing after the last step (default 2), and `GAP=<s>` sets the pause
between suite actions.

`vmctl.py` talks QMP directly on `/var/run/qemu-server/<vmid>.qmp`. The
`qm monitor` path in `lib.sh` costs ~0.8s per command, which turns every
click into a 1s press-and-hold and rules out knob drags.

Known GearBox coordinates (800x600 guest, GearBox window at its default
position): amp knobs at y=130 (Drive 228, Bass 315, Middle 401, Treble 487,
Presence 573, Volume 660; drag vertically). Block ON/OFF strips are at
y=282 (Gate 229, Wah 277, Stomp 328, Amp 378, Comp 428, EQ 479, Vol 528,
Mod 627, Delay 677, Verb 727). Clicking the upper half of a block (y≈258)
opens its panel instead of toggling it.

### Manual

```
tools/vm-capture/run-action.sh <vmid> <action-name> ["<description>"]
```

This takes a "before" screenshot, starts a `usbmon` capture scoped to the
POD X3's bus, waits for you to perform exactly one action in Gearbox on
the VM's console, then stops the capture and takes an "after" screenshot.
Output lands in `captures/<NNN-action-name>/` (`before.png`, `after.png`,
`traffic.pcapng`), and every run appends an entry to `captures/actions.log`.

One action per run is the point - it's what makes the resulting pcap
attributable to a specific UI action, same rationale as the byte-diffing
methodology in `docs/PROTOCOL.md`.

## Primitives (`lib.sh`)

`vm_screenshot_png`, `vm_click`, `vm_key`, `vm_type` wrap the underlying
`qm monitor` HMP commands (`screendump`, `mouse_move`/`mouse_button`,
`sendkey`). These aren't wired into `run-action.sh` as automated clicks
yet, because the actual Gearbox window layout (button coordinates) isn't
known - `vm_type` in particular only maps lowercase letters/digits/space
today. Once a screenshot of the real Gearbox UI is available, the manual
"press Enter when done" step in `run-action.sh` can be replaced with a
scripted sequence of `vm_click`/`vm_key` calls for full automation.

## Getting captures analyzed

Commit small `captures/<n>/traffic.pcapng` directories to this repo (or
attach them directly) along with what action they correspond to - findings
get folded back into `docs/PROTOCOL.md`.
