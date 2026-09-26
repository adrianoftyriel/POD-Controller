# Design brief: the POD Controller web UI

You are the design agent for POD Controller, an open-source replacement for
Line 6's Gearbox editor for the **POD X3 Live** guitar processor. The editor
already works end to end against real hardware. Your job is how it looks and
feels. You have **full freedom over layout, faders, knobs, images,
orientation and themes**, within the few constraints below.

## Hard constraints (from the owner)

1. **Faders are vertical**, never horizontal.
2. **Faders win over knobs.** Parameters are shown as vertical faders.
   Don't swap them for rotary knobs unless the owner asks for it.
3. **Patches are laid out horizontally**: the four patches of a bank sit
   side by side in a row, not stacked in a list. The same goes for each
   bank's row in the all-banks browser.
4. **Every function below keeps working** (see "Must keep working").

Everything else is yours: the overall layout, panel arrangement, the signal
chain's look, typography, colour, spacing, fader styling (caps, scales,
LEDs, value readouts), imagery (amp faces, pedal art, textures, logos),
icons, motion, portrait versus landscape and tablet layouts, and the theme
set.

## What the app does

- **Bank strip:** shows one bank (4 patches, A-D). Change banks with ◀ ▶,
  the mouse wheel over the strip, or ←/→ and PageUp/PageDown. Clicking a
  patch loads it into the POD.
- **All banks:** a pop-up grid of all 16 banks × 4 patches, like Gearbox's
  Hardware Memory window. It has two modes: *load* (click to load) and
  *save* (click to choose where the current patch is written, with an
  overwrite confirmation).
