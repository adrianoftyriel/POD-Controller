// POD Controller web UI. Talks to `pod-cli serve` (API documented at the
// top of crates/pod-cli/src/serve.rs). Plain JS, no build step: edit and
// reload. Presentation lives in style.css; this file builds DOM with stable
// class names and data-* attributes (see designprompt.md).

'use strict';

// ---------------------------------------------------------------- constants

const BLOCK = { amp: 0, cab: 1, stomp: 2, mod: 3, delay: 4, reverb: 5, gate: 6,
  comp: 7, eq: 8, wah: 9, volume: 10, loop: 11 };
const PRE = 2, POST = 5;

// Gearbox's drawing order. Movable blocks appear in whichever of their two
// places matches their current group.
const CHAIN_ORDER = [
  ['gate'], ['volume', PRE], ['wah'], ['stomp'], ['mod', PRE], ['delay', PRE],
  ['reverb', PRE], ['loop', PRE], ['amp'], ['comp'], ['eq'], ['volume', POST],
  ['loop', POST], ['mod', POST], ['delay', POST], ['reverb', POST],
];

// Extra panels that aren't block records.
const EXTRA_PANELS = [{ key: 'variax', label: 'Variax' }];

const STOMP_FAMILIES = { 0: 'Dynamics', 5: 'Distortion', 10: 'Filter & Synth' };
const AMP_FAMILIES = { 2: 'Amps', 3: 'Model pack amps (03)', 4: 'Model pack amps (04)' };

const THEMES = [
  { id: 'mesa', name: '1 · Mesa' },
  { id: 'neon', name: '2 · Neon Stage' },
  { id: 'tweed', name: "3 · Tweed '59" },
  { id: 'marshall', name: '4 · Marshall' },
  { id: 'studio-dark', name: 'Studio Dark' },
  { id: 'daylight', name: 'Daylight' },
];

// How parameters are drawn: 'fader' (vertical, the default) or 'knob'.
// `all` is the base rule, `tone` overrides it for the amp tone controls, and
// `overrides` holds per-parameter choices made by press-and-hold.
const CONTROLS_KEY = 'pod-controls';
const TONE_CONTROLS = ['bass', 'middle', 'mid', 'treble', 'presence', 'tone'];
const CTRL = loadControls();

function loadControls() {
  let raw = null;
  try { raw = JSON.parse(localStorage.getItem(CONTROLS_KEY)); } catch (_) { /* ignore */ }
  return {
    all: raw?.all === 'knob' ? 'knob' : 'fader',
    tone: raw?.tone === 'fader' || raw?.tone === 'knob' ? raw.tone : 'inherit',
    overrides: (raw && typeof raw.overrides === 'object' && raw.overrides) || {},
  };
}

function saveControls() {
  try { localStorage.setItem(CONTROLS_KEY, JSON.stringify(CTRL)); } catch (_) { /* ignore */ }
}

function isToneControl(label) {
  return TONE_CONTROLS.includes(String(label).toLowerCase().trim());
}

/// Which control to draw for a parameter: per-parameter override, else the
/// tone rule (for tone controls), else the "all" rule.
function controlFor(blockKey, paramId, label) {
  const override = CTRL.overrides[`${blockKey}.${paramId}`];
  if (override === 'fader' || override === 'knob') return override;
  if (isToneControl(label)) return CTRL.tone === 'inherit' ? CTRL.all : CTRL.tone;
  return CTRL.all;
}

// ---------------------------------------------------------------- state

const S = {
  catalog: null,
  patches: [],          // [{slot, code, name}]
  banks: 16,
  bank: 1,              // bank shown in the strip (1-based)
  current: null,        // working copy from the server
  tone: 0,              // tone being edited
  panel: 'amp',         // selected block key or extra panel
  showHidden: false,
  modalMode: null,      // 'load' | 'save'
  connected: false,
};

// ---------------------------------------------------------------- helpers

const $ = (id) => document.getElementById(id);

function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'dataset') Object.assign(n.dataset, v);
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else if (v === true) n.setAttribute(k, '');
    else n.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    n.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return n;
}

function log(msg, isErr = false) {
  const l = $('log');
  l.textContent = msg;
  l.dataset.state = isErr ? 'error' : 'info';
}

async function api(path, params = {}) {
  const qs = new URLSearchParams(params).toString();
  const res = await fetch(`/api/${path}${qs ? '?' + qs : ''}`, { method: 'POST' });
  const data = await res.json();
  if ('current' in data) setCurrent(data.current);
  if (!data.ok) throw new Error(data.error || 'request failed');
  return data;
}

async function getJSON(path) {
  const res = await fetch(path, { cache: 'no-store' });
  return res.json();
}

const bankOf = (slot) => Math.floor(slot / 4) + 1;
const slotCode = (slot) => `${String(bankOf(slot)).padStart(2, '0')}${'ABCD'[slot % 4]}`;
const patchName = (slot) => S.patches[slot]?.name;

