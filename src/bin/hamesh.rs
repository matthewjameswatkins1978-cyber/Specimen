//! hamesh CLI: the external Eye and Hand.
//!
//! A separate process drives (or inspects) a running Hamesh application
//! over the local inspector protocol — no shared memory, no source
//! imports, no special knowledge of the app beyond what the protocol
//! reveals. This is the decisive Hamesh exercise: an unfamiliar agent
//! uses only this binary.
//!
//! Usage: hamesh [--port P] [--token T] <op> [args...]
//!   discovery (default when --port is absent): target/hamesh-inspect.json
//! Ops: map | detail <id> | click <id|x,y> | key <Key...> | type <id> <text...>
//!      | snapshot [path] [overlay-id] | wait [rev] [timeout_ms]
//#! snapshot with overlay-id paints that component's diagnostic guides
//! into the capture only (bounds/content/trim/ink/baseline/centres).
//! Reconciled with upstream hameshgui@c5b794c3 (was 9fa37c33).
//! Keys: Tab ShiftTab Enter Escape Backspace Delete Left Right Up Down
//!       Home End Char:X (e.g. Char:a; Char:, is a comma)
//!
//! Exit codes: 0 ok · 1 transport/protocol error · 2 stale/denied (retry
//! after re-inspecting) · 3 usage error.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn usage() -> ! {
    eprintln!(
        "usage: hamesh [--port P] [--token T] <op> [args...]\n\
         ops: map | detail <id> | click <id|x,y> | key <Key...> | type <id> <text...> |\n     snapshot [path] | wait [rev] [timeout_ms]"
    );
    std::process::exit(3);
}

fn read_discovery() -> Option<(u16, String)> {
    let raw = std::fs::read_to_string("target/hamesh-inspect.json").ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some((
        v.get("port")?.as_u64()? as u16,
        v.get("token")?.as_str()?.to_string(),
    ))
}

fn main() {
    let mut argv = std::env::args().skip(1).peekable();
    let mut port: Option<u16> = None;
    let mut token: Option<String> = None;
    let mut expect_rev: Option<u64> = None;
    while let Some(a) = argv.peek() {
        if *a == "--port" {
            argv.next();
            port = argv.next().and_then(|p| p.parse().ok());
        } else if *a == "--token" {
            argv.next();
            token = argv.next();
        } else if *a == "--expect-rev" {
            argv.next();
            expect_rev = argv.next().and_then(|r| r.parse().ok());
        } else {
            break;
        }
    }
    if port.is_none() || token.is_none() {
        if let Some((p, t)) = read_discovery() {
            if port.is_none() {
                port = Some(p);
            }
            if token.is_none() {
                token = Some(t);
            }
        }
    }
    let (port, token) = match (port, token) {
        (Some(p), Some(t)) => (p, t),
        _ => {
            eprintln!("no inspector found: pass --port/--token or run the app with --inspect");
            std::process::exit(3);
        }
    };
    let op = argv.next().unwrap_or_else(|| usage());
    let rest: Vec<String> = argv.collect();

    let (op_name, params) = build_op(&op, &rest, expect_rev);
    let req = serde_json::json!({"id": 1, "token": token, "op": op_name, "params": params});

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap_or_else(|e| {
        eprintln!("connect 127.0.0.1:{port} failed: {e}");
        std::process::exit(1);
    });
    stream
        .write_all(serde_json::to_string(&req).unwrap().as_bytes())
        .unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap_or_else(|e| {
        eprintln!("read failed: {e}");
        std::process::exit(1);
    });
    let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| {
        eprintln!("bad response: {e}: {line}");
        std::process::exit(1);
    });
    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
    let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        return;
    }
    let code = resp.get("code").and_then(|v| v.as_str()).unwrap_or("");
    std::process::exit(if code == "STALE" || code == "DENIED" {
        2
    } else {
        1
    });
}

fn build_op<'a>(
    op: &'a str,
    rest: &'a [String],
    expect_rev: Option<u64>,
) -> (&'a str, serde_json::Value) {
    use serde_json::json;
    let stale = |mut p: serde_json::Value| {
        if let (Some(map), Some(r)) = (p.as_object_mut(), expect_rev) {
            map.insert("expect_rev".to_string(), json!(r));
        }
        p
    };
    match op {
        "map" => ("map", json!({})),
        "detail" => (
            "detail",
            json!({"id": rest.first().unwrap_or_else(|| usage())}),
        ),
        "click" => {
            let target = rest.first().unwrap_or_else(|| usage());
            if let Some((x, y)) = target.split_once(',') {
                (
                    "click_at",
                    stale(json!({
                        "x": x.parse::<f64>().unwrap_or_else(|_| usage()),
                        "y": y.parse::<f64>().unwrap_or_else(|_| usage()),
                    })),
                )
            } else {
                ("click_id", stale(json!({"id": target})))
            }
        }
        "key" => {
            // One key per call (matches the protocol vocabulary); sequences
            // are separate invocations so each gets its own revision stamp.
            let k = rest.first().unwrap_or_else(|| usage());
            ("key", stale(json!({"key": k})))
        }
        "type" => {
            if rest.len() < 2 {
                usage();
            }
            ("type", json!({"id": rest[0], "text": rest[1..].join(" ")}))
        }
        // Reconciled with upstream hameshgui@c5b794c3 (was 9fa37c33).
        "snapshot" => {
            let mut v = json!({"path": rest.first().cloned().unwrap_or_else(|| "target/hamesh-ext-snapshot.png".to_string())});
            if let Some(id) = rest.get(1) {
                v["overlay"] = serde_json::Value::String(id.clone());
            }
            ("snapshot", v)
        }
        "wait" => (
            "wait",
            json!({
                "app_rev": rest.first().and_then(|r| r.parse::<u64>().ok()).unwrap_or(0),
                "timeout_ms": rest.get(1).and_then(|t| t.parse::<u64>().ok()).unwrap_or(5000),
            }),
        ),
        _ => usage(),
    }
}
