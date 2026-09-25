//! A tiny hand-rolled HTTP server for watching and live-editing real device
//! state in a browser, without pulling in a web framework for what's meant
//! to be a quick, disposable view. One request at a time — this is a
//! dev/demo tool, not something to expose beyond a trusted LAN, and every
//! write here lands on the device's *currently loaded* live patch — it
//! will audibly change what the unit sounds like in real time.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use pod_core::{AmpKnob, Block, PodDevice};

/// Minimum gap enforced between any two messages sent to the device
/// (regardless of channel). Not a confirmed hardware requirement — added
/// after a live-channel float-set (`set_amp_knob`) immediately followed by
/// a patch-channel read (`read_patch`) wedged a real POD X3 Live hard
/// enough to need a power cycle (the OUT endpoint stopped accepting data
/// for a full 2s host-side timeout). Every previously-confirmed sequence
/// either stayed within one channel (a stream of live knob writes) or was
/// a lone read; this pause guards the untested cross-channel case, which
/// is exactly what interleaving the periodic `/api/patches` poll with a
/// user's knob/block/select action does below.
const MIN_INTER_MESSAGE_GAP: Duration = Duration::from_millis(300);

const AMP_KNOBS: &[&str] = &["bass", "middle", "treble", "drive", "presence", "volume"];
const BLOCKS: &[&str] = &[
    "gate", "wah", "stomp", "amp", "eq", "comp", "mod", "delay", "reverb",
];

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>POD Controller — live</title>
<style>
  body { margin: 0; padding: 32px; background: #16181d; color: #e8e6e1; font-family: system-ui, sans-serif; }
  h1 { font-size: 18px; margin: 0 0 4px; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: 0.05em; color: #8a8f98; margin: 28px 0 12px; }
  .sub { color: #8a8f98; font-size: 13px; margin-bottom: 24px; }
  .status { display: inline-flex; align-items: center; gap: 8px; padding: 6px 12px; border-radius: 20px; background: #1f2937; font-size: 13px; margin-bottom: 8px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #4b5563; }
  .status.ok .dot { background: #34d399; }
  .status.err .dot { background: #f87171; }
  .layout { display: flex; gap: 40px; flex-wrap: wrap; }
  table { border-collapse: collapse; width: 100%; max-width: 420px; }
  td { padding: 8px 12px; border-bottom: 1px solid #262a33; font-size: 14px; cursor: pointer; }
  td.code { font-family: monospace; color: #8a8f98; width: 60px; }
  tr.selected td { color: #34d399; }
  tr:hover td { background: #1c1f26; }
  .panel { min-width: 280px; }
  .knob-row { display: grid; grid-template-columns: 90px 1fr 48px; align-items: center; gap: 10px; margin-bottom: 10px; }
  .knob-row label { font-size: 13px; color: #c9cdd3; }
  .knob-row input[type=range] { width: 100%; }
  .knob-row .val { font-family: monospace; font-size: 12px; color: #8a8f98; text-align: right; }
  .blocks { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 4px; }
  .block-toggle { display: flex; align-items: center; gap: 6px; background: #1f2937; border: 1px solid #2a2f3a; border-radius: 8px; padding: 6px 10px; font-size: 13px; cursor: pointer; }
  .block-toggle input { margin: 0; }
  .hint { margin-top: 20px; color: #8a8f98; font-size: 12px; max-width: 480px; }
  #log { margin-top: 16px; font-family: monospace; font-size: 12px; color: #8a8f98; min-height: 1.2em; }
  #log.err { color: #f87171; }
</style>
</head>
<body>
<h1>POD Controller — live</h1>
<div class="sub">Every edit below writes to the device's <b>currently loaded</b> patch in real time. Bank/tone set by <code>pod-cli serve</code> flags.</div>
<div id="status" class="status"><span class="dot"></span><span>connecting…</span></div>

<div class="layout">
  <div>
    <h2>Patches (click to load)</h2>
    <table id="patches"></table>
  </div>

  <div class="panel">
    <h2 id="tone-heading">Amp — Tone</h2>
    <div id="knobs"></div>
    <h2>Blocks</h2>
    <div id="blocks" class="blocks"></div>
  </div>
</div>

<div id="log"></div>
<div class="hint">Rename a patch or turn a knob on the unit's front panel and the patch list should update on the next poll. Selecting a patch here calls the device's "select slot" message, which is undocumented as fire-and-forget (no confirmed reply) — if it doesn't seem to do anything, that's a protocol gap, not this page.</div>

<script>
const AMP_KNOBS = __AMP_KNOBS__;
const BLOCKS = __BLOCKS__;
let selectedSlot = null;

function log(msg, isErr) {
  const el = document.getElementById('log');
  el.textContent = msg;
  el.className = isErr ? 'err' : '';
}

async function post(path) {
  const res = await fetch(path, { method: 'POST' });
  const data = await res.json();
  if (!data.ok) throw new Error(data.error || 'request failed');
  return data;
}

function buildKnobs() {
  const el = document.getElementById('knobs');
  el.innerHTML = AMP_KNOBS.map(k => `
    <div class="knob-row">
      <label for="k-${k}">${k}</label>
      <input type="range" id="k-${k}" min="0" max="1" step="0.01" value="0.5"
        oninput="onKnob('${k}', this.value)">
      <span class="val" id="v-${k}">0.50</span>
    </div>`).join('');
}

function buildBlocks() {
  const el = document.getElementById('blocks');
  el.innerHTML = BLOCKS.map(b => `
    <label class="block-toggle">
      <input type="checkbox" id="b-${b}" onchange="onBlock('${b}', this.checked)">
      ${b}
    </label>`).join('');
}

async function onKnob(knob, value) {
  document.getElementById('v-' + knob).textContent = Number(value).toFixed(2);
  try {
    await post(`/api/amp?tone=0&knob=${knob}&value=${value}`);
  } catch (e) {
    log('amp ' + knob + ' failed: ' + e.message, true);
  }
}

async function onBlock(block, enabled) {
  try {
    await post(`/api/block?tone=0&block=${block}&enabled=${enabled}`);
    log(block + (enabled ? ' enabled' : ' disabled'));
  } catch (e) {
    log('block ' + block + ' failed: ' + e.message, true);
  }
}

async function selectPatch(slot, code) {
  try {
    await post(`/api/select?slot=${slot}`);
    selectedSlot = slot;
    log('selected ' + code);
    renderPatchSelection();
  } catch (e) {
    log('select failed: ' + e.message, true);
  }
}

function renderPatchSelection() {
  document.querySelectorAll('#patches tr').forEach(tr => {
    tr.classList.toggle('selected', Number(tr.dataset.slot) === selectedSlot);
  });
}

async function poll() {
  const statusEl = document.getElementById('status');
  const tableEl = document.getElementById('patches');
  try {
    const status = await (await fetch('/api/status')).json();
    statusEl.className = 'status ' + (status.connected ? 'ok' : 'err');
    statusEl.querySelector('span:last-child').textContent = status.connected
      ? `connected · POD X3${status.product_id === '414b' ? ' Live' : ''} (bus ${status.bus} addr ${status.addr})`
      : ('disconnected' + (status.error ? ': ' + status.error : ''));

    const patches = await (await fetch('/api/patches')).json();
    tableEl.innerHTML = patches.map(p =>
      `<tr data-slot="${p.slot}" onclick="selectPatch(${p.slot}, '${p.code}')">
        <td class="code">${p.code}</td><td>${p.name || '(read error)'}</td>
      </tr>`
    ).join('');
    renderPatchSelection();
  } catch (e) {
    statusEl.className = 'status err';
    statusEl.querySelector('span:last-child').textContent = 'poll failed: ' + e;
  }
}

buildKnobs();
buildBlocks();
poll();
setInterval(poll, 3000);
</script>
</body>
</html>"#;

fn amp_knobs_js() -> String {
    format!(
        "[{}]",
        AMP_KNOBS
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn blocks_js() -> String {
    format!(
        "[{}]",
        BLOCKS
            .iter()
            .map(|b| format!("\"{b}\""))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape_json(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            _ => vec![c],
        })
        .collect()
}

fn ok_json() -> String {
    r#"{"ok":true}"#.to_string()
}

fn err_json(msg: &str) -> String {
    format!(r#"{{"ok":false,"error":"{}"}}"#, escape_json(msg))
}

fn status_json() -> String {
    match pod_core::find_devices() {
        Ok(devices) => match devices.first() {
            Some(d) => format!(
                r#"{{"connected":true,"vendor_id":"{:04x}","product_id":"{:04x}","bus":{},"addr":{}}}"#,
                d.vendor_id(),
                d.product_id(),
                d.busnum(),
                d.device_address()
            ),
            None => r#"{"connected":false}"#.to_string(),
        },
        Err(e) => format!(
            r#"{{"connected":false,"error":"{}"}}"#,
            escape_json(&e.to_string())
        ),
    }
}

/// Runs `f` against the persistent device connection, opening it lazily on
/// first use. On failure, drops the connection and retries once against a
/// freshly (re)opened one — cheap insurance against a stalled endpoint,
/// and much less prone to the USB claim/release races that came from the
/// earlier design of opening a brand new connection per request.
///
/// Also enforces [`MIN_INTER_MESSAGE_GAP`] since the last message sent to
/// the device, sleeping first if the last one was too recent — see that
/// constant for why.
fn with_device<T>(
    dev: &mut Option<PodDevice>,
    last_op: &mut Instant,
    f: impl Fn(&mut PodDevice) -> pod_core::Result<T>,
) -> pod_core::Result<T> {
    let elapsed = last_op.elapsed();
    if elapsed < MIN_INTER_MESSAGE_GAP {
        std::thread::sleep(MIN_INTER_MESSAGE_GAP - elapsed);
    }
    *last_op = Instant::now();

    if dev.is_none() {
        *dev = Some(PodDevice::open_first()?);
    }
    match f(dev.as_mut().expect("just set")) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Drop the stale connection (releasing its interface claim)
            // before opening a new one — otherwise the reopen contends
            // with our own still-held claim and masks the real error.
            *dev = None;
            *dev = Some(PodDevice::open_first()?);
            f(dev.as_mut().expect("just set"))
        }
    }
}

/// Live-reads the tone name for each slot in `bank` (1-32) straight off the
/// device. Non-destructive (read-only).
fn patches_json(bank: u8, dev: &mut Option<PodDevice>, last_op: &mut Instant) -> String {
    let entries: Vec<String> = (0..4u8)
        .map(|i| {
            let slot = bank.saturating_sub(1) * 4 + i;
            let code = format!("{bank:02}{}", (b'A' + i) as char);
            let name = match with_device(dev, last_op, |d| d.read_patch(slot)) {
                Ok(patch) => pod_core::blob::tone_name(&patch, pod_core::blob::TONE1_NAME_OFFSET),
                Err(e) => format!("error: {e}"),
            };
            format!(
                r#"{{"slot":{slot},"code":"{code}","name":"{}"}}"#,
                escape_json(&name)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Parses `key=value` pairs out of a raw (not percent-decoded) query
/// string. Fine for this page's own params (digits, decimals, plain
/// lowercase words) — never use this on untrusted external input.
fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key { Some(v) } else { None }
    })
}

fn api_select(query: &str, dev: &mut Option<PodDevice>, last_op: &mut Instant) -> String {
    let Some(slot) = query_param(query, "slot").and_then(|s| s.parse::<u8>().ok()) else {
        return err_json("missing or invalid slot");
    };
    match with_device(dev, last_op, |d| d.select_slot(slot)) {
        Ok(()) => ok_json(),
        Err(e) => err_json(&e.to_string()),
    }
}

fn api_amp(query: &str, dev: &mut Option<PodDevice>, last_op: &mut Instant) -> String {
    let tone = query_param(query, "tone").and_then(|s| s.parse::<u8>().ok());
    let knob = query_param(query, "knob").and_then(|s| s.parse::<AmpKnob>().ok());
    let value = query_param(query, "value").and_then(|s| s.parse::<f32>().ok());
    let (Some(tone), Some(knob), Some(value)) = (tone, knob, value) else {
        return err_json("missing or invalid tone/knob/value");
    };
    match with_device(dev, last_op, |d| d.set_amp_knob(tone, knob, value)) {
        Ok(()) => ok_json(),
        Err(e) => err_json(&e.to_string()),
    }
}

fn api_block(query: &str, dev: &mut Option<PodDevice>, last_op: &mut Instant) -> String {
    let tone = query_param(query, "tone").and_then(|s| s.parse::<u8>().ok());
    let block = query_param(query, "block").and_then(|s| s.parse::<Block>().ok());
    let enabled = query_param(query, "enabled").map(|s| s == "true");
    let (Some(tone), Some(block), Some(enabled)) = (tone, block, enabled) else {
        return err_json("missing or invalid tone/block/enabled");
    };
    match with_device(dev, last_op, |d| d.set_block_enabled(tone, block, enabled)) {
        Ok(()) => ok_json(),
        Err(e) => err_json(&e.to_string()),
    }
}

fn handle(mut stream: TcpStream, bank: u8, dev: &mut Option<PodDevice>, last_op: &mut Instant) {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    let mut parts = request.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let (status, content_type, body) = match (method, path) {
        ("GET", "/") => {
            let html = INDEX_HTML
                .replace("__AMP_KNOBS__", &amp_knobs_js())
                .replace("__BLOCKS__", &blocks_js());
            ("200 OK", "text/html; charset=utf-8", html)
        }
        ("GET", "/api/status") => ("200 OK", "application/json", status_json()),
        ("GET", "/api/patches") => (
            "200 OK",
            "application/json",
            patches_json(bank, dev, last_op),
        ),
        ("POST", "/api/select") => (
            "200 OK",
            "application/json",
            api_select(query, dev, last_op),
        ),
        ("POST", "/api/amp") => ("200 OK", "application/json", api_amp(query, dev, last_op)),
        ("POST", "/api/block") => (
            "200 OK",
            "application/json",
            api_block(query, dev, last_op),
        ),
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

pub fn run(port: u16, bank: u8) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("Serving live POD view on http://<this-host>:{port}/ (bank {bank:02})");
    let mut dev: Option<PodDevice> = None;
    let mut last_op = Instant::now()
        .checked_sub(MIN_INTER_MESSAGE_GAP)
        .unwrap_or_else(Instant::now);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => handle(s, bank, &mut dev, &mut last_op),
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
    Ok(())
}
