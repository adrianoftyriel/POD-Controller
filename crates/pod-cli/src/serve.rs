//! `pod-cli serve`: a small HTTP server with a browser editor for the POD.
//! Meant for a trusted LAN: there is no authentication.
//!
//! The server owns one device connection and a **working copy** of the
//! patch loaded into the POD. Every edit is sent to the POD live and applied
//! to the working copy, so "save" writes exactly what is being heard, to any
//! slot. The POD's edit buffer can't be read back, so edits made on the
//! unit's own front panel are not reflected.
//!
//! The UI is static files (see `--ui-dir`); this module only serves them and
//! the JSON API below. All mutating calls are POSTs with query-string
//! arguments and answer `{"ok":true,"current":<working copy>}` or
//! `{"ok":false,"error":"...","current":...}`.
//!
//! | Call | Arguments |
//! |---|---|
//! | `GET /api/state` | device status, scan progress, working copy |
//! | `GET /api/patches` | every slot's name (`null` until scanned) |
//! | `POST /api/rescan` | re-read all patch names |
//! | `POST /api/load` | `slot` |
//! | `POST /api/revert` | reload the working copy's slot |
//! | `POST /api/save` | `slot` (any slot; the working copy moves there) |
//! | `POST /api/rename` | `tone`, `name` (printable ASCII, <= 16) |
//! | `POST /api/tone` | `tone`: which tone the POD edits (front panel) |
//! | `POST /api/param` | `tone`, `block`, `id` (hex param ID), `value` |
//! | `POST /api/enable` | `tone`, `block`, `on` (`true`/`false`) |
//! | `POST /api/model` | `tone`, `block`, `category`, `table`, `model`, `params` (`id:value,...`) |
//! | `POST /api/move` | `tone`, `block`, `slot`, `group` |
//! | `POST /api/setting` | `tone`, `key` (see `blob::TONE_SETTINGS`), `value` |
//!
//! `/mcp` is an MCP server over the same working copy (see `mcp.rs`).
//!
//! `block` is the record index in the tone block (0 amp ... 11 FX loop,
//! `blob::RECORD_NAMES`).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pod_core::blob::{self, Record};
use pod_core::protocol::{self, ToneValue};
use pod_core::PodDevice;
use serde_json::{json, Value};

/// User patch slots on a POD X3 Live: 16 banks of 4 (Gearbox's GET ALL
/// reads slots 0-63).
pub(crate) const SLOT_COUNT: usize = 64;

/// Record indices of the amp and cab: Gearbox's amp on/off button switches
/// both.
const AMP: usize = 0;
const CAB: usize = 1;

/// The UI files built into the binary, used when a file isn't found in the
/// UI directory (e.g. an installed binary with no source tree). Add any new
/// file the UI can't do without here.
const EMBEDDED: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../ui/index.html")),
    ("style.css", include_bytes!("../ui/style.css")),
    ("app.js", include_bytes!("../ui/app.js")),
    ("catalog.json", include_bytes!("../ui/catalog.json")),
];

pub(crate) struct Working {
    pub(crate) slot: usize,
    pub(crate) patch: Vec<u8>,
    pub(crate) dirty: bool,
}

#[derive(Default)]
pub(crate) struct State {
    dev: Option<PodDevice>,
    pub(crate) names: Vec<Option<String>>,
    pub(crate) scanned: usize,
    pub(crate) scanning: bool,
    pub(crate) current: Option<Working>,
    pub(crate) error: Option<String>,
    initial_bank: usize,
    /// Bumped on every successful edit/load/save, so a browser can tell the
    /// working copy changed under it (e.g. by an MCP client).
    pub(crate) rev: u64,
}

pub(crate) type Shared = Arc<Mutex<State>>;

pub(crate) fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run `f` against the device, opening it on first use. On failure the
/// connection is dropped and `f` retried once on a fresh one.
fn with_device<T>(
    st: &mut State,
    f: impl Fn(&mut PodDevice) -> pod_core::Result<T>,
) -> pod_core::Result<T> {
    let result = (|| {
        if st.dev.is_none() {
            st.dev = Some(PodDevice::open_first()?);
        }
        match f(st.dev.as_mut().expect("just set")) {
            Ok(v) => Ok(v),
            Err(_) => {
                // Release the old claim before reopening.
                st.dev = None;
                st.dev = Some(PodDevice::open_first()?);
                f(st.dev.as_mut().expect("just set"))
            }
        }
    })();
    st.error = result.as_ref().err().map(|e| e.to_string());
    result
}