function tone() { return S.current?.tones[S.tone]; }
function record(key) { return tone()?.blocks[BLOCK[key]]; }

function catalogBlock(key) {
  return S.catalog.blocks.find((b) => b.key === key);
}

function modelDef(key, rec) {
  const b = catalogBlock(key);
  return b?.models.find((m) => m.category === rec.category && m.table === rec.table
    && m.model === rec.model);
}

function modelName(key, rec) {
  return modelDef(key, rec)?.name ?? `Model ${rec.table.toString(16)}/${rec.model}`;
}

// Throttle per key: send at most every `ms`, always sending the last value.
const throttles = new Map();
function throttled(key, ms, fn) {
  let t = throttles.get(key);
  if (!t) { t = { last: 0, timer: null, pending: null }; throttles.set(key, t); }
  t.pending = fn;
  const run = () => {
    t.last = Date.now(); t.timer = null;
    const f = t.pending; t.pending = null;
    f().catch((e) => log(e.message, true));
  };
  const wait = ms - (Date.now() - t.last);
  if (wait <= 0) run();
  else if (!t.timer) t.timer = setTimeout(run, wait);
}

// ---------------------------------------------------------------- fader

// A vertical fader. opts: {label, min, max, value, step, format, onInput,
// onChange, defaultValue}. Drag, wheel, arrow keys; double-click resets to
// defaultValue. Orientation comes from CSS (.fader[data-orient]).
function Fader(opts) {
  const node = $('fader-template').content.firstElementChild.cloneNode(true);
  const track = node.querySelector('[data-role=track]');
  const valueEl = node.querySelector('[data-role=value]');
  node.querySelector('[data-role=label]').textContent = opts.label;
  node.dataset.orient = opts.orient || 'vertical';
  const min = opts.min ?? 0, max = opts.max ?? 1;
  const step = opts.step ?? (max - min) / 1000;
  let value = opts.value ?? min;

  const fmt = opts.format || ((v) => v.toFixed(2));
  const clamp = (v) => Math.min(max, Math.max(min, v));
  const quant = (v) => (step >= 1 ? Math.round(v) : v);
  const paint = () => {
    const frac = max > min ? (value - min) / (max - min) : 0;
    node.style.setProperty('--fader-pos', frac);
    valueEl.textContent = fmt(value);
    node.setAttribute('aria-valuenow', value);
  };
  const set = (v, final) => {
    v = quant(clamp(v));
    if (v === value && !final) return;
    value = v; paint();
    opts.onInput?.(value);
    if (final) opts.onChange?.(value);
  };
  const fromPointer = (e) => {
    const r = track.getBoundingClientRect();
    const vertical = node.dataset.orient !== 'horizontal';
    const frac = vertical ? 1 - (e.clientY - r.top) / r.height : (e.clientX - r.left) / r.width;
    return min + Math.min(1, Math.max(0, frac)) * (max - min);
  };

  node.setAttribute('role', 'slider');
  node.setAttribute('aria-label', opts.label);
  node.setAttribute('aria-valuemin', min);
  node.setAttribute('aria-valuemax', max);
  track.addEventListener('pointerdown', (e) => {
    track.setPointerCapture(e.pointerId);
    node.classList.add('dragging');
    set(fromPointer(e));
    const move = (ev) => set(fromPointer(ev));
    const up = (ev) => {
      set(fromPointer(ev), true);
      node.classList.remove('dragging');
      track.removeEventListener('pointermove', move);
      track.removeEventListener('pointerup', up);
      track.removeEventListener('pointercancel', up);
    };
    track.addEventListener('pointermove', move);
    track.addEventListener('pointerup', up);
    track.addEventListener('pointercancel', up);
  });
  node.addEventListener('wheel', (e) => {
    e.preventDefault();
    const inc = Math.max(step, (max - min) / 100);
    set(value + (e.deltaY < 0 ? inc : -inc), true);
  }, { passive: false });
  node.addEventListener('keydown', (e) => {
    const inc = Math.max(step, (max - min) / (e.shiftKey ? 10 : 100));
    if (['ArrowUp', 'ArrowRight'].includes(e.key)) set(value + inc, true);
    else if (['ArrowDown', 'ArrowLeft'].includes(e.key)) set(value - inc, true);
    else return;
    e.preventDefault();
  });
  node.addEventListener('dblclick', () => {
    if (opts.defaultValue !== undefined) set(opts.defaultValue, true);
  });
  paint();
  if (opts.onLongPress) onLongPress(node, opts.onLongPress);
  return node;
}

// Press and hold (without moving) to run `handler`; cancelled by movement or
// release. Used to offer switching a control between fader and knob.
function onLongPress(node, handler) {
  let timer = null, sx = 0, sy = 0;
  const clear = () => { if (timer) { clearTimeout(timer); timer = null; } };
  node.addEventListener('pointerdown', (e) => {
    if (e.button) return;
    sx = e.clientX; sy = e.clientY;
    clear();
    timer = setTimeout(() => { timer = null; handler(); }, 550);
  });
  node.addEventListener('pointermove', (e) => {
    if (Math.hypot(e.clientX - sx, e.clientY - sy) > 8) clear();
  });
  ['pointerup', 'pointercancel', 'pointerleave', 'blur'].forEach((ev) =>
    node.addEventListener(ev, clear));
}

