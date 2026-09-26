//! MCP (Model Context Protocol) server built into `pod-cli serve`, at
//! `POST /mcp` (Streamable HTTP transport, stateless, JSON responses; no
//! server-initiated SSE stream).
//!
//! The tools drive the same working copy as the web UI, through the same
//! `serve::api` calls, so an agent and a browser can share a session. They
//! speak in names and display units instead of raw IDs: blocks by key
//! (`delay`), models and parameters by their catalog names, tones as 1/2,
//! patches as codes (`08C`), normal knobs as 0-100.
//!
//! Like the rest of `serve`, this is for a trusted LAN: there is no
//! authentication. Requests with a foreign `Origin` header are refused, so a
//! web page can't drive the POD through a browser (DNS rebinding).

use serde_json::{json, Map, Value};

use crate::serve::{self, lock, slot_code, Query, Request, Shared, State, SLOT_COUNT};
use pod_core::blob;

const SERVER_NAME: &str = "pod-controller";
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Controls a Line 6 POD X3 Live guitar processor. \
Load a patch with load_patch, inspect it with get_patch, then edit: every edit is \
heard immediately but only stored when save_patch is called. Blocks: amp, cab, \
stomp, mod, delay, reverb, gate, comp, eq, wah, volume, loop. Knob values are \
0-100 unless get_patch shows a unit. Patches are codes 01A-16D. Bank 08 is the \
owner's scratch bank; do not save over other patches unless the user asks.";

/// Gearbox's drawing order; movable blocks appear where their group says.
const CHAIN_ORDER: &[(&str, Option<u16>)] = &[
    ("gate", None),
    ("volume", Some(2)),
    ("wah", None),
    ("stomp", None),
    ("mod", Some(2)),
    ("delay", Some(2)),
    ("reverb", Some(2)),
    ("loop", Some(2)),
    ("amp", None),
    ("cab", None),
    ("comp", None),
    ("eq", None),
    ("volume", Some(5)),
    ("loop", Some(5)),
    ("mod", Some(5)),
    ("delay", Some(5)),
    ("reverb", Some(5)),
];

/// The UI's catalog.json (see tools/catalog/build_catalog.py).
pub(crate) struct Catalog(pub(crate) Value);

impl Catalog {
    fn block(&self, key: &str) -> Option<&Value> {
        self.0["blocks"]
            .as_array()?
            .iter()
            .find(|b| b["key"] == key)
    }