pub(crate) fn slot_code(slot: usize) -> String {
    format!("{:02}{}", slot / 4 + 1, (b'A' + (slot % 4) as u8) as char)
}

// --- JSON views -------------------------------------------------------------

fn record_json(index: usize, r: &Record) -> Value {
    json!({
        "index": index,
        "key": blob::RECORD_NAMES[index],
        "category": r.category,
        "table": r.table,
        "model": r.model,
        "slot": r.slot,
        "group": r.group,
        "enabled": r.enabled,
        "sync": r.sync,
        "params": r.params.iter()
            .map(|p| json!({"id": format!("{:08x}", p.id), "value": p.value}))
            .collect::<Vec<_>>(),
    })
}

fn working_json(w: &Working) -> Value {
    let tones: Vec<Value> = (0..2)
        .map(|t| {
            let settings: serde_json::Map<String, Value> = blob::TONE_SETTINGS
                .iter()
                .map(|s| {
                    let v = blob::get_setting(&w.patch, t, s).unwrap_or(0.0);
                    (s.key.to_string(), json!(v))
                })
                .collect();
            let blocks: Vec<Value> = (0..blob::RECORD_COUNT)
                .filter_map(|i| {
                    blob::record(&w.patch, t, i)
                        .ok()
                        .map(|r| record_json(i, &r))
                })
                .collect();
            json!({
                "name": blob::tone_name(&w.patch, t * blob::TONE_LEN),
                "settings": settings,
                "blocks": blocks,
            })
        })
        .collect();
    json!({
        "slot": w.slot,
        "code": slot_code(w.slot),
        "dirty": w.dirty,
        "tones": tones,
    })
}

pub(crate) fn current_json(st: &State) -> Value {
    st.current.as_ref().map(working_json).unwrap_or(Value::Null)
}

fn state_json(st: &State) -> Value {
    let device = pod_core::find_devices().ok().and_then(|d| {
        d.first().map(|d| {
            json!({
                "product_id": format!("{:04x}", d.product_id()),
                "live": d.product_id() == protocol::PRODUCT_ID_X3_LIVE,
                "bus": d.busnum(),
                "addr": d.device_address(),
            })
        })
    });
    json!({
        "connected": device.is_some(),
        "device": device,
        "error": st.error,
        "initial_bank": st.initial_bank,
        "rev": st.rev,
        "scan": {"scanned": st.scanned, "total": SLOT_COUNT, "scanning": st.scanning},
        "current": current_json(st),
    })
}

fn patches_json(st: &State) -> Value {
    json!({
        "total": SLOT_COUNT,
        "banks": SLOT_COUNT / 4,
        "patches": (0..SLOT_COUNT).map(|s| json!({
            "slot": s,
            "code": slot_code(s),
            "name": st.names.get(s).cloned().flatten(),
        })).collect::<Vec<_>>(),
    })
}

// --- API --------------------------------------------------------------------

pub(crate) struct Query(pub(crate) Vec<(String, String)>);

impl Query {
    fn parse(q: &str) -> Self {
        Query(
            q.split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    (url_decode(k), url_decode(v))
                })
                .collect(),
        )
    }

    fn str(&self, key: &str) -> Result<&str, String> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| format!("missing {key}"))
    }

    fn num<T: std::str::FromStr>(&self, key: &str) -> Result<T, String> {
        self.str(key)?.parse().map_err(|_| format!("invalid {key}"))
    }
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(b)) => {
                out.push(b);
                i += 2;
            }
            (b, _) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn tone_arg(q: &Query) -> Result<usize, String> {
    let tone: usize = q.num("tone")?;
    if tone > 1 {
        return Err("tone must be 0 or 1".into());
    }
    Ok(tone)
}

fn block_arg(q: &Query) -> Result<usize, String> {
    let block: usize = q.num("block")?;
    if block >= blob::RECORD_COUNT {
        return Err(format!("block must be 0-{}", blob::RECORD_COUNT - 1));
    }
    Ok(block)
}

