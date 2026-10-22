//! External inspector protocol: separate-process Eye and Hand.
//!
//! A TCP loopback server (127.0.0.1 only, token-gated, dev-only) exposes
//! the SAME operations the in-process Hand uses — no parallel semantics.
//! The trick that keeps one implementation: the server thread never touches
//! application state. It forwards each request as a winit user event, and
//! the UI thread applies it with the ordinary handlers, then replies
//! through a per-request channel.
//!
//! Wire format is JSONL (one object per line, like the Pi RPC):
//!   -> {"id":1,"token":"...","op":"map","params":{}}
//!   <- {"id":1,"ok":true,"app_rev":12,"scene_rev":3,"result":{...}}
//! Every response carries the revisions it was computed at. Mutating ops
//! accept `expect_rev`: when the app moved since the agent last looked,
//! the server refuses with code STALE instead of acting on a dead layout.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::time::Duration;

pub const DISCOVERY_PATH: &str = "target/hamesh-inspect.json";

/// What the UI thread computes for one request.
#[derive(Debug)]
pub enum OpOutcome {
    Ok(serde_json::Value),
    Err {
        code: &'static str,
        message: String,
        /// Revision at refusal time, so the agent can re-inspect.
        /// Server-side errors (bad token, bad JSON) carry 0.
        app_rev: u64,
        scene_rev: u64,
    },
}

/// One inspector operation. UI-thread owned; cheap to match.
#[derive(Debug)]
pub enum Op {
    Map,
    Detail {
        id: String,
    },
    ClickId {
        id: String,
        expect_rev: Option<u64>,
    },
    ClickAt {
        x: f32,
        y: f32,
        expect_rev: Option<u64>,
    },
    Key {
        key: KeyMsg,
        expect_rev: Option<u64>,
    },
    Type {
        id: String,
        text: String,
    },
    Snapshot {
        path: String,
    },
    Wait {
        app_rev: u64,
        timeout_ms: u64,
    },
}

/// Keyboard input over the wire. `Char` carries one character.
#[derive(Debug, Clone)]
pub enum KeyMsg {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    ShiftTab,
    Escape,
}

impl KeyMsg {
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(c) = s.strip_prefix("Char:") {
            return c.chars().next().map(KeyMsg::Char);
        }
        Some(match s {
            "Enter" => Self::Enter,
            "Backspace" => Self::Backspace,
            "Delete" => Self::Delete,
            "Left" => Self::Left,
            "Right" => Self::Right,
            "Up" => Self::Up,
            "Down" => Self::Down,
            "Home" => Self::Home,
            "End" => Self::End,
            "Tab" => Self::Tab,
            "ShiftTab" => Self::ShiftTab,
            "Escape" => Self::Escape,
            _ => return None,
        })
    }
}

/// A request travelling UI-ward with its reply channel.
pub struct OpRequest {
    pub op: Op,
    pub respond: mpsc::Sender<OpOutcome>,
}

impl std::fmt::Debug for OpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpRequest").field("op", &self.op).finish()
    }
}

fn parse_op(v: &serde_json::Value) -> Result<Op, String> {
    let op = v
        .get("op")
        .and_then(|o| o.as_str())
        .ok_or_else(|| "missing op".to_string())?;
    let p = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
    let get = |k: &str| p.get(k).cloned();
    let expect_rev = get("expect_rev").and_then(|r| r.as_u64());
    match op {
        "map" => Ok(Op::Map),
        "detail" => Ok(Op::Detail {
            id: get("id")
                .and_then(|i| i.as_str().map(str::to_string))
                .ok_or("detail needs id")?,
        }),
        "click_id" => Ok(Op::ClickId {
            id: get("id")
                .and_then(|i| i.as_str().map(str::to_string))
                .ok_or("click_id needs id")?,
            expect_rev,
        }),
        "click_at" => Ok(Op::ClickAt {
            x: get("x")
                .and_then(|x| x.as_f64())
                .ok_or("click_at needs x")? as f32,
            y: get("y")
                .and_then(|y| y.as_f64())
                .ok_or("click_at needs y")? as f32,
            expect_rev,
        }),
        "key" => Ok(Op::Key {
            key: KeyMsg::parse(
                get("key")
                    .and_then(|k| k.as_str().map(str::to_string))
                    .as_deref()
                    .unwrap_or(""),
            )
            .ok_or("key needs key (Tab|Enter|Char:X|...), see KeyMsg")?,
            expect_rev,
        }),
        "type" => Ok(Op::Type {
            id: get("id")
                .and_then(|i| i.as_str().map(str::to_string))
                .ok_or("type needs id")?,
            text: get("text")
                .and_then(|t| t.as_str().map(str::to_string))
                .unwrap_or_default(),
        }),
        "snapshot" => Ok(Op::Snapshot {
            path: get("path")
                .and_then(|p| p.as_str().map(str::to_string))
                .unwrap_or_else(|| "target/hamesh-ext-snapshot.png".to_string()),
        }),
        "wait" => Ok(Op::Wait {
            app_rev: get("app_rev").and_then(|r| r.as_u64()).unwrap_or(0),
            timeout_ms: get("timeout_ms").and_then(|t| t.as_u64()).unwrap_or(5000),
        }),
        other => Err(format!("unknown op: {other}")),
    }
}

