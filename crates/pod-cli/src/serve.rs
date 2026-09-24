//! A tiny hand-rolled HTTP server for watching real device data live in a
//! browser, without pulling in a web framework for what's meant to be a
//! quick, disposable view. GET-only, one request at a time — this is a
//! dev/demo tool, not something to expose beyond a trusted LAN.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use pod_core::PodDevice;

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>POD Controller — live</title>
<style>
  body { margin: 0; padding: 32px; background: #16181d; color: #e8e6e1; font-family: system-ui, sans-serif; }
  h1 { font-size: 18px; margin: 0 0 4px; }
  .sub { color: #8a8f98; font-size: 13px; margin-bottom: 24px; }
  .status { display: inline-flex; align-items: center; gap: 8px; padding: 6px 12px; border-radius: 20px; background: #1f2937; font-size: 13px; margin-bottom: 20px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #4b5563; }
  .status.ok .dot { background: #34d399; }
  .status.err .dot { background: #f87171; }
  table { border-collapse: collapse; width: 100%; max-width: 480px; }
  td { padding: 8px 12px; border-bottom: 1px solid #262a33; font-size: 14px; }
  td.code { font-family: monospace; color: #8a8f98; width: 60px; }
  .hint { margin-top: 20px; color: #8a8f98; font-size: 12px; }
</style>
</head>
<body>
<h1>POD Controller — live</h1>
<div class="sub">Polling the real device every 3s. Bank set by <code>--bank</code> on <code>pod-cli serve</code>.</div>
<div id="status" class="status"><span class="dot"></span><span>connecting…</span></div>
<table id="patches"></table>
<div class="hint">Rename a patch on the unit's front panel and this should update on the next poll.</div>
<script>
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
      `<tr><td class="code">${p.code}</td><td>${p.name || '(read error)'}</td></tr>`
    ).join('');
  } catch (e) {
    statusEl.className = 'status err';
    statusEl.querySelector('span:last-child').textContent = 'poll failed: ' + e;
  }
}
poll();
setInterval(poll, 3000);
</script>
</body>
</html>"#;

fn escape_json(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            _ => vec![c],
        })
        .collect()
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

/// Live-reads the tone name for each slot in `bank` (1-32) straight off the
/// device. Non-destructive (read-only), but re-opens the device per call —
/// don't poll faster than a human would refresh a page.
fn patches_json(bank: u8) -> String {
    let mut dev = PodDevice::open_first().ok();
    let entries: Vec<String> = (0..4u8)
        .map(|i| {
            let slot = bank.saturating_sub(1) * 4 + i;
            let code = format!("{bank:02}{}", (b'A' + i) as char);
            let name = match dev.as_mut() {
                Some(d) => match d.read_patch(slot) {
                    Ok(patch) => pod_core::blob::tone_name(&patch, pod_core::blob::TONE1_NAME_OFFSET),
                    Err(e) => format!("error: {e}"),
                },
                None => "no device".to_string(),
            };
            format!(
                r#"{{"slot":{slot},"code":"{code}","name":"{}"}}"#,
                escape_json(&name)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

fn handle(mut stream: TcpStream, bank: u8) {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/");

    let (status, content_type, body) = match path {
        "/" => ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string()),
        "/api/status" => ("200 OK", "application/json", status_json()),
        "/api/patches" => ("200 OK", "application/json", patches_json(bank)),
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
    for stream in listener.incoming() {
        match stream {
            Ok(s) => handle(s, bank),
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
    Ok(())
}