fn slot_arg(q: &Query) -> Result<usize, String> {
    let slot: usize = q.num("slot")?;
    if slot >= SLOT_COUNT {
        return Err(format!("slot must be 0-{}", SLOT_COUNT - 1));
    }
    Ok(slot)
}

fn working(st: &mut State) -> Result<&mut Working, String> {
    st.current
        .as_mut()
        .ok_or_else(|| "no patch loaded yet: load one first".to_string())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Read `slot` from memory and load it into the POD's edit buffer.
fn load(st: &mut State, slot: usize) -> Result<(), String> {
    let patch = with_device(st, |d| d.select_slot(slot as u8)).map_err(err)?;
    st.names[slot] = Some(blob::tone_name(&patch, 0));
    st.current = Some(Working {
        slot,
        patch,
        dirty: false,
    });
    Ok(())
}

fn push_tone(st: &mut State, tone: usize) -> Result<(), String> {
    let w = working(st)?;
    let block = w.patch[tone * blob::TONE_LEN..(tone + 1) * blob::TONE_LEN].to_vec();
    with_device(st, |d| d.push_tone(tone as u8, &block)).map_err(err)
}

fn parse_params(s: &str) -> Result<Vec<blob::Param>, String> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (id, v) = p.split_once(':').ok_or("params are id:value pairs")?;
            Ok(blob::Param {
                id: u32::from_str_radix(id, 16).map_err(|_| "invalid param id")?,
                value: v.parse().map_err(|_| "invalid param value")?,
            })
        })
        .collect()
}

/// Run one API call and bump `rev` if it succeeded.
pub(crate) fn api(st: &mut State, path: &str, q: &Query) -> Result<(), String> {
    let result = api_inner(st, path, q);
    if result.is_ok() {
        st.rev += 1;
    }
    result
}