// A rotary knob with the same contract as Fader: drag (up/down), wheel,
// arrow keys, double-click reset, role="slider" and aria values. Drag moves
// vertically, so it feels like the fader it can replace.
function Knob(opts) {
  const node = el('div', { class: 'knob', tabindex: '0', role: 'slider' });
  const dial = el('div', { class: 'knob-dial' }, el('div', { class: 'knob-pointer' }));
  const valueEl = el('div', { class: 'knob-value' });
  node.append(valueEl, dial, el('div', { class: 'knob-label' }, opts.label));
  const min = opts.min ?? 0, max = opts.max ?? 1;
  const range = max - min || 1;
  const step = opts.step ?? range / 1000;
  let value = opts.value ?? min;
  const fmt = opts.format || ((v) => v.toFixed(2));
  const clamp = (v) => Math.min(max, Math.max(min, v));
  const quant = (v) => (step >= 1 ? Math.round(v) : v);
  const paint = () => {
    const frac = max > min ? (value - min) / (max - min) : 0;
    node.style.setProperty('--knob-pos', frac);
    node.style.setProperty('--knob-rot', `${-135 + frac * 270}deg`);
    valueEl.textContent = fmt(value);
    node.setAttribute('aria-valuenow', value);
    node.setAttribute('aria-valuetext', fmt(value));
  };
  const set = (v, final) => {
    v = quant(clamp(v));
    if (v === value && !final) return;
    value = v; paint();
    opts.onInput?.(value);
    if (final) opts.onChange?.(value);
  };

  node.setAttribute('aria-label', opts.label);
  node.setAttribute('aria-valuemin', min);
  node.setAttribute('aria-valuemax', max);
  dial.addEventListener('pointerdown', (e) => {
    dial.setPointerCapture(e.pointerId);
    node.classList.add('dragging');
    const startY = e.clientY, startVal = value;
    const at = (ev) => set(startVal + ((startY - ev.clientY) / 150) * range);
    const move = (ev) => at(ev);
    const up = (ev) => {
      at(ev);
      opts.onChange?.(value);
      node.classList.remove('dragging');
      dial.removeEventListener('pointermove', move);
      dial.removeEventListener('pointerup', up);
      dial.removeEventListener('pointercancel', up);
    };
    dial.addEventListener('pointermove', move);
    dial.addEventListener('pointerup', up);
    dial.addEventListener('pointercancel', up);
  });
  node.addEventListener('wheel', (e) => {
    e.preventDefault();
    const inc = Math.max(step, range / 100);
    set(value + (e.deltaY < 0 ? inc : -inc), true);
  }, { passive: false });
  node.addEventListener('keydown', (e) => {
    const inc = Math.max(step, range / (e.shiftKey ? 10 : 100));
    if (['ArrowUp', 'ArrowRight'].includes(e.key)) set(value + inc, true);
    else if (['ArrowDown', 'ArrowLeft'].includes(e.key)) set(value - inc, true);
    else return;
    e.preventDefault();
  });
  node.addEventListener('dblclick', () => {
    if (opts.defaultValue !== undefined) set(opts.defaultValue, true);
  });
  paint();
  if (opts.onLongPress) onLongPress(node, opts.onLongPress);
  return node;
}

// ---------------------------------------------------------------- top bar

function setCurrent(cur) {
  S.current = cur;
  renderTitle();
  renderBankStrip();
  renderChain();
}

function renderTitle() {
  const c = S.current;
  $('patch-code').textContent = c ? c.code : '—';
  $('patch-name').textContent = c ? (c.tones[0].name || '(unnamed)') : 'No patch loaded';
  $('dirty-dot').hidden = !c?.dirty;
  document.body.dataset.dirty = c?.dirty ? 'true' : 'false';
  for (const b of ['btn-save', 'btn-save-as', 'btn-revert']) $(b).disabled = !c;
  $('tone-name').textContent = c ? (c.tones[S.tone].name || '(unnamed)') : '—';
  document.querySelectorAll('.tone-tab').forEach((t) =>
    t.classList.toggle('active', Number(t.dataset.tone) === S.tone));
}

function startRename(buttonId, inputId, toneIdx) {
  if (!S.current) return;
  const btn = $(buttonId), input = $(inputId);
  input.value = S.current.tones[toneIdx].name;
  btn.hidden = true; input.hidden = false; input.focus(); input.select();
  const finish = async (commit) => {
    input.hidden = true; btn.hidden = false;
    input.onkeydown = input.onblur = null;
    const name = input.value.replace(/[^\x20-\x7e]/g, '').slice(0, 16);
    if (!commit || name === S.current.tones[toneIdx].name) return;
    try {
      await api('rename', { tone: toneIdx, name });
      log(`Renamed tone ${toneIdx + 1} to "${name}"`);
    } catch (e) { log('Rename failed: ' + e.message, true); }
  };
  input.onkeydown = (e) => {
    if (e.key === 'Enter') finish(true);
    if (e.key === 'Escape') finish(false);
  };
  input.onblur = () => finish(true);
}