/// Serve one listener forever. `send` forwards each validated request to
/// the UI thread; `allow` decides which peer addresses may connect
/// (default: loopback only — pass an explicit predicate to widen it,
/// which the dev server never does).
pub fn serve<F>(listener: std::net::TcpListener, token: String, send: F)
where
    F: FnMut(OpRequest) -> bool + Send + 'static,
{
    // One thread per connection: a parked `wait` must never starve other
    // ops — the standing drill caught exactly that (wait blocked the
    // only server thread, so the change it waited for could never arrive
    // through the inspector). The UI thread stays the serializer:
    // check_stale and apply both run there, in send order, so rev
    // discipline is unchanged. Per-connection request order is preserved
    // because each connection keeps its own sequential loop.
    let send = std::sync::Arc::new(std::sync::Mutex::new(send));
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let peer = stream.peer_addr().ok();
        if !peer.map(|p| p.ip().is_loopback()).unwrap_or(false) {
            continue;
        }
        let token = token.clone();
        let send = std::sync::Arc::clone(&send);
        std::thread::spawn(move || {
            // One TcpStream per direction; try_clone on a fresh accept does
            // not fail in practice, but a failed split must drop the
            // connection rather than mangle framing.
            let reader_stream = match stream.try_clone() {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut reader = BufReader::new(reader_stream);
            let mut line = String::new();
            // One request per line; the connection stays open for a session.
            loop {
                line.clear();
                let n = match reader.read_line(&mut line) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                let req: serde_json::Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = respond_err(
                            &mut stream,
                            0,
                            "PROTOCOL",
                            &format!("bad json: {e}"),
                            0,
                            0,
                        );
                        continue;
                    }
                };
                let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                if req.get("token").and_then(|t| t.as_str()) != Some(token.as_str()) {
                    let _ = respond_err(&mut stream, id, "DENIED", "bad token", 0, 0);
                    continue;
                }
                let op = match parse_op(&req) {
                    Ok(o) => o,
                    Err(e) => {
                        let _ = respond_err(&mut stream, id, "BAD_OP", &e, 0, 0);
                        continue;
                    }
                };
                let (tx, rx) = mpsc::channel::<OpOutcome>();
                let sent = send
                    .lock()
                    .map(|mut g| g(OpRequest { op, respond: tx }))
                    .unwrap_or(false);
                if !sent {
                    let _ = respond_err(&mut stream, id, "BUSY", "ui thread gone", 0, 0);
                    break;
                }
                match rx.recv_timeout(Duration::from_secs(30)) {
                    Ok(OpOutcome::Ok(result)) => {
                        let _ = respond_ok(&mut stream, id, result);
                    }
                    Ok(OpOutcome::Err {
                        code,
                        message,
                        app_rev,
                        scene_rev,
                    }) => {
                        let _ = respond_err(&mut stream, id, code, &message, app_rev, scene_rev);
                    }
                    Err(_) => {
                        let _ = respond_err(&mut stream, id, "TIMEOUT", "ui busy", 0, 0);
                    }
                }
            }
        });
    }
}

fn respond_ok(
    stream: &mut std::net::TcpStream,
    id: u64,
    result: serde_json::Value,
) -> std::io::Result<()> {
    // Revisions are filled in by the UI thread inside `result` when it
    // knows them (map/detail include app_rev/scene_rev); the envelope
    // carries them again for uniform stale-checks.
    let app_rev = result.get("app_rev").and_then(|v| v.as_u64()).unwrap_or(0);
    let scene_rev = result
        .get("scene_rev")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let line = serde_json::json!({
        "id": id, "ok": true,
        "app_rev": app_rev, "scene_rev": scene_rev,
        "result": result,
    });
    stream.write_all(serde_json::to_string(&line).unwrap().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn respond_err(
    stream: &mut std::net::TcpStream,
    id: u64,
    code: &str,
    message: &str,
    app_rev: u64,
    scene_rev: u64,
) -> std::io::Result<()> {
    let line = serde_json::json!({
        "id": id, "ok": false, "code": code, "error": message,
        "app_rev": app_rev, "scene_rev": scene_rev,
    });
    stream.write_all(serde_json::to_string(&line).unwrap().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

/// Dev-grade token: NOT a security boundary, just a shared secret so stray
/// local processes don't accidentally drive the UI. Production would use
/// OS access controls (named-pipe ACLs / unix-socket permissions).
pub fn dev_token() -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (std::process::id(), std::time::SystemTime::now()).hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Write the discovery file the CLI reads: 127.0.0.1 port + token.
pub fn write_discovery(port: u16, token: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(DISCOVERY_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        DISCOVERY_PATH,
        serde_json::json!({
            "host": "127.0.0.1",
            "port": port,
            "token": token,
            "pid": std::process::id(),
        })
        .to_string(),
    )
}
