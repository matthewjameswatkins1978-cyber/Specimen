#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Native Hamesh host for Specimen. UI and external events share Editor.
mod artwork;
mod document;
mod editor;
#[cfg(test)]
mod editor_tests;
mod inspect_srv;

use anyhow::{bail, Context, Result};
use editor::Editor;
use inspect_srv::{KeyMsg, Op, OpOutcome, OpRequest};
use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

#[derive(Debug)]
pub enum UserEvent {
    Inspect(OpRequest),
}
struct Pending {
    rev: u64,
    deadline: Instant,
    respond: Sender<OpOutcome>,
}
struct Host {
    editor: Editor,
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    size: (u32, u32),
    cursor: (f32, f32),
    modifiers: ModifiersState,
    proxy: EventLoopProxy<UserEvent>,
    trace: hamesh_diag::Trace,
    pending: Vec<Pending>,
    deadline: Option<Instant>,
    evidence: PathBuf,
    frame: Option<tiny_skia::Pixmap>,
    inspect: bool,
}
impl Host {
    fn view(&self) -> editor::View {
        self.editor.view(self.size.0, self.size.1)
    }
    fn ok(&self, mut v: serde_json::Value) -> OpOutcome {
        if let Some(o) = v.as_object_mut() {
            o.insert("app_rev".into(), self.editor.revision.into());
            o.insert("scene_rev".into(), self.editor.revision.into());
        }
        OpOutcome::Ok(v)
    }
    fn err(&self, code: &'static str, error: impl ToString) -> OpOutcome {
        OpOutcome::Err {
            code,
            message: error.to_string(),
            app_rev: self.editor.revision,
            scene_rev: self.editor.revision,
        }
    }
    fn stale(&self, rev: Option<u64>) -> Option<OpOutcome> {
        rev.filter(|r| *r != self.editor.revision).map(|r| {
            self.err(
                "STALE",
                format!("Expected {r}, live {}. Reinspect.", self.editor.revision),
            )
        })
    }
    fn record(&mut self, kind: &str, message: &str) {
        self.trace.record(kind, message, self.editor.revision, None);
        if let Err(e) = std::fs::write(
            self.evidence.join("interaction-trace.json"),
            self.trace.export_json(),
        ) {
            eprintln!("trace: {e}");
        }
    }
    fn report(&mut self, r: Result<()>) {
        if let Err(e) = r {
            self.editor.status = format!("{e:#}");
            self.editor.revision += 1;
            self.record("error", &e.to_string());
        }
        if let Some(w) = &self.window {
            w.request_redraw()
        }
    }
    // Overlay reconciliation (upstream hameshgui@c5b794c3): with an
    // overlay id, paint that component's diagnostic guides into this
    // capture only and report its resolved geometry. Normal rendering,
    // state and hit-testing are untouched.
    fn capture(&mut self, path: &Path, overlay: Option<&str>) -> Result<Option<serde_json::Value>> {
        if let Some(p) = path.parent() {
            if !p.as_os_str().is_empty() {
                std::fs::create_dir_all(p)?;
            }
        }
        let view = self.view();
        let mut pix = self.editor.render(&view)?;
        let geom = overlay.and_then(|id| {
            view.scene.nodes.iter().find(|n| n.id == id).map(|n| {
                hamesh_render::overlay::draw_guides(&mut pix, n);
                serde_json::json!({
                    "id": n.id,
                    "rect": n.rect,
                    "text_geom": {
                        "origin": n.origin,
                        "content": n.content,
                        "halign": n.halign,
                        "advance": n.advance,
                        "ink": n.ink,
                        "trim": n.trim,
                        "metrics": n.metrics,
                        "baseline": n.baseline,
                        "lines": n.lines,
                    },
                })
            })
        });
        if overlay.is_some() && geom.is_none() {
            bail!("no scene node for overlay {}", overlay.unwrap_or("?"));
        }
        pix.save_png(path)?;
        let mut side = path.as_os_str().to_os_string();
        side.push(".json");
        std::fs::write(
            PathBuf::from(side),
            serde_json::to_vec_pretty(&serde_json::json!({
                "app_revision":self.editor.revision,"scene_revision":self.editor.revision,
                "viewport":[self.size.0,self.size.1],"paper":view.paper,"scale":view.scale,
                "document":self.editor.history.document,"renderer":"Hamesh CPU chrome + canonical Parley/Swash artwork",
                "text_diagnostics":artwork::inspect(&self.editor.history.document)?
            }))?,
        )?;
        Ok(geom)
    }
    fn finish(&mut self) {
        let p = self.evidence.join("specimen-window.png");
        if let Err(e) = self.capture(&p, None) {
            self.record("capture-failed", &e.to_string())
        }
        if self.editor.dirty() {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let recovery = self.evidence.join(format!(
                "recovery-{timestamp}-{}.specimen.json",
                std::process::id()
            ));
            if let Err(e) = self.editor.history.document.save(&recovery) {
                self.record("recovery-failed", &e.to_string())
            }
        }
        self.record(
            "window.close",
            "Native window closed; current frame and trace retained.",
        );
    }
    fn press(&mut self, x: f32, y: f32, release: bool) -> Result<Option<String>> {
        let mut view = self.view();
        let hit = self.editor.press(&mut view, x, y)?;
        if release {
            self.editor.release()?;
        }
        Ok(hit)
    }
    fn apply(&mut self, req: OpRequest) {
        let OpRequest { op, respond } = req;
        let revision_before = self.editor.revision;
        let reply = match op {
            Op::Map => {
                let view = self.view();
                let map = hamesh_inspect::compact_map(&view.app, &view.layout, &view.scene);
                self.ok(serde_json::to_value(map).unwrap())
            }
            Op::Detail { id } => {
                let view = self.view();
                match hamesh_inspect::detail(&view.app, &view.layout, Some(&view.scene), &id) {
                    Some(d) => self.ok(serde_json::to_value(d).unwrap()),
                    None => self.err("NOT_FOUND", format!("Unknown component {id}")),
                }
            }
            Op::ClickId { id, expect_rev } => {
                if let Some(e) = self.stale(expect_rev) {
                    e
                } else {
                    let target_view = self.view();
                    let actionable = target_view.app.get(&id).is_some_and(|c| {
                        c.enabled
                            && (matches!(
                                c.role,
                                hamesh_core::Role::Button | hamesh_core::Role::TextInput
                            ) || id == "canvas")
                    });
                    let before_document = self.editor.fingerprint();
                    let center = target_view
                        .layout
                        .get(&id)
                        .map(|n| (n.rect.x + n.rect.w / 2.0, n.rect.y + n.rect.h / 2.0));
                    match center {
                        None => self.err("NOT_FOUND", id),
                        Some((x, y)) => match self.press(x, y, true) {
                            Ok(hit) => {
                                self.record(
                                    "ext.click",
                                    &format!("HumanEquivalent click_id={id} hit={hit:?}"),
                                );
                                self.ok(serde_json::json!({"mechanism":"HumanEquivalent","hit":hit,"acted":actionable && hit.as_deref()==Some(&id),"document_changed":before_document!=self.editor.fingerprint(),"focus":self.editor.focus}))
                            }
                            Err(e) => self.err("BAD_OP", e),
                        },
                    }
                }
            }
            Op::ClickAt { x, y, expect_rev } => {
                if let Some(e) = self.stale(expect_rev) {
                    e
                } else {
                    match self.press(x, y, true) {
                        Ok(hit) => {
                            self.record(
                                "ext.click",
                                &format!("HumanEquivalent click_at=({x},{y}) hit={hit:?}"),
                            );
                            self.ok(serde_json::json!({"mechanism":"HumanEquivalent","hit":hit,"focus":self.editor.focus}))
                        }
                        Err(e) => self.err("BAD_OP", e),
                    }
                }
            }
            Op::Key { key, expect_rev } => {
                if let Some(e) = self.stale(expect_rev) {
                    e
                } else {
                    let name = match key {
                        KeyMsg::Char(c) => format!("Char:{c}"),
                        KeyMsg::Enter => "Enter".into(),
                        KeyMsg::Backspace => "Backspace".into(),
                        KeyMsg::Delete => "Delete".into(),
                        KeyMsg::Left => "Left".into(),
                        KeyMsg::Right => "Right".into(),
                        KeyMsg::Up => "Up".into(),
                        KeyMsg::Down => "Down".into(),
                        KeyMsg::Home => "Home".into(),
                        KeyMsg::End => "End".into(),
                        KeyMsg::Tab => "Tab".into(),
                        KeyMsg::ShiftTab => "ShiftTab".into(),
                        KeyMsg::Escape => "Escape".into(),
                    };
                    let view = self.view();
                    match self.editor.key(&view, &name) {
                        Ok(()) => {
                            self.record("ext.key", &format!("HumanEquivalent {name}"));
                            self.ok(serde_json::json!({"mechanism":"HumanEquivalent","key":name,"focus":self.editor.focus}))
                        }
                        Err(e) => self.err("BAD_OP", e),
                    }
                }
            }
            Op::Type { id, text } => {
                let view = self.view();
                let target = view.app.get(&id);
                if !target.is_some_and(|c| c.role == hamesh_core::Role::TextInput) {
                    self.err("BAD_OP", "type requires an ordinary text input")
                } else {
                    let r = view.layout.get(&id).unwrap().rect.clone();
                    match self.press(r.x + r.w / 2.0, r.y + r.h / 2.0, true) {
                        Ok(Some(hit)) if hit == id => match self.editor.insert(&text) {
                            Ok(()) => {
                                self.record("ext.type",&format!("HumanEquivalent focused={id}; {} characters via shared Composer path",text.chars().count()));
                                self.ok(serde_json::json!({"mechanism":"HumanEquivalent","hit":hit,"characters":text.chars().count(),"commit":"Press Enter"}))
                            }
                            Err(e) => self.err("BAD_OP", e),
                        },
                        Ok(hit) => self.err(
                            "BAD_OP",
                            format!("Pointer reached {hit:?}, not requested field"),
                        ),
                        Err(e) => self.err("BAD_OP", e),
                    }
                }
            }
            Op::Snapshot { path, overlay } => {
                match self.capture(Path::new(&path), overlay.as_deref()) {
                    Ok(geom) => {
                        self.record("eye.snapshot", &path);
                        self.ok(serde_json::json!({"path":path,"renderer":"CPU","overlay":geom}))
                    }
                    Err(e) => self.err("BAD_OP", e),
                }
            }
            Op::Wait {
                app_rev,
                timeout_ms,
            } => {
                if self.editor.revision != app_rev {
                    self.ok(serde_json::json!({"waited":false}))
                } else {
                    self.pending.push(Pending {
                        rev: app_rev,
                        deadline: Instant::now() + Duration::from_millis(timeout_ms.min(25000)),
                        respond,
                    });
                    return;
                }
            }
        };
        let _ = respond.send(reply);
        if self.editor.revision != revision_before {
            if let Some(w) = &self.window {
                w.request_redraw()
            }
        }
    }
    fn key_native(&mut self, key: Key, text: Option<&str>) -> Result<()> {
        if self.modifiers.shift_key()
            && key == Key::Named(NamedKey::Enter)
            && self.editor.active.as_deref() == Some("text")
        {
            return self.editor.insert("\n");
        }
        if self.modifiers.control_key() {
            if let Key::Character(c) = &key {
                match c.to_ascii_lowercase().as_str() {
                    "s" => {
                        self.editor.commit_field()?;
                        return self.editor.action("save");
                    }
                    "o" => {
                        self.editor.commit_field()?;
                        return self.editor.action("open");
                    }
                    "e" => {
                        self.editor.commit_field()?;
                        return self.editor.action("export");
                    }
                    "z" => {
                        self.editor.active = None;
                        return self.editor.action(if self.modifiers.shift_key() {
                            "redo"
                        } else {
                            "undo"
                        });
                    }
                    "y" => {
                        self.editor.active = None;
                        return self.editor.action("redo");
                    }
                    "v" if self.editor.active.is_some() => {
                        let value = arboard::Clipboard::new()?.get_text()?;
                        return self.editor.insert(&value);
                    }
                    "c" if self.editor.active.is_some() => {
                        arboard::Clipboard::new()?.set_text(self.editor.composer.text.clone())?;
                        return Ok(());
                    }
                    "x" if self.editor.active.is_some() => {
                        arboard::Clipboard::new()?.set_text(self.editor.composer.text.clone())?;
                        let view = self.view();
                        self.editor.key(&view, "SelectAll")?;
                        return self.editor.key(&view, "Delete");
                    }
                    "a" => {
                        let view = self.view();
                        return self.editor.key(&view, "SelectAll");
                    }
                    _ => return Ok(()),
                }
            }
        }
        let name = match key {
            Key::Named(NamedKey::Enter) => Some("Enter"),
            Key::Named(NamedKey::Escape) => Some("Escape"),
            Key::Named(NamedKey::Backspace) => Some("Backspace"),
            Key::Named(NamedKey::Delete) => Some("Delete"),
            Key::Named(NamedKey::ArrowLeft) => Some("Left"),
            Key::Named(NamedKey::ArrowRight) => Some("Right"),
            Key::Named(NamedKey::ArrowUp) => Some("Up"),
            Key::Named(NamedKey::ArrowDown) => Some("Down"),
            Key::Named(NamedKey::Home) => Some("Home"),
            Key::Named(NamedKey::End) => Some("End"),
            Key::Named(NamedKey::Tab) => Some(if self.modifiers.shift_key() {
                "ShiftTab"
            } else {
                "Tab"
            }),
            _ => None,
        };
        if let Some(name) = name {
            let view = self.view();
            self.editor.key(&view, name)?;
        } else if let Some(t) = text {
            if self.editor.active.is_some() && !t.chars().any(|c| c.is_control()) {
                self.editor.insert(t)?;
            }
        }
        Ok(())
    }
}
impl ApplicationHandler<UserEvent> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Specimen - Typography & composition")
            .with_inner_size(PhysicalSize::new(1440, 980))
            .with_min_inner_size(PhysicalSize::new(1100, 820));
        let window = Arc::new(event_loop.create_window(attrs).expect("native window"));
        window.set_ime_allowed(true);
        let size = window.inner_size();
        self.size = (size.width, size.height);
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window.clone());
        if self.inspect {
            match std::net::TcpListener::bind("127.0.0.1:0") {
                Ok(listener) => {
                    let port = listener.local_addr().unwrap().port();
                    let token = inspect_srv::dev_token();
                    inspect_srv::write_discovery(port, &token).expect("discovery");
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        inspect_srv::serve(listener, token, move |req| {
                            proxy.send_event(UserEvent::Inspect(req)).is_ok()
                        })
                    });
                    self.record(
                        "inspect.ready",
                        &format!("Upstream Hamesh inspector transport on 127.0.0.1:{port}"),
                    );
                }
                Err(e) => self.record("inspect.error", &e.to_string()),
            }
        }
        self.record(
            "window.open",
            &format!("{}x{} native Hamesh CPU", self.size.0, self.size.1),
        );
        window.request_redraw();
    }
    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Inspect(req) => self.apply(req),
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.finish();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.size = (size.width.max(1), size.height.max(1));
                self.editor.revision += 1;
                self.record("window.resize", &format!("{}x{}", self.size.0, self.size.1));
                if let Some(w) = &self.window {
                    w.request_redraw()
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if !self.editor.dragging() {
                    return;
                }
                let scale = self.view().scale;
                let before = self.editor.revision;
                self.editor
                    .pointer_move(self.cursor.0, self.cursor.1, scale);
                if before != self.editor.revision {
                    if let Some(w) = &self.window {
                        w.request_redraw()
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let r = if state == ElementState::Pressed {
                    self.press(self.cursor.0, self.cursor.1, false).map(|_| ())
                } else {
                    self.editor.release()
                };
                self.record(
                    "hand.pointer",
                    if state == ElementState::Pressed {
                        "Physical press"
                    } else {
                        "Physical release"
                    },
                );
                self.report(r);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if self.cursor.0 < 220.0 {
                    let r = self
                        .editor
                        .action(if dy > 0.0 { "layers-up" } else { "layers-down" });
                    self.report(r);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let r = self.key_native(event.logical_key, event.text.as_deref());
                self.record("hand.key", "Physical keyboard via shared Editor handlers");
                self.report(r);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.editor.active.is_some() {
                    self.editor.composer.clear_preedit();
                    let r = self.editor.insert(&text);
                    self.report(r);
                }
            }
            WindowEvent::Ime(Ime::Preedit(text, ..)) => {
                self.editor.composer.set_preedit(&text);
                self.editor.revision += 1;
                if let Some(w) = &self.window {
                    w.request_redraw()
                }
            }
            WindowEvent::RedrawRequested => {
                let view = self.view();
                match self.editor.render(&view) {
                    Ok(pix) => {
                        if let Some(surface) = &mut self.surface {
                            if let (Some(w), Some(h)) =
                                (NonZeroU32::new(self.size.0), NonZeroU32::new(self.size.1))
                            {
                                if let Err(e) = surface.resize(w, h) {
                                    eprintln!("resize: {e}");
                                    return;
                                }
                                match surface.buffer_mut() {
                                    Ok(mut buffer) => {
                                        for (dst, p) in buffer.iter_mut().zip(pix.pixels()) {
                                            *dst = ((p.red() as u32) << 16)
                                                | ((p.green() as u32) << 8)
                                                | p.blue() as u32;
                                        }
                                        if let Err(e) = buffer.present() {
                                            eprintln!("present: {e}")
                                        }
                                    }
                                    Err(e) => eprintln!("buffer: {e}"),
                                }
                            }
                        }
                        self.frame = Some(pix);
                    }
                    Err(e) => {
                        self.editor.status = format!("Rendering failed: {e}");
                        self.record("render.error", &e.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let pending = std::mem::take(&mut self.pending);
        for wait in pending {
            if self.editor.revision != wait.rev {
                let _ = wait
                    .respond
                    .send(self.ok(serde_json::json!({"waited":true})));
            } else if now >= wait.deadline {
                let _ = wait
                    .respond
                    .send(self.err("TIMEOUT", "No revision change before deadline"));
            } else {
                self.pending.push(wait);
            }
        }
        if self.deadline.is_some_and(|d| now >= d) {
            self.finish();
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(50)));
    }
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!("Specimen - native Hamesh typography studio\n\nUsage: specimen [--project=PATH] [--evidence-dir=DIR] [--smoke-test=SECONDS] [--no-inspect]\n       specimen --render PROJECT OUTPUT.png\n       specimen --render-template=N OUTPUT.png\n       specimen --check PROJECT\n       specimen --fonts\n\nEye/Hand: hamesh map | detail ID | click ID | type FIELD TEXT | key Enter | snapshot PATH\nKeyboard: Ctrl+S save, Ctrl+O open current path, Ctrl+E export, Ctrl+Z undo, Ctrl+Shift+Z redo.\nInspector fields: click/type to replace; Enter applies; Escape cancels. Canvas tab sets file path.\nDirect --render/--check commands are document operations, not GUI evidence.");
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--fonts") {
        println!("{}", serde_json::to_string_pretty(&artwork::fonts())?);
        return Ok(());
    }
    if let Some(index) = args
        .first()
        .and_then(|a| a.strip_prefix("--render-template="))
    {
        let d = document::Document::template(index.parse()?);
        let path = args.get(1).context("Need output PNG path")?;
        artwork::export(&d, Path::new(path))?;
        println!("{}", serde_json::to_string_pretty(&artwork::inspect(&d)?)?);
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--render") {
        let d = document::Document::load(Path::new(args.get(1).context("Need project path")?))?;
        artwork::export(&d, Path::new(args.get(2).context("Need PNG path")?))?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--check") {
        let d = document::Document::load(Path::new(args.get(1).context("Need project path")?))?;
        println!("{}", serde_json::to_string_pretty(&artwork::inspect(&d)?)?);
        return Ok(());
    }
    for a in &args {
        if !(a.starts_with("--project=")
            || a.starts_with("--evidence-dir=")
            || a.starts_with("--smoke-test=")
            || a == "--no-inspect"
            || a == "--inspect")
        {
            bail!("Unknown argument {a}; see --help")
        }
    }
    let evidence = args
        .iter()
        .find_map(|a| a.strip_prefix("--evidence-dir="))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs"));
    std::fs::create_dir_all(&evidence)?;
    let project_arg = args.iter().find_map(|a| a.strip_prefix("--project="));
    let project = project_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.join("after-frequency.specimen.json"));
    let mut editor = Editor::new(project.clone())?;
    if project_arg.is_some() {
        editor.open_path(project)?;
    }
    let seconds = args
        .iter()
        .find_map(|a| a.strip_prefix("--smoke-test="))
        .map(str::parse::<u64>)
        .transpose()?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut host = Host {
        editor,
        window: None,
        context: None,
        surface: None,
        size: (1440, 980),
        cursor: (0.0, 0.0),
        modifiers: ModifiersState::empty(),
        proxy: event_loop.create_proxy(),
        trace: hamesh_diag::Trace::new(2048),
        pending: Vec::new(),
        deadline: seconds.map(|s| Instant::now() + Duration::from_secs(s)),
        evidence,
        frame: None,
        inspect: !args.iter().any(|a| a == "--no-inspect"),
    };
    event_loop.run_app(&mut host)?;
    Ok(())
}