function confirmDiscard() {
  return !S.current?.dirty
    || confirm(`${S.current.code} "${S.current.tones[0].name}" has unsaved changes. Discard them?`);
}

async function loadSlot(slot) {
  if (!confirmDiscard()) return;
  try {
    log(`Loading ${slotCode(slot)}…`);
    await api('load', { slot });
    S.bank = bankOf(slot);
    renderAll();
    log(`Loaded ${slotCode(slot)} "${patchName(slot) ?? ''}"`);
  } catch (e) { log('Load failed: ' + e.message, true); }
}

async function saveTo(slot) {
  const c = S.current;
  if (!c) return;
  if (slot !== c.slot) {
    const target = patchName(slot);
    const msg = `Save "${c.tones[0].name}" to ${slotCode(slot)}`
      + (target ? `, replacing "${target}"?` : '?');
    if (!confirm(msg)) return;
  }
  try {
    await api('save', { slot });
    await refreshPatches();
    log(`Saved to ${slotCode(slot)}`);
  } catch (e) { log('Save failed: ' + e.message, true); }
}

// ---------------------------------------------------------------- bank strip

function renderBankStrip() {
  $('bank-number').textContent = String(S.bank).padStart(2, '0');
  const box = $('bank-patches');
  box.replaceChildren(...[0, 1, 2, 3].map((ch) => {
    const slot = (S.bank - 1) * 4 + ch;
    const name = patchName(slot);
    return el('button', {
      class: 'patch-card',
      dataset: { slot, channel: 'ABCD'[ch], state: S.current?.slot === slot ? 'loaded' : '' },
      onclick: () => loadSlot(slot),
      title: `Load ${slotCode(slot)}`,
    },
    el('span', { class: 'patch-card-channel' }, 'ABCD'[ch]),
    el('span', { class: 'patch-card-name' }, name ?? '…'));
  }));
}

function setBank(b) {
  S.bank = ((b - 1 + S.banks) % S.banks) + 1;
  renderBankStrip();
}

// ---------------------------------------------------------------- all banks

function openBanks(mode) {
  S.modalMode = mode;
  $('banks-title').textContent = mode === 'save' ? 'Save to…' : 'All banks';
  $('banks-sub').textContent = mode === 'save'
    ? `Pick a slot for "${S.current?.tones[0].name ?? ''}"` : 'Click a patch to load it';
  $('banks-modal').dataset.mode = mode;
  $('banks-modal').hidden = false;
  renderBanksGrid();
}

function closeBanks() { $('banks-modal').hidden = true; S.modalMode = null; }

// ---------------------------------------------------------------- settings

function renderControlSettings() {
  document.querySelectorAll('#settings-modal .setting-group[data-setting]').forEach((group) => {
    const cur = group.dataset.setting === 'all' ? CTRL.all : CTRL.tone;
    group.querySelectorAll('.segmented button').forEach((b) => {
      const on = b.dataset.value === cur;
      b.classList.toggle('active', on);
      b.setAttribute('aria-checked', on ? 'true' : 'false');
    });
  });
  const n = Object.keys(CTRL.overrides).length;
  $('overrides-count').textContent = n ? `${n} parameter${n === 1 ? '' : 's'} set` : 'none';
}

function setControlSetting(name, value) {
  if (name === 'all') CTRL.all = value === 'knob' ? 'knob' : 'fader';
  else CTRL.tone = ['fader', 'knob', 'inherit'].includes(value) ? value : 'inherit';
  saveControls();
  renderControlSettings();
  renderPanel();
}

function openSettings() {
  $('settings-modal').hidden = false;
  renderControlSettings();
}
function closeSettings() { $('settings-modal').hidden = true; }

function renderBanksGrid() {
  if ($('banks-modal').hidden) return;
  const rows = [];
  for (let b = 1; b <= S.banks; b++) {
    rows.push(el('div', { class: 'banks-row', dataset: { bank: b } },
      el('span', { class: 'banks-row-number' }, String(b).padStart(2, '0')),
      ...[0, 1, 2, 3].map((ch) => {
        const slot = (b - 1) * 4 + ch;
        return el('button', {
          class: 'banks-cell',
          dataset: { slot, state: S.current?.slot === slot ? 'loaded' : '' },
          onclick: () => {
            const mode = S.modalMode;
            closeBanks();
            if (mode === 'save') saveTo(slot); else loadSlot(slot);
          },
        },
        el('span', { class: 'banks-cell-channel' }, 'ABCD'[ch]),
        el('span', { class: 'banks-cell-name' }, patchName(slot) ?? '…'));
      })));
  }
  $('banks-grid').replaceChildren(...rows);
}