fn api_inner(st: &mut State, path: &str, q: &Query) -> Result<(), String> {
    match path {
        "/api/load" => load(st, slot_arg(q)?),
        "/api/revert" => {
            let slot = working(st)?.slot;
            load(st, slot)
        }
        "/api/save" => {
            let slot = slot_arg(q)?;
            let patch = working(st)?.patch.clone();
            with_device(st, |d| d.write_patch(slot as u8, &patch)).map_err(err)?;
            st.names[slot] = Some(blob::tone_name(&patch, 0));
            let w = working(st)?;
            w.slot = slot;
            w.dirty = false;
            Ok(())
        }
        "/api/rename" => {
            let tone = tone_arg(q)?;
            let name = q.str("name")?.to_string();
            let w = working(st)?;
            blob::set_tone_name(&mut w.patch, tone, &name).map_err(err)?;
            w.dirty = true;
            push_tone(st, tone)
        }
        "/api/tone" => {
            let tone = tone_arg(q)?;
            let msg = protocol::encode_device_setting(
                0x00,
                protocol::setting::SELECTED_TONE,
                tone as u32,
            );
            with_device(st, |d| d.write_raw(&msg)).map_err(err)
        }
        "/api/param" => {
            let (tone, block) = (tone_arg(q)?, block_arg(q)?);
            let id = u32::from_str_radix(q.str("id")?, 16).map_err(|_| "invalid id")?;
            let value: f32 = q.num("value")?;
            let w = working(st)?;
            let rec = blob::record(&w.patch, tone, block).map_err(err)?;
            blob::set_param(&mut w.patch, tone, block, id, value).map_err(err)?;
            w.dirty = true;
            with_device(st, |d| {
                d.set_param_at(tone as u8, rec.slot, rec.group, id, value)
            })
            .map_err(err)
        }
        "/api/enable" => {
            let (tone, block) = (tone_arg(q)?, block_arg(q)?);
            let on = q.str("on")? == "true";
            let targets = if block == AMP || block == CAB {
                vec![AMP, CAB]
            } else {
                vec![block]
            };
            let w = working(st)?;
            let mut addrs = Vec::new();
            for i in targets {
                let mut rec = blob::record(&w.patch, tone, i).map_err(err)?;
                rec.enabled = on;
                blob::set_record(&mut w.patch, tone, i, &rec).map_err(err)?;
                addrs.push((rec.slot, rec.group));
            }
            w.dirty = true;
            with_device(st, |d| {
                addrs
                    .iter()
                    .try_for_each(|&(s, g)| d.set_enabled_at(tone as u8, s, g, on))
            })
            .map_err(err)
        }
        "/api/model" => {
            let (tone, block) = (tone_arg(q)?, block_arg(q)?);
            let params = parse_params(q.str("params").unwrap_or(""))?;
            let (category, table, model) = (q.num("category")?, q.num("table")?, q.num("model")?);
            let w = working(st)?;
            let mut rec = blob::record(&w.patch, tone, block).map_err(err)?;
            rec.category = category;
            rec.table = table;
            rec.model = model;
            if !params.is_empty() {
                rec.params = params;
            }
            blob::set_record(&mut w.patch, tone, block, &rec).map_err(err)?;
            w.dirty = true;
            // No parameter set reaches the model: Gearbox re-pushes the tone.
            push_tone(st, tone)
        }
        "/api/move" => {
            let (tone, block) = (tone_arg(q)?, block_arg(q)?);
            let (slot, group): (u16, u16) = (q.num("slot")?, q.num("group")?);
            let w = working(st)?;
            let mut rec = blob::record(&w.patch, tone, block).map_err(err)?;
            let (old_slot, old_group) = (rec.slot, rec.group);
            rec.slot = slot;
            rec.group = group;
            // The POD re-announces a moved block as enabled.
            rec.enabled = true;
            blob::set_record(&mut w.patch, tone, block, &rec).map_err(err)?;
            w.dirty = true;
            with_device(st, |d| {
                d.move_block(tone as u8, old_slot, old_group, slot, group)
            })
            .map_err(err)
        }
        "/api/setting" => {
            let tone = tone_arg(q)?;
            let key = q.str("key")?;
            let s = blob::tone_setting(key).ok_or_else(|| format!("unknown setting {key}"))?;
            let value: f64 = q.num("value")?;
            let w = working(st)?;
            blob::set_setting(&mut w.patch, tone, s, value).map_err(err)?;
            w.dirty = true;
            let v = if s.is_float {
                ToneValue::Float(value as f32)
            } else {
                ToneValue::Int(value as u32)
            };
            with_device(st, |d| d.set_tone_setting(tone as u8, s.param, v)).map_err(err)
        }
        _ => Err("not found".into()),
    }
}

// --- scanning ---------------------------------------------------------------

