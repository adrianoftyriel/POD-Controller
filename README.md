# POD Controller

An open-source, cross-platform replacement for Line6's abandoned **Gearbox**
software: set up patches and edit effects on a **Line6 POD X3 / X3 Live**
over USB, from Windows, macOS, and (eventually) Android.

## Why this is harder than it sounds

The POD X3 is **not** a USB class-compliant MIDI or audio device. It uses a
proprietary, undocumented USB bulk-transfer protocol (vendor class `0xFF`).
Line6 never published this protocol, and the official Gearbox app is the
only software that ever fully spoke it. See [`docs/PROTOCOL.md`](docs/PROTOCOL.md)
for everything currently known about the wire format, its sources, and what's
still unknown.

This project builds on prior reverse-engineering work by
[andree182/podx3](https://github.com/andree182/podx3) (raw USB protocol,
GPLv2) and tracks the parallel community effort in
[arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70).

## Status

Confirmed on a real POD X3 Live (2026-09-25): reading patches (`dump`),
writing them (`restore`, byte-identical on readback), loading a stored
patch (`select`), live edits (`set-amp`, `block`) and setting queries
(`query`). Hundreds of edits in a row go through without trouble.

The "wedge" that earlier looked like a flaky USB link was two host-side
bugs, both fixed in `PodDevice` and described in `docs/PROTOCOL.md` under
"Host requirements":

- the POD accepts a bulk OUT message only while the host has a bulk-IN
  read pending, so pod-core now keeps IN transfers queued all the time;
- it drops a message whose packets arrive back-to-back, so writes now go
  out as one 64-byte transfer per packet.

On Linux, the kernel's `snd_usb_podhd` driver binds to the POD and has to
be kept off it (`blacklist snd_usb_podhd` in `/etc/modprobe.d/`, or unbind
it) before pod-core can claim the control interface.

### Browser editor

`pod-cli serve --port 8080` runs a web editor for the connected POD:
- browse all 64 patches (bank strip or an all-banks pop-up);
- load, rename, and save to any slot;
- per-block model menus and vertical faders for every parameter;
- block on/off and pre/post moves;
- cab, mic and room settings, and Variax model and tone.

Edits are live and stay in a working copy until you save. The UI is
static files in `crates/pod-cli/ui/`, and the brief for restyling it is
in [`designprompt.md`](designprompt.md).

### MCP server

The same server speaks the [Model Context Protocol](https://modelcontextprotocol.io)
at `http://<host>:8080/mcp` (Streamable HTTP), so an AI agent can drive the
POD by name. It can list and load patches, read the whole chain, change
models and parameters, and switch blocks on/off or pre/post. It can also set
Variax, mic and room, rename, and save. It shares the working copy with the
web UI, and the UI follows the agent's edits. For example, with Claude Code:

```
claude mcp add --transport http pod http://172.16.88.33:8080/mcp
```

Knob values are 0-100 (real units where a parameter has one), tones are 1
and 2, and patches are codes like `08C`. `save_patch` refuses to replace a
different named patch unless it's given `overwrite: true`. There's no
authentication, so keep it on a trusted LAN. Requests from foreign web
origins are rejected.

## Planned phasing

1. **Core protocol crate** (`crates/pod-core`, Rust, using [`nusb`](https://docs.rs/nusb)):
   device discovery, control handshake, bulk framing, EffectDump
   (patch blob) get/set, and int/float live-parameter get/set — the pieces
   already confirmed working by prior reverse-engineering.
2. **Patch librarian MVP**: back up, restore, and swap patches between slots,
   treating the 4096-byte EffectDump blob as opaque. This does not require
   decoding its internal layout.
3. **Live effect editing MVP**: get/set individual effect parameters
   (volume, gain, etc.) on the currently loaded patch via the confirmed
   float/int parameter messages.
4. **Blob decoding**: reverse-engineer the internal layout of the EffectDump
   blob (which bytes encode which effect model/parameter) by diffing dumps
   taken before/after changes made on the unit's own front panel. This
   unlocks full patch editing (building effect chains from scratch, naming
   patches, etc.) — see `docs/PROTOCOL.md` for the diffing methodology.
5. **Desktop UI** (Windows/macOS) on top of `pod-core`.
6. **Android port**: reimplement the transport layer on Android's native USB
   host API (`android.hardware.usb`), reusing the protocol/patch-model logic.

## License

GPL-3.0, for consistency with the reverse-engineering projects this work
builds on (`pod-ui`, `l6t-rs`, `podx3`). Open to revisiting this.