// ---------------------------------------------------------------- chain

function chainEntries() {
  const t = tone();
  if (!t) return [];
  return CHAIN_ORDER.filter(([key, where]) => {
    if (where === undefined) return true;
    return t.blocks[BLOCK[key]].group === where;
  }).map(([key]) => key);
}

function renderChain() {
  const t = tone();
  const chain = $('chain');
  if (!t) { chain.replaceChildren(); return; }
  const items = chainEntries().map((key) => {
    const rec = t.blocks[BLOCK[key]];
    const cat = catalogBlock(key);
    return el('div', {
      class: 'chain-block',
      dataset: { block: key, enabled: rec.enabled, selected: S.panel === key,
        position: rec.group === PRE ? 'pre' : rec.group === POST ? 'post' : 'amp' },
    },
    el('button', { class: 'chain-select', onclick: () => selectPanel(key) },
      el('span', { class: 'chain-label' }, cat.label),
      el('span', { class: 'chain-model' }, modelName(key, rec))),
    el('button', {
      class: 'chain-power', title: rec.enabled ? 'Turn off' : 'Turn on',
      'aria-pressed': rec.enabled ? 'true' : 'false',
      onclick: () => setEnabled(key, !rec.enabled),
    }, rec.enabled ? 'ON' : 'OFF'));
  });
  const extras = EXTRA_PANELS.map((p) => el('div', {
    class: 'chain-block chain-extra', dataset: { block: p.key, selected: S.panel === p.key },
  }, el('button', { class: 'chain-select', onclick: () => selectPanel(p.key) },
    el('span', { class: 'chain-label' }, p.label),
    el('span', { class: 'chain-model' }, variaxSummary()))));
  chain.replaceChildren(
    el('div', { class: 'chain-blocks' }, items),
    el('div', { class: 'chain-extras' }, extras));
}

function variaxSummary() {
  const s = tone()?.settings;
  if (!s) return '';
  const opt = S.catalog.settings.find((x) => x.key === 'variax_model')
    .options.find((o) => o.value === s.variax_model);
  return opt ? opt.name : `User ${s.variax_model}`;
}

function selectPanel(key) {
  S.panel = key;
  renderChain();
  renderPanel();
}

async function setEnabled(key, on) {
  try {
    await api('enable', { tone: S.tone, block: BLOCK[key], on });
    renderPanel();
  } catch (e) { log(`${key} on/off failed: ` + e.message, true); }
}

// ---------------------------------------------------------------- panels

function renderPanel() {
  const panel = $('panel');
  panel.dataset.block = S.panel;
  if (!S.current) {
    panel.replaceChildren(el('div', { class: 'empty-hint' },
      'Load a patch from the bank strip above to start editing.'));
    return;
  }
  if (S.panel === 'variax') return panel.replaceChildren(...variaxPanel());
  panel.replaceChildren(...blockPanel(S.panel));
}

function modelSelect(key, rec, families) {
  const cat = catalogBlock(key);
  const sel = el('select', { class: 'model-select', dataset: { block: key } });
  const groups = new Map();
  for (const m of cat.models) {
    const fam = families ? (families[m.table] ?? `Table ${m.table}`) : '';
    if (!groups.has(fam)) groups.set(fam, []);
    groups.get(fam).push(m);
  }
  let found = false;
  for (const [fam, models] of groups) {
    const parent = fam ? el('optgroup', { label: fam }) : sel;
    for (const m of models) {
      const selected = m.category === rec.category && m.table === rec.table && m.model === rec.model;
      found ||= selected;
      parent.append(el('option', {
        value: `${m.category}/${m.table}/${m.model}`, selected,
      }, m.name + (m.params_known ? '' : ' *')));
    }
    if (fam) sel.append(parent);
  }
  if (!found) {
    sel.prepend(el('option', { value: `${rec.category}/${rec.table}/${rec.model}`, selected: true },
      modelName(key, rec)));
  }
  sel.addEventListener('change', () => {
    const [category, table, model] = sel.value.split('/').map(Number);
    changeModel(key, { category, table, model });
  });
  return sel;
}

async function changeModel(key, target) {
  const rec = record(key);
  const def = catalogBlock(key).models.find((m) => m.category === target.category
    && m.table === target.table && m.model === target.model);
  // Keep values the new model shares with the old one; defaults otherwise.
  // Models with no known parameter list keep the block's current params.
  const current = new Map(rec.params.map((p) => [p.id, p.value]));
  const params = (def?.params ?? []).map((p) => `${p.id}:${current.get(p.id) ?? p.default}`);
  try {
    await api('model', { tone: S.tone, block: BLOCK[key], ...target, params: params.join(',') });
    renderPanel();
    log(`${catalogBlock(key).label}: ${def?.name ?? 'model changed'}`);
  } catch (e) { log('Model change failed: ' + e.message, true); }
}