    fn models(&self, key: &str) -> &[Value] {
        self.block(key)
            .and_then(|b| b["models"].as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn model_def(&self, key: &str, r: &blob::Record) -> Option<&Value> {
        self.models(key)
            .iter()
            .find(|m| m["category"] == r.category && m["table"] == r.table && m["model"] == r.model)
    }

    fn setting(&self, key: &str) -> Option<&Value> {
        self.0["settings"]
            .as_array()?
            .iter()
            .find(|s| s["key"] == key)
    }
}

pub(crate) struct Reply {
    pub(crate) status: &'static str,
    pub(crate) content_type: &'static str,
    pub(crate) headers: Vec<(&'static str, String)>,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    fn text(status: &'static str, body: &str) -> Self {
        Reply {
            status,
            content_type: "text/plain",
            headers: vec![],
            body: body.as_bytes().to_vec(),
        }
    }
}

/// Is `origin` this server itself or localhost? Browsers send `Origin` on
/// cross-site requests; MCP clients usually send none.
fn origin_allowed(origin: &str, host: Option<&str>) -> bool {
    let o = origin
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let o_host = o.split(['/', ':']).next().unwrap_or("");
    ["localhost", "127.0.0.1", "[::1]"].contains(&o_host)
        || host.is_some_and(|h| h.split(':').next() == Some(o_host))
}

pub(crate) fn handle(req: &Request, state: &Shared, catalog: &Catalog) -> Reply {
    if let Some(origin) = req.header("origin") {
        if !origin_allowed(origin, req.header("host")) {
            return Reply::text("403 Forbidden", "origin not allowed");
        }
    }
    if req.method != "POST" {
        let mut r = Reply::text("405 Method Not Allowed", "POST JSON-RPC to /mcp");
        r.headers.push(("Allow", "POST".into()));
        return r;
    }
    let msg: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return json_reply(rpc_error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    let replies: Vec<Value> = match &msg {
        Value::Array(batch) => batch
            .iter()
            .filter_map(|m| dispatch(m, state, catalog))
            .collect(),
        m => dispatch(m, state, catalog).into_iter().collect(),
    };
    match (msg.is_array(), replies.len()) {
        (_, 0) => Reply {
            status: "202 Accepted",
            content_type: "application/json",
            headers: vec![],
            body: vec![],
        },
        (false, _) => json_reply(replies.into_iter().next().expect("one reply")),
        (true, _) => json_reply(Value::Array(replies)),
    }
}

fn json_reply(v: Value) -> Reply {
    Reply {
        status: "200 OK",
        content_type: "application/json",
        headers: vec![],
        body: v.to_string().into_bytes(),
    }
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Handle one JSON-RPC message. Notifications and client responses get no
/// reply.
fn dispatch(m: &Value, state: &Shared, catalog: &Catalog) -> Option<Value> {
    let method = m["method"].as_str()?;
    let id = m.get("id")?.clone();
    let params = &m["params"];
    let result = match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let empty = Map::new();
            let args = params["arguments"].as_object().unwrap_or(&empty);
            let out = call_tool(name, args, state, catalog);
            Ok(match out {
                Ok(v) => json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}],
                    "structuredContent": v,
                    "isError": false,
                }),
                Err(e) => json!({
                    "content": [{"type": "text", "text": e}],
                    "isError": true,
                }),
            })
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, msg)) => rpc_error(id, code, &msg),
    })
}

// --- tool definitions -------------------------------------------------------

fn tone_prop() -> Value {
    json!({"type": "integer", "enum": [1, 2], "description": "Tone 1 or 2 (default 1)."})
}

fn block_prop() -> Value {
    json!({"type": "string", "enum": blob::RECORD_NAMES,
           "description": "Block key. 'amp' on/off also switches the cab."})
}

fn patch_prop(what: &str) -> Value {
    json!({"type": "string", "pattern": "^[0-9]{1,2}[A-Da-d]$",
           "description": format!("{what}: a patch code, bank 01-16 plus channel A-D, e.g. 08C.")})
}

fn tool(name: &str, title: &str, desc: &str, props: Value, required: &[&str], ann: Value) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": {"type": "object", "properties": props, "required": required,
                        "additionalProperties": false},
        "annotations": ann,
    })
}