/// Read every slot's name in the background, one slot per lock so UI
/// requests get in between.
pub(crate) fn start_scan(state: &Shared) {
    {
        let mut st = lock(state);
        if st.scanning {
            return;
        }
        st.scanning = true;
        st.scanned = 0;
    }
    let state = state.clone();
    std::thread::spawn(move || {
        for slot in 0..SLOT_COUNT {
            {
                let mut st = lock(&state);
                match with_device(&mut st, |d| d.read_patch(slot as u8)) {
                    Ok(patch) => st.names[slot] = Some(blob::tone_name(&patch, 0)),
                    Err(e) => eprintln!("scan: slot {slot}: {e}"),
                }
                st.scanned = slot + 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        lock(&state).scanning = false;
    });
}

// --- HTTP -------------------------------------------------------------------

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// A UI file: from `ui_dir` if given, else the built-in copy. `rel` has no
/// leading slash.
pub(crate) fn static_file(ui_dir: Option<&Path>, rel: &str) -> Option<Vec<u8>> {
    if rel.split('/').any(|c| c == ".." || c.is_empty()) {
        return None;
    }
    if let Some(dir) = ui_dir {
        if let Ok(bytes) = std::fs::read(dir.join(rel)) {
            return Some(bytes);
        }
    }
    EMBEDDED
        .iter()
        .find(|(name, _)| *name == rel)
        .map(|(_, body)| body.to_vec())
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    respond_with(stream, status, ctype, &[], body)
}

fn respond_with(
    stream: &mut TcpStream,
    status: &str,
    ctype: &str,
    headers: &[(&str, String)],
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_reply(stream: &mut TcpStream, v: Value) {
    respond(
        stream,
        "200 OK",
        "application/json",
        v.to_string().as_bytes(),
    )
}

/// A parsed HTTP request. Header names are lower-cased.
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Largest request body accepted (MCP calls are small).
const MAX_BODY: usize = 1 << 20;

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut parts = lines.next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return None;
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Some(Request {
        method,
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body,
    })
}

fn handle(mut stream: TcpStream, state: &Shared, ui: &Ui) {
    let Some(req) = read_request(&mut stream) else {
        return respond(&mut stream, "400 Bad Request", "text/plain", b"bad request");
    };

    match (req.method.as_str(), req.path.as_str()) {
        (_, "/mcp") => {
            let reply = crate::mcp::handle(&req, state, &ui.catalog);
            respond_with(
                &mut stream,
                reply.status,
                reply.content_type,
                &reply.headers,
                &reply.body,
            )
        }
        ("GET", "/api/state") => json_reply(&mut stream, state_json(&lock(state))),
        ("GET", "/api/patches") => json_reply(&mut stream, patches_json(&lock(state))),
        ("POST", "/api/rescan") => {
            start_scan(state);
            json_reply(&mut stream, json!({"ok": true}))
        }
        ("POST", p) if p.starts_with("/api/") => {
            let q = Query::parse(&req.query);
            let mut st = lock(state);
            let reply = match api(&mut st, p, &q) {
                Ok(()) => json!({"ok": true, "current": current_json(&st)}),
                Err(e) => json!({"ok": false, "error": e, "current": current_json(&st)}),
            };
            drop(st);
            json_reply(&mut stream, reply)
        }
        ("GET", p) => {
            let rel = match p.trim_start_matches('/') {
                "" => "index.html",
                rel => rel,
            };
            match static_file(ui.dir.as_deref(), rel) {
                Some(body) => respond(&mut stream, "200 OK", content_type(rel), &body),
                None => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
            }
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Where the UI comes from, plus its parsed catalog (MCP tools use its
/// model and parameter names).
struct Ui {
    dir: Option<PathBuf>,
    catalog: crate::mcp::Catalog,
}

pub fn run(port: u16, bank: u8, ui_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let ui_dir = ui_dir.or_else(|| {
        let dev_tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        dev_tree.is_dir().then_some(dev_tree)
    });
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!(
        "Serving the POD editor on http://<this-host>:{port}/ (UI from {})",
        ui_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "built-in copy".into())
    );
    let state: Shared = Arc::new(Mutex::new(State {
        names: vec![None; SLOT_COUNT],
        initial_bank: (bank.max(1) as usize).min(SLOT_COUNT / 4),
        ..Default::default()
    }));
    let catalog = static_file(ui_dir.as_deref(), "catalog.json")
        .and_then(|b| serde_json::from_slice(&b).ok())
        .map(crate::mcp::Catalog)
        .ok_or_else(|| anyhow::anyhow!("catalog.json missing or invalid"))?;
    println!("MCP endpoint: http://<this-host>:{port}/mcp");
    start_scan(&state);
    let ui = Arc::new(Ui {
        dir: ui_dir,
        catalog,
    });
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let state = state.clone();
                let ui = ui.clone();
                std::thread::spawn(move || handle(s, &state, &ui));
            }
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_codes() {
        assert_eq!(slot_code(0), "01A");
        assert_eq!(slot_code(31), "08D");
        assert_eq!(slot_code(63), "16D");
    }

    #[test]
    fn url_decoding() {
        assert_eq!(url_decode("Tweed+Solo%21"), "Tweed Solo!");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("a%2"), "a%2");
    }

    #[test]
    fn params_parse() {
        let p = parse_params("3f100000:0.5,3f010001:1").unwrap();
        assert_eq!(
            p[0],
            blob::Param {
                id: 0x3F10_0000,
                value: 0.5
            }
        );
        assert_eq!(p[1].id, 0x3F01_0001);
        assert!(parse_params("zz:1").is_err());
    }

    #[test]
    fn static_files_refuse_traversal() {
        assert!(static_file(None, "../Cargo.toml").is_none());
        assert!(static_file(None, "index.html").is_some());
    }
}