// Fader order: normal knobs, real-unit controls, Mix, then unnamed 3F20s,
// each by index.
const NAMESPACE_ORDER = { '3f10': 0, '3f00': 1, '3f01': 2, '3f20': 3 };
function paramRank(id) {
  return (NAMESPACE_ORDER[id.slice(0, 4)] ?? 4) * 0x10000 + parseInt(id.slice(4), 16);
}

// Owner's preferred parameter order for amps and stomp boxes; anything not
// listed follows, in the normal rank order. Matched by name, case-insensitive
// (Middle/Mid and Presense/Presence are treated as the same).
const PARAM_ORDER = [
  'drive', 'amount', 'level', 'threshold', 'sustain', 'ramp',
  'bass', 'mid', 'mid freq', 'treble', 'presence', 'frequency',
  'tone', 'wave', 'filter', 'decay', '-1 oct', '-2 oct', 'interval',
  'sensitivity', 'q', 'gain', 'volume',
];
const PARAM_ORDER_INDEX = new Map(PARAM_ORDER.map((n, i) => [n, i]));

function canonicalParamName(name) {
  const n = String(name).toLowerCase()
    .replace(/[\u2212\u2013\u2014]/g, '-')
    .replace(/\s+/g, ' ')
    .replace(/\.$/, '')
    .trim();
  if (n === 'middle' || n === 'mid') return 'mid';
  if (n === 'presense') return 'presence';
  if (n === 'mid frequency') return 'mid freq';
  return n;
}

/// Sort amp/stomp parameters into the owner's order, keeping unlisted
/// parameters after them in rank order.
function sortByParamOrder(list) {
  return list.slice().sort((a, b) => {
    const ra = PARAM_ORDER_INDEX.get(canonicalParamName(a.d?.name ?? '')) ?? Infinity;
    const rb = PARAM_ORDER_INDEX.get(canonicalParamName(b.d?.name ?? '')) ?? Infinity;
    if (ra !== rb) return ra < rb ? -1 : 1;
    return paramRank(a.p.id) - paramRank(b.p.id);
  });
}

// Press-and-hold a parameter control to ask whether to show it the other way.
function promptControlSwitch(key, id, label, current) {
  const target = current === 'fader' ? 'knob' : 'fader';
  const what = target === 'knob' ? 'a rotary knob' : 'a vertical fader';
  const also = target === 'knob' ? 'fader' : 'knob';
  if (!confirm(`Show "${label}" as ${what} instead of a ${also}?`)) return;
  CTRL.overrides[`${key}.${id}`] = target;
  saveControls();
  renderPanel();
  log(`${label}: now a ${target}`);
}

function paramControls(key, rec) {
  const def = modelDef(key, rec);
  const byId = new Map((def?.params ?? []).map((p) => [p.id, p]));
  const shown = rec.params
    .map((p) => ({ p, d: byId.get(p.id) }))
    .filter(({ p, d }) => S.showHidden || !(d?.hidden ?? p.id.startsWith('3f20')));
  const ordered = key === 'amp' || key === 'stomp'
    ? sortByParamOrder(shown)
    : shown.sort((a, b) => paramRank(a.p.id) - paramRank(b.p.id));
  return ordered.map(({ p, d }) => {
    const min = Math.min(d?.min ?? 0, p.value), max = Math.max(d?.max ?? 1, p.value);
    const unit = d?.unit ?? '';
    const normalized = min === 0 && max === 1 && !unit;
    const label = d?.name ?? `Param ${p.id}`;
    const common = {
      label, min, max, value: p.value, defaultValue: d?.default,
      format: normalized ? (v) => Math.round(v * 100) : (v) => `${v.toFixed(1)}${unit ? ' ' + unit : ''}`,
      onInput: (v) => throttled(`p${S.tone}/${key}/${p.id}`, 40,
        () => api('param', { tone: S.tone, block: BLOCK[key], id: p.id, value: v })),
    };
    const kind = controlFor(key, p.id, label);
    const onLongPress = () => promptControlSwitch(key, p.id, label, kind);
    return kind === 'knob'
      ? Knob({ ...common, onLongPress })
      : Fader({ ...common, onLongPress });
  });
}

function settingControl(key) {
  const def = S.catalog.settings.find((s) => s.key === key);
  const value = tone().settings[key];
  const send = (v) => api('setting', { tone: S.tone, key, value: v })
    .catch((e) => log(`${def.label} failed: ` + e.message, true));
  if (def.type === 'enum') {
    const sel = el('select', { class: 'setting-select', dataset: { setting: key } });
    let found = false;
    for (const o of def.options) {
      found ||= o.value === value;
      sel.append(el('option', { value: o.value, selected: o.value === value }, o.name));
    }
    if (!found) sel.prepend(el('option', { value, selected: true }, `User ${value}`));
    sel.addEventListener('change', () => send(Number(sel.value)).then(renderChain));
    return el('label', { class: 'setting', dataset: { setting: key } },
      el('span', { class: 'setting-label' }, def.label), sel);
  }
  const isInt = def.type === 'int';
  const common = {
    label: def.label, min: def.min, max: def.max, value, step: isInt ? 1 : undefined,
    format: isInt ? (v) => String(v) : (v) => Math.round(v * 100),
    onInput: (v) => throttled(`s${S.tone}/${key}`, 40, () => send(v)),
  };
  return controlFor('setting', key, def.label) === 'knob' ? Knob(common) : Fader(common);
}

