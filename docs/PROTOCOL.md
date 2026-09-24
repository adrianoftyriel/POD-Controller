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

## Confirmed from real Gearbox traffic (2026-09-24)

Captured with `tools/vm-capture/` (Gearbox on a Windows 7 VM, POD X3 Live
passed through by device, host-side `usbmon`). Each finding comes from a
single scripted UI action with its own capture in `captures/`. Offsets
below are into the **message** (after the 4-byte bulk framing header)
unless marked "packet".

### Common message header

```
+0  type        01 EffectDump, 02 ConfigCmd, 04 int, 05 ?, 06 float
+1  ??          00, except 02 on per-tone dump pushes and 04 on the EffectDump reply
+2  route (4)   host->POD: 0A 40 <ch> 03    POD->host: 0A 03 <ch> 40
+6  00
+7  subcommand  (table below)
+8  tone        00 = Tone 1, 01 = Tone 2 (for per-tone messages)
```

`<ch>` is `01` for live edits and `02` for patch-memory traffic (slot
select, dump push/read). Replies swap the two halves of the route, so these
bytes look like source/destination addresses.

| Type | Sub | Direction | Meaning |
|---|---|---|---|
| `04` | `13` | host->POD | int parameter set |
| `06` | `15` | host->POD | float parameter set |
| `05` | `16` | host->POD | sent in pairs on tone switch, value 0/1 per tone (focus flags?) |
| `04` | `20` | host->POD | select tone for editing (value = tone index) |
| `02` | `21` | host->POD | query (arg `03`, `07` seen, meaning unknown) |
| `04` | `22` | POD->host | answer to `21` |
| `02` | `00` | host->POD | **request EffectDump**, u32 arg = slot |
| `01` | `01` | POD->host | **EffectDump** reply (4104 bytes) |
| `02` | `04` | host->POD | push one tone's 2048-byte block into the edit buffer |
| `02` | `27` | host->POD | select patch slot, u32 arg = slot |
| `02` | `03` | POD->host | ack for `04` pushes |

Slot numbering is `(bank-1)*4 + channel`, with A=0..D=3 (5A = `0x10`).

Setting a parameter is **fire-and-forget**: one bulk OUT per value change,
with no reply. Knob drags stream one float set per mouse step.

### Int set (`04`/`13`): block on/off

```
04 00 0A 40 01 03 00 13 <tone> 00 00 00 <slot u16> <group u16> <u32 LE value>
```

| Block | slot | group |
|---|---|---|
| Gate | `00` | `02` |
| Wah | `02` | `02` |
| Stomp | `03` | `02` |
| Amp (sends slot `00` and `01`, amp+cab?) | `00`,`01` | `03` |
| EQ | `04` | `03` |
| Comp | `00` | `05` |
| Mod | `03` | `05` |
| Delay | `04` | `05` |
| Reverb | `05` | `05` |

Values: `0` = off, `1` = on. VOL has no on/off switch in the UI.

### Float set (`06`/`15`): amp knobs

```
06 00 0A 40 01 03 00 15 <tone> 00 00 00 <slot u16> <group u16> 01 00 00 00 <idx u16> 10 3F <f32 LE>
```

The amp knobs are slot `00`, group `03`:

| Knob | `idx` |
|---|---|
| Bass | `00` |
| Middle | `01` |
| Treble | `02` |
| Drive | `03` |
| Presence | `04` |
| Volume | `05` (matches the example above) |

Values are 0.0-1.0. `<tone>` is confirmed: the same Drive drag on Tone 2
sends `01` there (so the earlier example with `01` was a Tone 2 edit). The
`01 00 00 00` and `10 3F` fields are still unknown. Inside the EffectDump,
parameters are stored as `<idx u16> 10 3F <f32>` too (see below).

### EffectDump: layout at the top level (confirmed)

`02`/`00` with slot `0x10` got a 4104-byte `01`/`01` reply: an 8-byte
header, then 4096 bytes. **Those 4096 bytes are exactly Tone 1's 2048-byte
block followed by Tone 2's 2048-byte block.** They match, byte for byte,
the two `02`/`04` pushes Gearbox sends when it loads the patch (12-byte
header + 2048).

Offsets inside a 2048-byte tone block:

| Offset | Field | Evidence |
|---|---|---|
| `0x000` | Tone name, ASCII, space-padded (16 bytes?) | "Sweetly Broken", "Misfit Toys-FX" |
| `0x0E4` | Amp model ID (u8) | Line 6 Class A `06`, 1953 Small Tweed `11`, 1958 Tweed B-Man `12`, 1965 Double Verb `15` (internal IDs, not menu order) |
| `0x0F4`-`0x11F` | Amp knobs as six `<idx> 10 3F <f32>` records | reset to per-model defaults when the amp model changes |
| `0x170` | Cab model ID (u8) | 1x12 Line 6 `05`, 1x12 1953 Small Tweed `06`, 1x12 1964 Blackface 'Lux `07`, 2x12 1965 Blackface `0B`, 4x10 1958 Tweed B-Man `10`. Guitar cabs look like 1-based menu order |

Changing the amp or cab model does **not** use a parameter set: Gearbox
re-pushes the whole tone block (`02`/`04`), and the POD acks it with
`02`/`03`. That is the write path for anything the int/float sets can't
reach.

### Patch load / read sequences

- **Load a patch** (double-click in the Hardware Memory window): `27`
  (select slot), push Tone 1 block, push Tone 2 block, `20` (select
  Tone 1), then `21` queries. Gearbox pushes its **cached** copy; nothing
  is read from the POD.
- **GET SELECTED**: `00` (request dump, slot) -> 4104-byte `01` reply,
  then the same load sequence as above using the fresh data.

Continuation packets of these multi-packet messages have flags `04` at
packet offset 2, and a byte at packet offset 1 that varies with no
obvious pattern (`42`, `53`, `0F`, `F7`, ...), which is still unknown.

## What's genuinely unknown

1. **EffectDump internal byte layout, beyond the fields above.** The
   top-level split (two 2048-byte tone blocks), the tone name, the amp and
   cab model IDs, and the amp knobs are known. The effect-block models and
   parameters are not yet mapped. This blocks "build a patch from scratch" /
   "rename a patch" style editing (it does *not* block backup/restore/swap,
   which only needs the blob to be opaque).
2. **Full parameter index catalogue.** Amp knobs and block on/off are
   confirmed (above). Every
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

Gearbox traffic can now be captured (see above and `tools/vm-capture/`),
which is the fastest way to map live parameters. For the EffectDump layout,
**black-box diffing against real hardware** still applies:

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
