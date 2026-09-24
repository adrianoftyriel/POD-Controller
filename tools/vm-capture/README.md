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

**The layout depends on the amp model.** Each amp has its own skin, so
knob positions (and even the block row) move when the amp changes. The
coordinates above are for "1958 Tweed B-Man" (patch 5A, Tone 1). Take a
screenshot (`./vmctl.py <vmid> screenshot x.png`) and check before writing
steps for another tone.

Model menus: AMP MODEL (290,221) and CAB MODEL (600,221) open a menu, then a
"Guitar ... Models" submenu. See `actions/amp-model-*.steps` and
`actions/cab-*.steps` for the hover path that keeps the submenu open. The
effect panel's EFFECT MODEL field is at (350,437).

### Guest display and the patch list

The guest runs at **1280x1024** so the Hardware Memory window (patch list)
shows all 16 banks: bank `b` channel `c` (A=0..D=3) has its channel
letter at x=57 (banks 1-8) or x=476 (9-16), y = 95 + ((b-1) % 8)·82 + c·18.
The main GearBox window keeps its position, so the coordinates above
still apply. The patch list sits on top of the main window, so use
`./focus.sh <vmid> main|hw` (it Alt-Tabs, and `front.py` checks the patch
list's title bar colour) before clicking in either one.

- Double-click a channel letter to load that patch (a single click only
  selects it; clicking a tone cell selects just that tone, which GET/PUT
  ignore).
- **GET/PUT are only enabled while the active patch is edited** (italic),
  and then only act on that patch. `./get-slot.sh <vmid> <bank> <A-D>`
  handles it: load, toggle the gate twice, GET SELECTED, confirm, and save
  `captures/dumps/<slot>.bin`.
- GET/PUT dialogs: Yes is at (730,570) for GET and (735,570) for PUT.
- **PUT writes to the POD. Only use it on bank 8 slots**, which the owner
  has cleared for overwriting. The originals are backed up on the host in
  `/root/pod-bank8-backup-2026-09-24/`.

### Analysis helpers

- `msgs.py <capture-dir> [--dump-dir DIR]`: reassemble a capture's bulk
  payloads into messages. With `--dump-dir` it also saves every EffectDump
  reply as `<slot>.bin`.
- `parse_tone.py <dump.bin>...`: print both tones' 12 block records
  (slot/group, model, enabled, params) from a 4096-byte dump.
- `vmctl.py <vmid> type <text>` types printable ASCII (US layout).

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