- **Patch title:** the slot code (e.g. `08C`) plus the patch name. Click the
  name to rename it (Tone 1's name, at most 16 printable ASCII characters).
  A dot marks unsaved changes. Buttons: **Save** (back to its own slot),
  **Save to…** (opens All banks in save mode), **Revert**.
- **Tone 1 / Tone 2:** every patch has two tones, each with its own name and
  full chain. The tabs pick which one is edited (and tell the POD).
- **Signal chain:** the tone's blocks in Gearbox's order, each with an
  on/off switch: Gate, Wah, Stomp, Amp (Amp + Cab together), Comp, EQ, plus
  Volume, FX Loop, Mod, Delay and Reverb, which each sit either before or
  after the amp. Click a block to open its panel. Variax is an extra panel
  at the end.
- **Block panel:**
  - a model menu (stomp and amp menus are grouped by family);
  - on/off;
  - Pre/Post for the five movable blocks;
  - one vertical fader per parameter, with double-click to reset, wheel
    and arrow-key support, and a "show hidden params" toggle;
  - the Amp panel also holds Cab model, Mic and Room.
- **Variax panel:** Variax model (a menu) and Variax tone (0-127).
- **Status:** device connection, and scan progress while the server reads
  all 64 patch names at start-up (about 10-20 s).
- **Themes:** a theme menu. The choice is remembered per browser.

Every edit is live: the POD changes sound straight away. Nothing is written
to the POD's memory until Save.

## Where things are

```
crates/pod-cli/ui/        the whole UI: static files, no build step
  index.html              structure: ids and data-role hooks the JS needs
  style.css               all visuals; theme tokens at the top
  app.js                  logic + DOM building (plain JS, no framework)
  catalog.json            generated model/parameter catalog (don't hand-edit)
crates/pod-cli/src/serve.rs   the HTTP server + JSON API (API table at the top)
tools/catalog/build_catalog.py  regenerates catalog.json from docs/
tools/ui-check/check.js   headless screenshots of every panel, desktop + phone
```

Add images, fonts and other assets under `crates/pod-cli/ui/` (e.g.
`ui/assets/`), and reference them with relative paths. The server serves
anything in that directory. The binary also carries a built-in copy of the
four core files for running without the source tree. If the UI can't work
without a new file, add it to `EMBEDDED` in `serve.rs`. External resources
(CDN fonts etc.) are allowed, but the UI must still work offline, with
system-font fallbacks.

## Running it

The dev server runs in LXC 114 (`dev2`, 172.16.88.33) with the POD
attached:

```
cd ~devadmin/POD-Controller
cargo build
./target/debug/pod-cli serve --port 8080 --bank 8     # http://172.16.88.33:8080/
```

`serve` reads the UI from `crates/pod-cli/ui` on each request, so **edits to
HTML/CSS/JS show up on a browser reload**, with no rebuild or restart. Only
one process can own the POD, so stop a running server before starting
another (`pkill -f "^./target/debug/pod-cli serve"`). `?bank=N` in the URL
opens a given bank.

Check your work with the screenshot script. It needs Playwright, and there
is already an install at `/tmp/pw` in dev2:

```
NODE_PATH=/tmp/pw/node_modules node tools/ui-check/check.js http://localhost:8080 /tmp/pw/out
```

It loads 08C, opens every panel and the all-banks grid at desktop and phone
sizes, saves PNGs and fails on any page error. Look at the PNGs.

## Rules for testing on the real POD

- **Bank 8 (08A-08D) is the scratch bank.** Load and tweak anything, but only
  *save* into bank 8, and only after backing the slot up
  (`pod-cli dump --slot N --out file`; restore with `pod-cli restore`).
  The other 60 patches are the owner's. Never overwrite them.
- Loading a patch or moving a fader changes the sound immediately. That's
  expected.
- If the POD stops answering, the server reconnects by itself. A real
  lock-up is rare now, and the owner can power-cycle it.

## How the code is put together (what you can change freely)

`app.js` is split into sections: constants, state (`S`), helpers, the
`Fader` component, top bar, bank strip, all banks, chain, panels, polling
and wiring. The render functions build the DOM with `el(tag, attrs,
...children)`. **You may restructure the markup, rewrite the render
functions and replace `Fader`** (canvas, SVG, images, a different
component), as long as the behaviour contract below holds. Keep it
framework-free, or bring in a small library with no build step, and say
why.

Useful hooks the CSS already uses:

- `[data-theme]` on `<html>`; add a theme to `THEMES` in `app.js` and a
  `[data-theme="..."]` token block in `style.css`.
- `.fader` with `--fader-pos` (0-1) set inline, `data-orient`, `.dragging`;
  parts `[data-role=track|fill|thumb|value|label]`.
- `.chain-block[data-block=<key>][data-enabled][data-selected][data-position=pre|amp|post]`.
- `.patch-card[data-slot][data-channel][data-state=loaded]`,
  `.banks-cell[data-state=loaded]`, `#banks-modal[data-mode=load|save]`.
- `#panel[data-block=<key>]`, so each block's panel can get its own look
  (e.g. an amp faceplate).
- `body[data-dirty]`, `.status[data-state=ok|warn|error]`,
  `.log[data-state=error]`.
- Block keys: `amp cab stomp mod delay reverb gate comp eq wah volume loop`,
  plus the extra panel `variax`.

## Must keep working (behaviour contract)

- All 64 patches reachable: the bank strip (buttons, wheel, keys) and the
  all-banks pop-up.
- Load, rename (both tones), Save, Save to any slot with an overwrite
  confirmation, Revert, and a "discard unsaved changes?" confirmation
  before loading over a dirty patch or closing the page.
- The tone switch; the chain order and on/off per block; the pre/post move
  for Volume, FX Loop, Mod, Delay and Reverb.
- Per block: the model menu and a fader for every stored parameter, labelled
  from the catalog. Value display: 0-100 for normal knobs, real units (e.g.
  dB) where the catalog gives a unit. Hidden parameters behind a toggle.
- The Amp panel's Cab model, Mic and Room; the Variax model and tone.
- Faders must support drag (mouse and touch), wheel, arrow keys and
  double-click reset, and stay keyboard-focusable with `role="slider"` and
  the aria values.
- While a fader is dragged, sends stay throttled (`throttled(..., 40, ...)`
  in `app.js`). Don't send one request per pixel.
- Errors show in the log line. The status shows the connection and scan
  progress.

## Data you get

`GET /api/state` → `{connected, device, error, initial_bank, scan:{scanned,
total, scanning}, current}`. `current` (also returned by every POST) is the
working copy:

```
{ slot, code: "08C", dirty,
  tones: [ { name,
             settings: { variax_model, variax_tone, mic, room, input },
             blocks: [ { index, key, category, table, model, slot, group,
                         enabled, sync, params: [ {id: "3f100001", value} ] } ×12 ] } ×2 ] }
```

`GET /api/patches` → `{total: 64, banks: 16, patches: [{slot, code, name}]}`
(`name` is `null` until scanned).

`catalog.json` → `blocks[]` (`index, key, label, positions`, and `models[]`
with `category, table, model, name, params_known`, and `params[]` with `id,
name, min, max, unit, default, hidden`), and `settings[]` (`key, label,
section, type: enum|int|float, options|min/max`). A model's `name` may be a
placeholder such as "Amp pack 04 #5" (Line 6 model-pack amps nobody has
named yet), and models marked `params_known: false` (shown with `*`) have no
parameter list. Design for those honestly; don't hide them.

The full POST API (load, save, rename, tone, param, enable, model, move,
setting, revert, rescan) is documented at the top of
`crates/pod-cli/src/serve.rs`. You shouldn't need to change it. If a design
needs something the API doesn't give (e.g. per-model images, which would be
a catalog field), add it and say so.

## Limits to design around

- The POD can't report its edit buffer, so knob turns on the unit's own
  front panel don't show up in the UI. Show state as "what this editor last
  sent", not as a live meter.
- There's no audio level data. Don't design meters.
- Tone names are at most 16 ASCII characters; slot codes are `01A`-`16D`.
- Parameter names come from Gearbox's menus, and some are generic ("Param 4",
  "Level 1").

## Deliverables

1. The restyled UI in `crates/pod-cli/ui/` (plus any assets and themes).
2. At least two themes, one dark and one light, switchable in the app.
3. A layout that works on desktop, tablet and phone (portrait and
   landscape).
4. Screenshots from `tools/ui-check/check.js` showing every panel, with no
   page errors.
5. A short note (commit message or `crates/pod-cli/ui/DESIGN.md`) on the
   design decisions and how to add a theme.