// Menus go in a row above the faders.
function settingsGroup(keys) {
  const controls = keys.map(settingControl);
  const menus = controls.filter((c) => c.classList.contains('setting'));
  const faders = controls.filter((c) => !c.classList.contains('setting'));
  return [
    menus.length ? el('div', { class: 'setting-row' }, menus) : null,
    faders.length ? el('div', { class: 'fader-bank settings' }, faders) : null,
  ].filter(Boolean);
}

function positionToggle(key, rec) {
  const pos = catalogBlock(key).positions;
  if (!pos) return null;
  const [pre, post] = pos;
  const isPre = rec.group === PRE;
  const move = async (to) => {
    try {
      await api('move', { tone: S.tone, block: BLOCK[key], slot: to[0], group: to[1] });
      renderPanel();
    } catch (e) { log('Move failed: ' + e.message, true); }
  };
  return el('div', { class: 'position-toggle', role: 'group', 'aria-label': 'Position' },
    el('button', { 'aria-pressed': isPre ? 'true' : 'false', onclick: () => !isPre && move(pre) }, 'Pre'),
    el('button', { 'aria-pressed': isPre ? 'false' : 'true', onclick: () => isPre && move(post) }, 'Post'));
}

function blockPanel(key) {
  const rec = record(key);
  const cat = catalogBlock(key);
  const families = key === 'stomp' ? STOMP_FAMILIES : key === 'amp' ? AMP_FAMILIES : null;
  const head = el('div', { class: 'panel-head' },
    el('h2', { class: 'panel-title' }, cat.label),
    cat.models.length > 1 ? modelSelect(key, rec, families)
      : el('span', { class: 'panel-model' }, modelName(key, rec)),
    positionToggle(key, rec),
    el('button', {
      class: 'panel-power', 'aria-pressed': rec.enabled ? 'true' : 'false',
      onclick: () => setEnabled(key, !rec.enabled),
    }, rec.enabled ? 'ON' : 'OFF'),
    el('label', { class: 'show-hidden' },
      el('input', { type: 'checkbox', checked: S.showHidden,
        onchange: (e) => { S.showHidden = e.target.checked; renderPanel(); } }),
      'show hidden params'));
  const sections = [el('div', { class: 'fader-bank', dataset: { block: key } }, paramControls(key, rec))];
  if (!modelDef(key, rec)?.params_known) {
    sections.push(el('p', { class: 'panel-note' },
      'Knob names for this model are not known yet; the faders are its stored parameters.'));
  }
  if (key === 'amp') {
    const cab = record('cab');
    sections.push(el('div', { class: 'panel-sub', dataset: { block: 'cab' } },
      el('h3', { class: 'panel-subtitle' }, 'Cab'),
      el('div', { class: 'setting-row' },
        el('label', { class: 'setting', dataset: { setting: 'cab-model' } },
          el('span', { class: 'setting-label' }, 'Cab model'),
          modelSelect('cab', cab, { 2: 'Cabinets', 3: 'Cabinets (03)' })),
        ...settingsGroup(['mic']).flatMap((n) => [...n.children])),
      ...settingsGroup(['room'])));
  }
  return [head, ...sections];
}

function variaxPanel() {
  return [
    el('div', { class: 'panel-head' }, el('h2', { class: 'panel-title' }, 'Variax')),
    ...settingsGroup(['variax_model', 'variax_tone']),
    el('p', { class: 'panel-note' },
      'Model and tone the POD sends to a connected Variax when this tone loads.'),
  ];
}

// ---------------------------------------------------------------- polling

async function refreshPatches() {
  const p = await getJSON('/api/patches');
  S.patches = p.patches;
  S.banks = p.banks;
  renderBankStrip();
  renderBanksGrid();
}

