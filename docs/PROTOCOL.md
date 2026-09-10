# POD X3 USB Protocol — known facts, sources, and open questions

This document tracks everything currently known about the POD X3 / X3 Live
USB protocol. It exists so reverse-engineering effort isn't duplicated or
lost. **Every claim here is sourced** — update this file as new facts are
confirmed against real hardware, and mark speculation clearly as such.

## Device identification

| Field | Value | Source |
|---|---|---|
| USB Vendor ID | `0x0E41` (Line6) | andree182/podx3 |
| Product ID — POD X3 (rack/desktop) | `0x414A` | andree182/podx3 |
| Product ID — POD X3 Live | `0x414B` | andree182/podx3, confirmed via `lsusb` |
| Device/interface class | `0xFF` (vendor-specific) | andree182/podx3 — **not** USB Audio class, **not** USB MIDI class |

Line6's own KB lists POD X3 as USB 2.0 but explicitly *not* class-compliant.
This means the OS will not surface it as a standard audio or MIDI device —
no off-the-shelf MIDI/audio library can talk to it. It requires either
Line6's proprietary driver (for audio, via ASIO/CoreAudio integration) or
raw USB access (libusb/WinUSB/IOKit-style) for the control protocol we care
about here.

## USB interfaces

- **Interface 0** (alt setting 1): isochronous audio streaming.
  8 channels in, 2 channels out, S24_3LE. Not needed for patch/effect
  control — out of scope for this project unless we later add audio
  routing features.
- **Interface 1**: the control channel we care about.
  - Bulk IN endpoint: `0x81`
  - Bulk OUT endpoint: `0x01`
  - Max packet size: 64 bytes (`0x40`)

## Control handshake (before bulk I/O starts)

Uses standard USB **control transfers**, not the bulk endpoints:

- `bRequest = 0x67` (named `L6_X3_CTRL` in prior art)
- `bmRequestType = 0x40` (host→device) or `0xC0` (device→host)
- Used to read serial number and firmware version, and to send an
  "INIT" sequence of 8-byte reads at `wValue` `0xF000`–`0xF080`. **Purpose
  of the INIT sequence is unconfirmed** — device responds, but the
  significance of the returned bytes is unknown. Treat as "send it because
  the only known-working implementation does," not as understood protocol.

After this handshake, normal operation moves to the bulk endpoints.

## Bulk transfer framing

Every bulk packet (max 64 bytes) starts with a 4-byte header:

```
byte 0: ContentsLength
byte 1: ?? (unconfirmed)
byte 2: Flags — 0x01 = first packet of a message, 0x04 = continuation
byte 3: ?? (unconfirmed)
```

Multi-packet messages are reassembled by concatenating payloads across
packets until `ContentsLength` worth of data has been collected. (Reference
implementation: `PacketCompleter` in andree182/podx3.)

## Message types (payload byte 0, after the 4-byte framing header)

| Type | Meaning | Confirmed working? |
|---|---|---|
| `0x01` | **EffectDump** — full patch/effect-chain blob, ~4096 bytes | Partially — dump can be requested/received; internal byte layout is **not decoded** |
| `0x02` | **ConfigCmd** — includes "Request EffectDump(i)" / "Send EffectDump(i, data)", i.e. read/write a patch by slot index | Request/send framing known; full semantics unconfirmed |
| `0x04` / `0x05` | Int parameter get/set (12/16-byte int tuples) | Some cases confirmed |
| `0x06` | **Float parameter set** — 20-byte struct, trailing 4 bytes = little-endian IEEE-754 float, parameter index near byte 0x18 | **Confirmed working** (example below) |

### Confirmed example: float parameter set (e.g. tone volume)

```
1C 00 01 00 06 00 0A 40 01 03 00 15 01 00 00 00 00 00 03 00 01 00 00 00 05 00 10 3F <4-byte float LE>
```

Value range for this parameter is `0x00000000`–`0x3F800000` = `0.0`–`1.0`.
**Other parameters' index values and ranges are not yet catalogued** — this
is a concrete, tractable reverse-engineering task: enumerate parameter
indices by trial against real hardware and record them here as found.

## What's genuinely unknown

1. **EffectDump internal byte layout.** We can read/write the whole 4096-byte
   blob, but don't know which bytes encode which effect model, which
   parameter, or the patch name. This blocks "build a patch from scratch" /
   "rename a patch" style editing (it does *not* block backup/restore/swap,
   which only needs the blob to be opaque).
2. **Full parameter index catalogue.** Only tone volume is confirmed. Every
   other knob (amp model, cab model, per-effect parameters, EQ, etc.) needs
   its index and value encoding discovered.
3. **Whether MIDI CC / SysEx also works over the 5-pin DIN MIDI ports**,
   independent of USB. Line6 publishes an official MIDI CC chart for X3
   Live, but per `pod-ui` maintainer `arteme` (issue #70), full SysEx
   patch-dump/UDI support over MIDI on X3 is unconfirmed — he tested a unit
   whose MIDI port was dead. If DIN MIDI CC works, it's a much simpler path
   for live parameter tweaking (standard MIDI libraries apply), but won't
   give the fuller librarian features.

## Reverse-engineering methodology going forward

Since we don't have access to a working Gearbox install to capture its USB
traffic, the practical path is **black-box diffing against real hardware**:

1. Take an EffectDump of a patch.
2. Change exactly one thing on the unit's own front panel (e.g. tweak one
   knob, or rename the patch).
3. Take another EffectDump.
4. Diff the two blobs. The changed byte(s) are very likely the encoding for
   that control.
5. Record the finding in this document, with the exact steps and byte
   offset/encoding.

This requires the physical POD X3 connected via USB to a machine running the
probe tooling in `crates/pod-core` — no Gearbox or Windows VM needed for
this part. (A capture of real Gearbox traffic, if one ever becomes
available, would still be valuable to cross-check — see
[arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70), where a
community member has offered to attempt exactly that.)

## Sources

- [andree182/podx3](https://github.com/andree182/podx3) — GPLv2, primary
  source for everything in this document.
- [arteme/pod-ui#70](https://github.com/arteme/pod-ui/issues/70) — ongoing
  community discussion of X3 support; confirms no prior art exists for the
  EffectDump layout or a working USB capture.
- [Line6 USB compatibility KB](https://kb.line6.com/usb-compatibility-with-line-6-devices)
  — confirms X3 is not class-compliant.
- [Line6 MIDI Continuous Controller Reference](https://line6.com/data/l/0a06000f1344c45ecbcd6e0293/application/pdf/MIDI%20Continuous%20Controller%20Reference%20)
  — official CC chart; relevant only if DIN MIDI path is pursued.