fn tools() -> Vec<Value> {
    let ro = json!({"readOnlyHint": true, "openWorldHint": false});
    let edit = json!({"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true,
                      "openWorldHint": false});
    vec![
        tool("pod_status", "POD status",
            "Connection state, patch-name scan progress, and the loaded patch's code, name and unsaved-changes flag.",
            json!({}), &[], ro.clone()),
        tool("list_patches", "List patches",
            "Names of the stored patches, all 64 or one bank. Names read as null until the start-up scan reaches them.",
            json!({"bank": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Only this bank."}}),
            &[], ro.clone()),
        tool("load_patch", "Load patch",
            "Load a stored patch into the POD (heard immediately) and make it the working copy. Discards unsaved edits to the previous patch.",
            json!({"patch": patch_prop("Patch to load")}), &["patch"],
            json!({"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false})),
        tool("get_patch", "Get patch",
            "The working copy: per tone, the name, Variax/mic/room/input settings, and the signal chain in order with each block's model, on/off, pre/post position and parameter values (0-100 unless a unit is given).",
            json!({"tone": tone_prop(),
                   "include_hidden": {"type": "boolean", "description": "Also list unnamed internal parameters."}}),
            &[], ro.clone()),
        tool("list_models", "List models",
            "Model names available for a block, for set_model. Names marked '(no parameter data)' have no known parameter list.",
            json!({"block": block_prop()}), &["block"], ro),
        tool("set_model", "Set model",
            "Change a block's model. Parameters it shares with the old model keep their values; others start at typical values.",
            json!({"tone": tone_prop(), "block": block_prop(),
                   "model": {"type": "string", "description": "Model name from list_models (case-insensitive; a unique partial match works)."}}),
            &["block", "model"], edit.clone()),
        tool("set_params", "Set parameters",
            "Set one or more parameters of a block, by the names get_patch shows (or hex IDs). Values are 0-100 unless the parameter has a unit.",
            json!({"tone": tone_prop(), "block": block_prop(),
                   "params": {"type": "object", "additionalProperties": {"type": "number"},
                              "description": "e.g. {\"Drive\": 70, \"Volume\": 55}"}}),
            &["block", "params"], edit.clone()),
        tool("set_block_enabled", "Block on/off", "Switch a block on or off.",
            json!({"tone": tone_prop(), "block": block_prop(), "enabled": {"type": "boolean"}}),
            &["block", "enabled"], edit.clone()),
        tool("move_block", "Move block pre/post",
            "Put volume, loop, mod, delay or reverb before ('pre') or after ('post') the amp. Moving switches the block on.",
            json!({"tone": tone_prop(), "block": {"type": "string", "enum": ["volume", "loop", "mod", "delay", "reverb"]},
                   "position": {"type": "string", "enum": ["pre", "post"]}}),
            &["block", "position"], edit.clone()),
        tool("set_setting", "Set tone setting",
            "Set a tone-level setting: variax_model (name, e.g. 'Spank 4'), variax_tone (0-127), mic (e.g. '57 On Axis'), room (0-100) or input (e.g. 'Guitar').",
            json!({"tone": tone_prop(),
                   "setting": {"type": "string", "enum": ["variax_model", "variax_tone", "mic", "room", "input"]},
                   "value": {"type": ["string", "number"]}}),
            &["setting", "value"], edit.clone()),
        tool("rename_tone", "Rename tone",
            "Rename a tone (up to 16 printable ASCII characters). Tone 1's name is the patch name shown in lists.",
            json!({"tone": tone_prop(), "name": {"type": "string", "maxLength": 16}}),
            &["name"], edit.clone()),
        tool("select_tone", "Select tone on the POD",
            "Make the POD's own front panel edit tone 1 or 2.",
            json!({"tone": tone_prop()}), &["tone"], edit),
        tool("save_patch", "Save patch",
            "Write the working copy to a patch slot (default: the slot it was loaded from). Saving over a different, named patch needs overwrite: true.",
            json!({"patch": patch_prop("Target slot"),
                   "overwrite": {"type": "boolean", "description": "Required to replace a different stored patch."}}),
            &[], json!({"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false})),
        tool("revert_patch", "Revert patch",
            "Reload the working copy's slot from memory, discarding unsaved edits.",
            json!({}), &[], json!({"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false})),
    ]
}

// --- tool implementations -----------------------------------------------------

type Args = Map<String, Value>;
type ToolResult = Result<Value, String>;

fn arg_str<'a>(a: &'a Args, key: &str) -> Result<&'a str, String> {
    a.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string argument '{key}'"))
}

fn arg_tone(a: &Args) -> Result<usize, String> {
    match a.get("tone").and_then(Value::as_u64) {
        None if !a.contains_key("tone") => Ok(0),
        Some(1) => Ok(0),
        Some(2) => Ok(1),
        _ => Err("tone must be 1 or 2".into()),
    }
}

fn arg_block(a: &Args) -> Result<usize, String> {
    let key = arg_str(a, "block")?.to_ascii_lowercase();
    blob::RECORD_NAMES
        .iter()
        .position(|k| *k == key)
        .ok_or_else(|| format!("unknown block '{key}'; use one of {:?}", blob::RECORD_NAMES))
}

pub(crate) fn parse_code(code: &str) -> Result<usize, String> {
    let code = code.trim();
    let (bank, ch) = code.split_at(code.len().saturating_sub(1));
    let bank: usize = bank
        .parse()
        .map_err(|_| format!("bad patch code '{code}'"))?;
    let ch = "ABCD"
        .find(&ch.to_ascii_uppercase())
        .ok_or_else(|| format!("bad patch code '{code}'"))?;
    if !(1..=SLOT_COUNT / 4).contains(&bank) {
        return Err(format!("bank must be 01-{:02}", SLOT_COUNT / 4));
    }
    Ok((bank - 1) * 4 + ch)
}

fn q(pairs: &[(&str, String)]) -> Query {
    Query(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn run(st: &mut State, path: &str, pairs: &[(&str, String)]) -> Result<(), String> {
    serve::api(st, path, &q(pairs))
}

fn working(st: &State) -> Result<&serve::Working, String> {
    st.current
        .as_ref()
        .ok_or_else(|| "no patch loaded: call load_patch first".to_string())
}

fn summary(st: &State) -> Value {
    match &st.current {
        Some(w) => json!({"patch": slot_code(w.slot), "name": blob::tone_name(&w.patch, 0),
                          "unsaved_changes": w.dirty}),
        None => Value::Null,
    }
}

/// How a parameter is shown and entered: normalized knobs as 0-100, others
/// in their own unit and range.
struct ParamView {
    id: u32,
    name: String,
    normalized: bool,
    min: f64,
    max: f64,
    unit: String,
    hidden: bool,
}

impl ParamView {
    fn display(&self, raw: f32) -> f64 {
        let v = if self.normalized {
            raw as f64 * 100.0
        } else {
            raw as f64
        };
        (v * 10.0).round() / 10.0
    }

    fn to_raw(&self, shown: f64) -> Result<f32, String> {
        let (lo, hi) = if self.normalized {
            (0.0, 100.0)
        } else {
            (self.min, self.max)
        };
        if !(lo..=hi).contains(&shown) {
            return Err(format!(
                "{} must be {lo}-{hi}{}",
                self.name,
                self.unit_suffix()
            ));
        }
        Ok(if self.normalized {
            shown / 100.0
        } else {
            shown
        } as f32)
    }

    fn unit_suffix(&self) -> String {
        if self.unit.is_empty() {
            String::new()
        } else {
            format!(" {}", self.unit)
        }
    }
}

/// The parameters of a record, named from its model's catalog entry.
fn param_views(catalog: &Catalog, key: &str, rec: &blob::Record) -> Vec<ParamView> {
    let defs: Vec<&Value> = catalog
        .model_def(key, rec)
        .and_then(|m| m["params"].as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    rec.params
        .iter()
        .map(|p| {
            let id_hex = format!("{:08x}", p.id);
            let def = defs.iter().find(|d| d["id"] == id_hex.as_str());
            let min = def
                .and_then(|d| d["min"].as_f64())
                .unwrap_or(0.0)
                .min(p.value as f64);
            let max = def
                .and_then(|d| d["max"].as_f64())
                .unwrap_or(1.0)
                .max(p.value as f64);
            let unit = def
                .and_then(|d| d["unit"].as_str())
                .unwrap_or("")
                .to_string();
            ParamView {
                id: p.id,
                name: def
                    .and_then(|d| d["name"].as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("Param {id_hex}")),
                normalized: min == 0.0 && max == 1.0 && unit.is_empty(),
                min,
                max,
                unit,
                hidden: def
                    .and_then(|d| d["hidden"].as_bool())
                    .unwrap_or(id_hex.starts_with("3f20")),
            }
        })
        .collect()
}

/// Same order as the UI: normal knobs, real-unit controls, Mix, then the
/// unnamed 3F20 IDs, each by index.
fn param_rank(id: u32) -> u32 {
    let ns = match id >> 16 {
        0x3F10 => 0,
        0x3F00 => 1,
        0x3F01 => 2,
        0x3F20 => 3,
        _ => 4,
    };
    ns << 16 | (id & 0xFFFF)
}

fn model_name(catalog: &Catalog, key: &str, rec: &blob::Record) -> String {
    catalog
        .model_def(key, rec)
        .and_then(|m| m["name"].as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("unknown model {}/{}/{}", rec.category, rec.table, rec.model))
}

fn setting_display(catalog: &Catalog, key: &str, raw: f64) -> Value {
    let Some(def) = catalog.setting(key) else {
        return json!(raw);
    };
    match def["type"].as_str() {
        Some("enum") => def["options"]
            .as_array()
            .and_then(|o| o.iter().find(|o| o["value"].as_f64() == Some(raw)))
            .and_then(|o| o["name"].as_str())
            .map(|n| json!(n))
            .unwrap_or_else(|| json!(format!("User {raw}"))),
        Some("float") => json!((raw * 1000.0).round() / 10.0),
        _ => json!(raw),
    }
}

fn tone_view(catalog: &Catalog, patch: &[u8], tone: usize, hidden: bool) -> Value {
    let rec = |key: &str| {
        let i = blob::RECORD_NAMES
            .iter()
            .position(|k| *k == key)
            .expect("known key");
        blob::record(patch, tone, i).expect("valid patch")
    };
    let chain: Vec<Value> = CHAIN_ORDER
        .iter()
        .filter_map(|&(key, group)| {
            let r = rec(key);
            if group.is_some_and(|g| g != r.group) {
                return None;
            }
            let views = param_views(catalog, key, &r);
            let mut shown: Vec<(&ParamView, &blob::Param)> = views.iter().zip(&r.params).collect();
            shown.sort_by_key(|(v, _)| param_rank(v.id));
            let params: Vec<Value> = shown
                .into_iter()
                .filter(|(v, _)| hidden || !v.hidden)
                .map(|(v, p)| {
                    let mut o = json!({"name": v.name, "value": v.display(p.value)});
                    if !v.normalized {
                        o["unit"] = json!(v.unit);
                        o["range"] = json!([v.min, v.max]);
                    }
                    o
                })
                .collect();
            let mut o = json!({
                "block": key,
                "model": model_name(catalog, key, &r),
                "enabled": r.enabled,
            });
            if group.is_some() {
                o["position"] = json!(if r.group == 2 { "pre" } else { "post" });
            }
            if !params.is_empty() {
                o["params"] = json!(params);
            }
            Some(o)
        })
        .collect();
    let settings: Map<String, Value> = blob::TONE_SETTINGS
        .iter()
        .map(|s| {
            let raw = blob::get_setting(patch, tone, s).unwrap_or(0.0);
            (s.key.to_string(), setting_display(catalog, s.key, raw))
        })
        .collect();
    json!({
        "tone": tone + 1,
        "name": blob::tone_name(patch, tone * blob::TONE_LEN),
        "settings": settings,
        "chain": chain,
    })
}

/// Find a model by name: exact (case-insensitive) first, else a unique
/// partial match.
fn find_model<'a>(catalog: &'a Catalog, key: &str, name: &str) -> Result<&'a Value, String> {
    let models = catalog.models(key);
    let lname = name.to_ascii_lowercase();
    let name_of = |m: &Value| m["name"].as_str().unwrap_or("").to_ascii_lowercase();
    if let Some(m) = models.iter().find(|m| name_of(m) == lname) {
        return Ok(m);
    }
    let partial: Vec<&Value> = models
        .iter()
        .filter(|m| name_of(m).contains(&lname))
        .collect();
    match partial.len() {
        1 => Ok(partial[0]),
        0 => Err(format!("no {key} model matches '{name}'; see list_models")),
        _ => Err(format!(
            "'{name}' matches several {key} models: {}",
            partial
                .iter()
                .filter_map(|m| m["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn call_tool(name: &str, a: &Args, state: &Shared, catalog: &Catalog) -> ToolResult {
    if name == "list_patches" || name == "pod_status" {
        let st = lock(state);
        return match name {
            "pod_status" => Ok(json!({
                "connected": pod_core::find_devices().map(|d| !d.is_empty()).unwrap_or(false),
                "last_error": st.error,
                "names_scanned": format!("{}/{}", st.scanned, SLOT_COUNT),
                "loaded": summary(&st),
            })),
            _ => {
                let bank = a.get("bank").and_then(Value::as_u64).map(|b| b as usize);
                let patches: Vec<Value> = (0..SLOT_COUNT)
                    .filter(|s| bank.is_none_or(|b| s / 4 + 1 == b))
                    .map(|s| json!({"patch": slot_code(s), "name": st.names[s]}))
                    .collect();
                Ok(json!({"patches": patches}))
            }
        };
    }
    if name == "list_models" {
        let key = blob::RECORD_NAMES[arg_block(a)?];
        let names: Vec<String> = catalog
            .models(key)
            .iter()
            .map(|m| {
                let n = m["name"].as_str().unwrap_or("?");
                if m["params_known"].as_bool() == Some(true) {
                    n.to_string()
                } else {
                    format!("{n} (no parameter data)")
                }
            })
            .collect();
        return Ok(json!({"block": key, "models": names}));
    }

    let mut guard = lock(state);
    let st = &mut *guard;
    match name {
        "load_patch" => {
            let slot = parse_code(arg_str(a, "patch")?)?;
            run(st, "/api/load", &[("slot", slot.to_string())])?;
            Ok(summary(st))
        }
        "get_patch" => {
            let w = working(st)?;
            let hidden = a
                .get("include_hidden")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let tones: Vec<Value> = match a.get("tone") {
                Some(_) => vec![tone_view(catalog, &w.patch, arg_tone(a)?, hidden)],
                None => (0..2)
                    .map(|t| tone_view(catalog, &w.patch, t, hidden))
                    .collect(),
            };
            Ok(json!({"patch": slot_code(w.slot), "unsaved_changes": w.dirty, "tones": tones}))
        }
        "set_model" => {
            let (tone, block) = (arg_tone(a)?, arg_block(a)?);
            let key = blob::RECORD_NAMES[block];
            let def = find_model(catalog, key, arg_str(a, "model")?)?;
            let rec = blob::record(&working(st)?.patch, tone, block).map_err(|e| e.to_string())?;
            let params: Vec<String> = def["params"]
                .as_array()
                .map(|ps| {
                    ps.iter()
                        .filter_map(|p| {
                            let id = p["id"].as_str()?;
                            let current = rec
                                .params
                                .iter()
                                .find(|c| format!("{:08x}", c.id) == id)
                                .map(|c| c.value as f64);
                            Some(format!("{id}:{}", current.or(p["default"].as_f64())?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            run(
                st,
                "/api/model",
                &[
                    ("tone", tone.to_string()),
                    ("block", block.to_string()),
                    ("category", def["category"].to_string()),
                    ("table", def["table"].to_string()),
                    ("model", def["model"].to_string()),
                    ("params", params.join(",")),
                ],
            )?;
            let w = working(st)?;
            Ok(
                json!({"block": key, "model": def["name"], "now": tone_view(catalog, &w.patch, tone, false)["chain"]
                .as_array().and_then(|c| c.iter().find(|b| b["block"] == key)).cloned()}),
            )
        }
        "set_params" => {
            let (tone, block) = (arg_tone(a)?, arg_block(a)?);
            let key = blob::RECORD_NAMES[block];
            let wanted = a
                .get("params")
                .and_then(Value::as_object)
                .ok_or("params must be an object of name: value")?;
            let rec = blob::record(&working(st)?.patch, tone, block).map_err(|e| e.to_string())?;
            let views = param_views(catalog, key, &rec);
            // Resolve everything first so a typo doesn't leave a half-applied edit.
            let mut sets = Vec::new();
            for (pname, v) in wanted {
                let shown = v
                    .as_f64()
                    .ok_or_else(|| format!("{pname}: value must be a number"))?;
                let lname = pname.to_ascii_lowercase();
                let view = views
                    .iter()
                    .find(|p| {
                        p.name.to_ascii_lowercase() == lname || format!("{:08x}", p.id) == lname
                    })
                    .ok_or_else(|| {
                        format!(
                            "{key} has no parameter '{pname}'; it has: {}",
                            views
                                .iter()
                                .map(|p| p.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                sets.push((view.id, view.to_raw(shown)?, view.name.clone(), shown));
            }
            let mut done = Map::new();
            for (id, raw, pname, shown) in sets {
                run(
                    st,
                    "/api/param",
                    &[
                        ("tone", tone.to_string()),
                        ("block", block.to_string()),
                        ("id", format!("{id:08x}")),
                        ("value", raw.to_string()),
                    ],
                )?;
                done.insert(pname, json!(shown));
            }
            Ok(json!({"block": key, "set": done}))
        }
        "set_block_enabled" => {
            let (tone, block) = (arg_tone(a)?, arg_block(a)?);
            let on = a
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("enabled must be true or false")?;
            run(
                st,
                "/api/enable",
                &[
                    ("tone", tone.to_string()),
                    ("block", block.to_string()),
                    ("on", on.to_string()),
                ],
            )?;
            Ok(json!({"block": blob::RECORD_NAMES[block], "enabled": on}))
        }
        "move_block" => {
            let (tone, block) = (arg_tone(a)?, arg_block(a)?);
            let key = blob::RECORD_NAMES[block];
            let positions = catalog
                .block(key)
                .and_then(|b| b["positions"].as_array())
                .ok_or_else(|| {
                    format!("{key} can't be moved; only volume, loop, mod, delay and reverb can")
                })?;
            let pos = arg_str(a, "position")?;
            let target = match pos {
                "pre" => &positions[0],
                "post" => &positions[1],
                _ => return Err("position must be 'pre' or 'post'".into()),
            };
            let rec = blob::record(&working(st)?.patch, tone, block).map_err(|e| e.to_string())?;
            if Some(rec.group as u64) == target[1].as_u64() {
                return Ok(json!({"block": key, "position": pos, "changed": false}));
            }
            run(
                st,
                "/api/move",
                &[
                    ("tone", tone.to_string()),
                    ("block", block.to_string()),
                    ("slot", target[0].to_string()),
                    ("group", target[1].to_string()),
                ],
            )?;
            Ok(json!({"block": key, "position": pos, "changed": true}))
        }
        "set_setting" => {
            let tone = arg_tone(a)?;
            let key = arg_str(a, "setting")?;
            let def = catalog
                .setting(key)
                .ok_or_else(|| format!("unknown setting '{key}'"))?;
            let value = &a["value"];
            let raw = match def["type"].as_str() {
                Some("enum") => {
                    let opts = def["options"].as_array().ok_or("bad catalog")?;
                    match value {
                        Value::String(s) => opts
                            .iter()
                            .find(|o| {
                                o["name"]
                                    .as_str()
                                    .is_some_and(|n| n.eq_ignore_ascii_case(s))
                            })
                            .and_then(|o| o["value"].as_f64())
                            .ok_or_else(|| {
                                format!(
                                    "{key} has no option '{s}'; options: {}",
                                    opts.iter()
                                        .filter_map(|o| o["name"].as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            })?,
                        v => v.as_f64().ok_or("value must be an option name or number")?,
                    }
                }
                Some("float") => value.as_f64().ok_or("value must be a number 0-100")? / 100.0,
                _ => value.as_f64().ok_or("value must be a number")?,
            };
            if let (Some(lo), Some(hi)) = (def["min"].as_f64(), def["max"].as_f64()) {
                if !(lo..=hi).contains(&raw) {
                    return Err(format!("{key} out of range"));
                }
            }
            run(
                st,
                "/api/setting",
                &[
                    ("tone", tone.to_string()),
                    ("key", key.to_string()),
                    ("value", raw.to_string()),
                ],
            )?;
            Ok(json!({"setting": key, "value": setting_display(catalog, key, raw)}))
        }
        "rename_tone" => {
            let tone = arg_tone(a)?;
            let new = arg_str(a, "name")?;
            run(
                st,
                "/api/rename",
                &[("tone", tone.to_string()), ("name", new.to_string())],
            )?;
            Ok(json!({"tone": tone + 1, "name": new}))
        }
        "select_tone" => {
            let tone = arg_tone(a)?;
            run(st, "/api/tone", &[("tone", tone.to_string())])?;
            Ok(json!({"tone": tone + 1}))
        }
        "save_patch" => {
            let current = working(st)?.slot;
            let slot = match a.get("patch") {
                Some(_) => parse_code(arg_str(a, "patch")?)?,
                None => current,
            };
            let overwrite = a.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
            if slot != current && !overwrite {
                if let Some(Some(existing)) = st.names.get(slot) {
                    return Err(format!(
                        "{} holds \"{existing}\"; pass overwrite: true to replace it",
                        slot_code(slot)
                    ));
                }
            }
            run(st, "/api/save", &[("slot", slot.to_string())])?;
            Ok(
                json!({"saved_to": slot_code(slot), "name": blob::tone_name(&working(st)?.patch, 0)}),
            )
        }
        "revert_patch" => {
            run(st, "/api/revert", &[])?;
            Ok(summary(st))
        }
        _ => Err(format!("unknown tool '{name}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_codes_parse() {
        assert_eq!(parse_code("01A"), Ok(0));
        assert_eq!(parse_code("8c"), Ok(30));
        assert_eq!(parse_code("16D"), Ok(63));
        assert!(parse_code("17A").is_err());
        assert!(parse_code("08E").is_err());
        assert!(parse_code("").is_err());
    }

    #[test]
    fn origins() {
        assert!(origin_allowed("http://localhost:3000", None));
        assert!(origin_allowed(
            "http://172.16.88.33:8080",
            Some("172.16.88.33:8080")
        ));
        assert!(!origin_allowed(
            "https://evil.example",
            Some("172.16.88.33:8080")
        ));
    }

    #[test]
    fn param_view_units() {
        let knob = ParamView {
            id: 0,
            name: "Drive".into(),
            normalized: true,
            min: 0.0,
            max: 1.0,
            unit: String::new(),
            hidden: false,
        };
        assert_eq!(knob.to_raw(50.0), Ok(0.5));
        assert!(knob.to_raw(101.0).is_err());
        assert_eq!(knob.display(0.333), 33.3);
        let db = ParamView {
            normalized: false,
            min: -96.0,
            max: 0.0,
            unit: "dB".into(),
            ..knob
        };
        assert_eq!(db.to_raw(-40.0), Ok(-40.0));
        assert!(db.to_raw(3.0).is_err());
    }

    #[test]
    fn rpc_basics() {
        let state: Shared = Default::default();
        let cat = Catalog(json!({"blocks": [], "settings": []}));
        let init = dispatch(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": "2025-03-26"}}),
            &state,
            &cat,
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(dispatch(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &state,
            &cat
        )
        .is_none());
        let list = dispatch(
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            &state,
            &cat,
        )
        .unwrap();
        assert!(list["result"]["tools"].as_array().unwrap().len() >= 10);
        let bad = dispatch(
            &json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
            &state,
            &cat,
        )
        .unwrap();
        assert_eq!(bad["error"]["code"], -32601);
    }
}