async function pollState() {
  try {
    const st = await getJSON('/api/state');
    S.connected = st.connected;
    const status = $('status');
    status.dataset.state = st.connected ? (st.error ? 'warn' : 'ok') : 'error';
    $('status-text').textContent = st.connected
      ? `POD X3${st.device.live ? ' Live' : ''}` + (st.error ? ` · ${st.error}` : '')
      : 'POD not connected';
    const sp = $('scan-progress');
    sp.hidden = !st.scan.scanning;
    sp.textContent = `Reading patch names… ${st.scan.scanned}/${st.scan.total}`;
    sp.style.setProperty('--scan', st.scan.scanned / st.scan.total);
    if (st.scan.scanning || S.patches.some((p) => p.name === null)) await refreshPatches();
    // Someone else (e.g. an MCP client) changed the working copy: re-render,
    // unless the user is mid-drag or mid-rename.
    const busy = document.querySelector('.fader.dragging, input:not([hidden]):focus');
    if (S.rev !== undefined && st.rev !== S.rev && !busy) {
      S.rev = st.rev;
      setCurrent(st.current);
      if (st.current) S.bank = S.bank || bankOf(st.current.slot);
      renderPanel();
      await refreshPatches();
    } else if (S.rev === undefined) {
      S.rev = st.rev;
    }
    return st;
  } catch (e) {
    $('status').dataset.state = 'error';
    $('status-text').textContent = 'server unreachable';
  }
}

// ---------------------------------------------------------------- wiring

function renderAll() {
  renderTitle();
  renderBankStrip();
  renderChain();
  renderPanel();
}

function applyTheme(id, persist = true) {
  document.documentElement.dataset.theme = id;
  if (persist) {
    try { localStorage.setItem('pod-theme', id); } catch (_) { /* private mode */ }
  }
}

function wire() {
  $('bank-prev').onclick = () => setBank(S.bank - 1);
  $('bank-next').onclick = () => setBank(S.bank + 1);
  $('bank-strip').addEventListener('wheel', (e) => {
    if (Math.abs(e.deltaY) < 4 && Math.abs(e.deltaX) < 4) return;
    e.preventDefault();
    setBank(S.bank + ((e.deltaY || e.deltaX) > 0 ? 1 : -1));
  }, { passive: false });
  document.addEventListener('keydown', (e) => {
    if (e.target.closest('input, select, textarea, [role=slider]')) return;
    if (e.key === 'Escape' && !$('settings-modal').hidden) closeSettings();
    else if (e.key === 'Escape' && !$('banks-modal').hidden) closeBanks();
    else if (e.key === 'ArrowLeft' || e.key === 'PageUp') setBank(S.bank - 1);
    else if (e.key === 'ArrowRight' || e.key === 'PageDown') setBank(S.bank + 1);
  });
  $('btn-all-banks').onclick = () => openBanks('load');
  $('btn-save-as').onclick = () => openBanks('save');
  $('btn-save').onclick = () => S.current && saveTo(S.current.slot);
  $('btn-revert').onclick = async () => {
    if (!confirmDiscard()) return;
    try { await api('revert'); renderAll(); log('Reverted'); }
    catch (e) { log('Revert failed: ' + e.message, true); }
  };
  $('banks-close').onclick = closeBanks;
  $('banks-modal').addEventListener('click', (e) => { if (e.target.id === 'banks-modal') closeBanks(); });
  $('btn-settings').onclick = openSettings;
  $('settings-close').onclick = closeSettings;
  $('settings-modal').addEventListener('click', (e) => {
    if (e.target.id === 'settings-modal') closeSettings();
    const b = e.target.closest('.segmented button');
    if (b) setControlSetting(b.closest('.setting-group').dataset.setting, b.dataset.value);
  });
  $('btn-clear-overrides').onclick = () => {
    CTRL.overrides = {};
    saveControls();
    renderControlSettings();
    renderPanel();
    log('Cleared per-parameter overrides');
  };
  $('patch-name').onclick = () => startRename('patch-name', 'patch-name-edit', 0);
  $('tone-name').onclick = () => startRename('tone-name', 'tone-name-edit', S.tone);
  document.querySelectorAll('.tone-tab').forEach((t) => {
    t.onclick = async () => {
      S.tone = Number(t.dataset.tone);
      renderAll();
      if (S.current) api('tone', { tone: S.tone }).catch((e) => log(e.message, true));
    };
  });
  const ts = $('theme-select');
  for (const t of THEMES) ts.append(el('option', { value: t.id }, t.name));
  let saved = null;
  try { saved = localStorage.getItem('pod-theme'); } catch (_) { /* ignore */ }
  // ?theme=<id> forces a theme for previews (e.g. /themes.html) without
  // changing the browser's remembered choice.
  const forced = new URLSearchParams(location.search).get('theme');
  const forcedValid = THEMES.some((t) => t.id === forced);
  ts.value = forcedValid ? forced : (THEMES.some((t) => t.id === saved) ? saved : THEMES[0].id);
  applyTheme(ts.value, !forcedValid);
  ts.onchange = () => applyTheme(ts.value);
  window.addEventListener('beforeunload', (e) => {
    if (S.current?.dirty) { e.preventDefault(); e.returnValue = ''; }
  });
}

async function main() {
  wire();
  S.catalog = await getJSON('catalog.json');
  const st = await pollState();
  const urlBank = Number(new URLSearchParams(location.search).get('bank'));
  S.bank = urlBank || st?.current && bankOf(st.current.slot) || st?.initial_bank || 1;
  if (st?.current) S.current = st.current;
  await refreshPatches();
  renderAll();
  setInterval(pollState, 2000);
}

main();
