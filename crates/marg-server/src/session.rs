//! One live session: a simulation on its own thread, driven by a command
//! channel, streaming frames and dashboards to its WebSocket clients
//! (plan P4.1, plan/02-architecture.md §8).
//!
//! The thread owns the [`LiveRun`]. Commands arrive between steps, so each
//! is stamped with the step it acts before; state-changing ones go through
//! the run's command log and replay identically headless.

use crate::frames::{self, FrameRequest};
use crate::protocol::*;
use marg_core::Simulation;
use marg_core::model::longitudinal::Idm;
use marg_io::LiveRun;
use marg_params::{VehicleClass, VehicleType};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;

/// Messages to a client's WebSocket writer.
#[derive(Debug)]
pub enum Outbound {
    Text(String),
    Binary(axum::body::Bytes),
}

/// Messages to the session thread.
pub enum Inbound {
    Client(ClientMessage),
    Connected(u64, Sender<Outbound>),
    Disconnected(u64),
    Shutdown,
}

/// Session settings.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Interval of the live detector windows (s).
    pub live_window_s: f64,
    /// Sampling interval of the time–space diagram (s).
    pub ts_every_s: f64,
    /// Session folders are written to `<runs_dir>/live/<id>/`.
    pub runs_dir: PathBuf,
    /// Threads for the headless replay after a save.
    pub replay_threads: usize,
}

/// What the session list shows (updated by the session thread).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub scenario: String,
    pub seed: u64,
    pub t: f64,
    pub step: u64,
    pub playing: bool,
    pub finished: bool,
    pub clients: usize,
}

/// Frames are sent only while the client's queue has this much room left,
/// so text messages (acks, dashboards) are never crowded out.
const FRAME_QUEUE_RESERVE: usize = 32;
/// Upper bound on time–space samples buffered between dashboard messages.
const TS_MAX_SAMPLES: usize = 100_000;
/// Wall time spent stepping before frames and messages are serviced.
const STEP_BUDGET: Duration = Duration::from_millis(25);
/// Stepping always gets at least this long per loop, even when a frame is due.
const MIN_STEP_SLICE: Duration = Duration::from_millis(2);
const KPI_EVERY: Duration = Duration::from_secs(1);
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// Default viewer frame rate.
pub const DEFAULT_FPS: f64 = 30.0;
/// Speed-histogram bin width (m/s) and maximum number of bins.
const SPEED_BIN: f64 = 1.0;
const SPEED_BINS_MAX: usize = 45;

fn num(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

fn round_to(x: f64, step: f64) -> f64 {
    (x / step).round() * step
}

/// Run a session until it is shut down or every handle is dropped.
pub fn run(
    id: String,
    live: LiveRun,
    cfg: SessionConfig,
    rx: Receiver<Inbound>,
    info: Arc<Mutex<SessionInfo>>,
) {
    let mut s = Session::new(id, live, cfg, info);
    loop {
        let wait = s.next_wakeup().saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(msg) => {
                if !s.handle(msg) {
                    break;
                }
                while let Ok(msg) = rx.try_recv() {
                    if !s.handle(msg) {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        s.advance();
        s.emit();
    }
}

struct Session {
    id: String,
    run: LiveRun,
    cfg: SessionConfig,
    info: Arc<Mutex<SessionInfo>>,
    started: Instant,
    clients: Vec<(u64, Sender<Outbound>)>,
    hello: Hello,
    playing: bool,
    speed: f64,
    anchor: (Instant, f64),
    fps: f64,
    viewport: Option<[f64; 4]>,
    colour: ColourBy,
    ts_ribbon: Option<usize>,
    ts_every: u64,
    /// Extra sampling stride while the buffer is thinned (reset after each dashboard message).
    ts_stride: u64,
    ts: TsSamples,
    selected: Option<u64>,
    breakpoints: Vec<BreakpointOn>,
    events_seen: usize,
    windows_sent: usize,
    undo_stack: Vec<u64>,
    undone: BTreeSet<u64>,
    next_frame: Instant,
    next_kpi: Instant,
    frame_dirty: bool,
    kpi_dirty: bool,
    rtf: (Instant, f64, f64),
    frames_dropped: u64,
}

impl Session {
    fn new(
        id: String,
        mut run: LiveRun,
        cfg: SessionConfig,
        info: Arc<Mutex<SessionInfo>>,
    ) -> Self {
        if let Err(e) = run.sim_mut().enable_live_detectors(cfg.live_window_s) {
            eprintln!("session {id}: live detectors off: {e}");
        }
        let hello = hello(&id, &run, cfg.live_window_s);
        let dt = run.sim().dt();
        let now = Instant::now();
        Self {
            ts_every: ((cfg.ts_every_s / dt).round() as u64).max(1),
            ts_stride: 1,
            id,
            hello,
            cfg,
            info,
            started: now,
            clients: Vec::new(),
            playing: false,
            speed: 1.0,
            anchor: (now, run.sim().time()),
            fps: DEFAULT_FPS,
            viewport: None,
            colour: ColourBy::Class,
            ts_ribbon: None,
            ts: TsSamples {
                ribbon: String::new(),
                t: Vec::new(),
                s: Vec::new(),
                v: Vec::new(),
            },
            selected: None,
            breakpoints: Vec::new(),
            events_seen: 0,
            windows_sent: 0,
            undo_stack: Vec::new(),
            undone: BTreeSet::new(),
            next_frame: now,
            next_kpi: now,
            frame_dirty: true,
            kpi_dirty: true,
            rtf: (now, run.sim().time(), 0.0),
            frames_dropped: 0,
            run,
        }
    }

    fn sim(&self) -> &Simulation {
        self.run.sim()
    }

    fn wall(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    // ------------------------------------------------------------ sending

    fn send_to(&mut self, conn: Option<u64>, msg: Outbound, is_frame: bool) {
        let mut closed = Vec::new();
        for (c, tx) in &self.clients {
            if conn.is_some_and(|x| x != *c) {
                continue;
            }
            if is_frame && tx.capacity() <= FRAME_QUEUE_RESERVE {
                self.frames_dropped += 1;
                continue;
            }
            // Cheap clones: frames are reference-counted bytes.
            let m = match &msg {
                Outbound::Text(t) => Outbound::Text(t.clone()),
                Outbound::Binary(b) => Outbound::Binary(b.clone()),
            };
            match tx.try_send(m) {
                Ok(()) => {}
                Err(TrySendError::Closed(_)) => closed.push(*c),
                Err(TrySendError::Full(_)) => self.frames_dropped += 1,
            }
        }
        self.clients.retain(|(c, _)| !closed.contains(c));
    }

    fn send(&mut self, m: &ServerMessage) {
        self.send_conn(None, m);
    }

    fn send_conn(&mut self, conn: Option<u64>, m: &ServerMessage) {
        match serde_json::to_string(m) {
            Ok(t) => self.send_to(conn, Outbound::Text(t), false),
            Err(e) => eprintln!("session {}: cannot encode message: {e}", self.id),
        }
    }

    fn ack(&mut self, seq: Option<u64>, cmd: &str, r: &Result<String, String>) {
        let (ok, message) = match r {
            Ok(m) => (true, m.clone()),
            Err(m) => (false, m.clone()),
        };
        self.send(&ServerMessage::Ack {
            seq,
            cmd: cmd.to_string(),
            ok,
            message,
        });
    }

    // ----------------------------------------------------------- commands

    /// Handle one message; `false` ends the session.
    fn handle(&mut self, msg: Inbound) -> bool {
        match msg {
            Inbound::Shutdown => return false,
            Inbound::Connected(conn, tx) => {
                self.clients.push((conn, tx));
                self.greet(conn);
            }
            Inbound::Disconnected(conn) => self.clients.retain(|(c, _)| *c != conn),
            Inbound::Client(m) => self.command(m),
        }
        self.update_info();
        true
    }

    /// Every road object, with its footprint in frame coordinates.
    fn objects(&self) -> ServerMessage {
        let sim = self.sim();
        let net = sim.network();
        let [ox, oy] = self.hello.origin;
        let finite = |x: f64| x.is_finite().then_some(x);
        let objects = sim
            .objects()
            .iter()
            .map(|o| {
                let r = net.ribbon(o.ribbon);
                let (hl, hw) = (0.5 * o.length, 0.5 * o.width);
                let corner = |ds: f64, dy: f64| {
                    let (x, y, _) = r.to_world(o.s + ds, o.y + dy);
                    [x - ox, y - oy]
                };
                ObjectView {
                    id: o.id.clone(),
                    ribbon: r.id.clone(),
                    s: o.s,
                    y: o.y,
                    length: o.length,
                    width: o.width,
                    shape: vec![
                        corner(-hl, -hw),
                        corner(hl, -hw),
                        corner(hl, hw),
                        corner(-hl, hw),
                    ],
                    present: o
                        .present
                        .iter()
                        .map(|&(a, b)| [finite(a), finite(b)])
                        .collect(),
                }
            })
            .collect();
        ServerMessage::Objects { objects }
    }

    fn greet(&mut self, conn: u64) {
        let hello = ServerMessage::Hello(Box::new(self.hello.clone()));
        self.send_conn(Some(conn), &hello);
        let status = self.status();
        self.send_conn(Some(conn), &status);
        let history = self.history();
        self.send_conn(Some(conn), &history);
        let objects = self.objects();
        self.send_conn(Some(conn), &objects);
        // Catch up on the dashboard windows closed so far.
        let mut kpi = self.kpi_frame(false);
        kpi.windows = rows(self.sim().live_detector_records(0, self.windows_sent));
        self.send_conn(Some(conn), &ServerMessage::Kpi(kpi));
        self.frame_dirty = true;
    }

    fn log(&mut self, cmd: &str, args: serde_json::Value, r: Result<String, String>) {
        let wall = self.wall();
        let seq = self.run.log(cmd, &args, &r, wall);
        self.ack(Some(seq), cmd, &r);
    }

    fn state_command(&mut self, cmd: &str, args: serde_json::Value) -> Result<String, String> {
        let wall = self.wall();
        let (seq, r) = self.run.command(cmd, &args, wall);
        self.ack(Some(seq), cmd, &r);
        if r.is_ok() {
            self.kpi_dirty = true;
            self.frame_dirty = true;
            if cmd == "set_param" && args.get("undo_of").is_none() {
                self.undo_stack.push(seq);
            }
        }
        r
    }

    fn command(&mut self, m: ClientMessage) {
        let name = m.name();
        match m {
            ClientMessage::Play => {
                let r = if self.sim().finished() {
                    Err("the scenario has ended".to_string())
                } else {
                    self.playing = true;
                    self.anchor = (Instant::now(), self.sim().time());
                    Ok("playing".to_string())
                };
                self.log(name, json!({}), r);
                self.send_status();
            }
            ClientMessage::Pause => {
                self.playing = false;
                self.log(name, json!({}), Ok("paused".into()));
                self.send_status();
            }
            ClientMessage::Step { n } => {
                self.playing = false;
                let mut done = 0;
                let mut hit = false;
                while done < n && !self.sim().finished() && !hit {
                    hit = self.step_once();
                    done += 1;
                }
                self.log(name, json!({ "n": n }), Ok(format!("stepped {done}")));
                self.kpi_dirty = true;
                self.next_kpi = Instant::now();
                self.send_status();
            }
            ClientMessage::Speed { x } => {
                let r = if x.is_finite() && x >= 0.0 {
                    self.speed = x;
                    self.anchor = (Instant::now(), self.sim().time());
                    Ok(if x == 0.0 {
                        "as fast as possible".into()
                    } else {
                        format!("{x}× real time")
                    })
                } else {
                    Err("speed must be a non-negative number".into())
                };
                self.log(name, json!({ "x": x }), r);
                self.send_status();
            }
            ClientMessage::SetParam {
                path,
                value,
                resample,
            } => {
                let r = self.state_command(name, json!({ "path": path, "value": value }));
                if r.is_ok() && resample {
                    let _ = self.state_command("resample", json!({ "path": path }));
                }
                let h = self.history();
                self.send(&h);
            }
            ClientMessage::Resample { path } => {
                let _ = self.state_command(name, json!({ "path": path }));
            }
            ClientMessage::Undo => match self.undo_stack.pop() {
                None => self.log(name, json!({}), Err("nothing to undo".into())),
                Some(seq) => {
                    let change = self.run.history().iter().find(|c| c.seq == seq).cloned();
                    match change.and_then(|c| c.before.map(|b| (c.path, b))) {
                        None => self.log(
                            name,
                            json!({ "of": seq }),
                            Err("the parameter had no value before; set one instead".into()),
                        ),
                        Some((path, before)) => {
                            let args = json!({ "path": path, "value": before, "undo_of": seq });
                            if self.state_command("set_param", args).is_ok() {
                                self.undone.insert(seq);
                            } else {
                                self.undo_stack.push(seq);
                            }
                            let h = self.history();
                            self.send(&h);
                        }
                    }
                }
            },
            ClientMessage::SelectAgent { id } => {
                let parsed = id.as_deref().map(str::parse::<u64>).transpose();
                match parsed {
                    Ok(sel) => {
                        self.selected = sel;
                        let msg = sel.map_or("selection cleared".into(), |i| format!("agent {i}"));
                        self.log(name, json!({ "id": id }), Ok(msg));
                        self.send_inspector();
                    }
                    Err(_) => self.log(name, json!({ "id": id }), Err("bad agent id".into())),
                }
            }
            ClientMessage::Breakpoint { on } => {
                let args = serde_json::to_value(&on).unwrap_or_default();
                self.breakpoints.push(on);
                self.log(name, args, Ok("breakpoint set".into()));
                self.send_status();
            }
            ClientMessage::ClearBreakpoints => {
                self.breakpoints.clear();
                self.log(name, json!({}), Ok("breakpoints cleared".into()));
                self.send_status();
            }
            ClientMessage::ToggleSignalState { junction, state } => self.log(
                name,
                json!({ "junction": junction, "state": state }),
                Err("signals are not modelled yet (phase P8)".into()),
            ),
            ClientMessage::AddObject { object } => {
                let _ = self.state_command(name, json!({ "object": object }));
                let m = self.objects();
                self.send(&m);
            }
            ClientMessage::RemoveObject { id } => {
                let _ = self.state_command(name, json!({ "id": id }));
                let m = self.objects();
                self.send(&m);
            }
            ClientMessage::Save => self.save(),
            ClientMessage::View {
                viewport,
                fps,
                colour,
                ts_ribbon,
            } => self.view(ViewSettings {
                viewport,
                fps,
                colour,
                ts_ribbon,
            }),
        }
    }

    fn view(&mut self, v: ViewSettings) {
        if let Some(vp) = v.viewport
            && vp.iter().all(|x| x.is_finite())
        {
            self.viewport = Some(vp);
        }
        if let Some(fps) = v.fps
            && fps.is_finite()
            && fps > 0.0
        {
            self.fps = fps.min(120.0);
        }
        if let Some(c) = v.colour {
            self.colour = c;
        }
        if let Some(r) = v.ts_ribbon {
            self.ts_ribbon = self.sim().network().index(&r).ok();
            self.ts.ribbon = r;
            self.ts.t.clear();
            self.ts.s.clear();
            self.ts.v.clear();
        }
        self.frame_dirty = true;
        self.next_frame = Instant::now();
    }

    fn save(&mut self) {
        let dir = self.cfg.runs_dir.join("live").join(&self.id);
        self.log(
            "save",
            json!({ "dir": dir.display().to_string() }),
            Ok("saving".into()),
        );
        match self.run.save(&dir, &self.id) {
            Err(e) => {
                let m = ServerMessage::Error {
                    message: format!("save failed: {e}"),
                };
                self.send(&m);
            }
            Ok(manifest) => {
                let clients: Vec<_> = self.clients.iter().map(|(_, t)| t.clone()).collect();
                let rel = dir
                    .strip_prefix(&self.cfg.runs_dir)
                    .unwrap_or(&dir)
                    .display()
                    .to_string();
                let threads = self.cfg.replay_threads;
                std::thread::spawn(move || {
                    let msg = match marg_io::replay(&dir, threads, true) {
                        Ok(r) => ServerMessage::Saved {
                            path: rel,
                            run_hash: manifest.run_hash,
                            output_hash: manifest.output_hash,
                            replay_output_hash: r.output_hash,
                            replay_matches: r.matches,
                            steps: r.steps,
                        },
                        Err(e) => ServerMessage::Error {
                            message: format!("replay of the saved session failed: {e}"),
                        },
                    };
                    if let Ok(t) = serde_json::to_string(&msg) {
                        for c in clients {
                            let _ = c.blocking_send(Outbound::Text(t.clone()));
                        }
                    }
                });
            }
        }
    }

    // ----------------------------------------------------------- stepping

    /// One step; returns whether a breakpoint fired (and paused the session).
    fn step_once(&mut self) -> bool {
        let t_prev = self.sim().time();
        self.run.step();
        self.frame_dirty = true;
        let sim = self.run.sim();
        if let Some(r) = self.ts_ribbon
            && sim
                .step_index()
                .is_multiple_of(self.ts_every * self.ts_stride)
        {
            let a = sim.agents();
            let t = round_to(sim.time(), 1e-3);
            for i in 0..a.len() {
                if a.ribbon[i] as usize == r {
                    self.ts.t.push(t);
                    self.ts.s.push(round_to(a.s[i], 0.01));
                    self.ts.v.push(round_to(a.v[i], 0.01));
                }
            }
            if self.ts.t.len() > TS_MAX_SAMPLES {
                // Running fast: keep every other sample time, and sample half as often until
                // the next dashboard message, so the diagram spans the whole interval.
                thin(&mut self.ts);
                self.ts_stride *= 2;
            }
        }
        self.check_breakpoints(t_prev)
    }

    fn check_breakpoints(&mut self, t_prev: f64) -> bool {
        if self.breakpoints.is_empty() {
            self.events_seen = self.sim().events().len();
            return false;
        }
        let t = self.sim().time();
        let mut hit: Option<(String, Option<EventRow>)> = None;
        for e in &self.sim().events()[self.events_seen..] {
            let wanted = self
                .breakpoints
                .iter()
                .any(|b| matches!(b, BreakpointOn::Event { kind } if kind == e.kind));
            if wanted {
                hit = Some((
                    format!("event `{}`", e.kind),
                    Some(EventRow {
                        t: e.t,
                        kind: e.kind.to_string(),
                        agent_id: e.agent_id.to_string(),
                        ribbon: e.ribbon.clone(),
                        s: e.s,
                        y: e.y,
                        attrs: e.attrs.clone(),
                    }),
                ));
                break;
            }
        }
        self.events_seen = self.sim().events().len();
        if hit.is_none() {
            let half = 0.5 * self.sim().dt();
            for b in &self.breakpoints {
                if let BreakpointOn::Time { t: tb } = b
                    && t_prev < tb - half
                    && t >= tb - half
                {
                    hit = Some((format!("time {tb} s"), None));
                }
            }
        }
        let Some((reason, event)) = hit else {
            return false;
        };
        self.playing = false;
        let step = self.sim().step_index();
        self.send(&ServerMessage::Breakpoint {
            reason,
            t,
            step,
            event,
        });
        self.kpi_dirty = true;
        self.next_kpi = Instant::now();
        self.send_status();
        true
    }

    fn advance(&mut self) {
        if !self.playing {
            return;
        }
        let now = Instant::now();
        let dt = self.sim().dt();
        let target = if self.speed == 0.0 {
            f64::INFINITY
        } else {
            self.anchor.1 + self.speed * (now - self.anchor.0).as_secs_f64()
        };
        // Yield in time for the next frame, so frames keep their cadence.
        let budget = (now + STEP_BUDGET).min(self.next_frame.max(now + MIN_STEP_SLICE));
        while self.sim().time() + 0.5 * dt <= target
            && !self.sim().finished()
            && Instant::now() < budget
        {
            if self.step_once() {
                return;
            }
        }
        // Behind by more than a wall second: re-anchor instead of bursting to catch up.
        if self.speed > 0.0 && target - self.sim().time() > self.speed {
            self.anchor = (Instant::now(), self.sim().time());
        }
        if self.sim().finished() {
            self.playing = false;
            self.kpi_dirty = true;
            self.next_kpi = Instant::now();
            self.send_status();
        }
    }

    fn next_wakeup(&self) -> Instant {
        let now = Instant::now();
        if self.playing {
            let step_due = if self.speed == 0.0 {
                now
            } else {
                let next_t = self.sim().time() + self.sim().dt() - self.anchor.1;
                self.anchor.0 + Duration::from_secs_f64((next_t / self.speed).max(0.0))
            };
            step_due.min(self.next_frame).min(self.next_kpi)
        } else if self.frame_dirty || self.kpi_dirty {
            self.next_frame.min(self.next_kpi)
        } else {
            now + IDLE_WAIT
        }
    }

    // ------------------------------------------------------------ outputs

    fn emit(&mut self) {
        let now = Instant::now();
        if now >= self.next_frame && self.frame_dirty {
            if !self.clients.is_empty() {
                let req = FrameRequest {
                    origin: self.hello.origin,
                    viewport: self.viewport,
                    colour: self.colour,
                };
                match frames::encode(self.sim(), &req) {
                    Ok(f) => self.send_to(None, Outbound::Binary(f.bytes.into()), true),
                    Err(e) => eprintln!("session {}: frame: {e}", self.id),
                }
            }
            self.frame_dirty = false;
            // A fixed cadence: late frames do not push the next one back.
            let period = Duration::from_secs_f64(1.0 / self.fps);
            self.next_frame += period;
            if self.next_frame <= now {
                self.next_frame = now + period;
            }
        }
        if now >= self.next_kpi && (self.kpi_dirty || self.playing) {
            let kpi = self.kpi_frame(true);
            self.send(&ServerMessage::Kpi(kpi));
            self.send_inspector();
            self.send_status();
            self.kpi_dirty = false;
            self.next_kpi = now + KPI_EVERY;
            self.update_info();
        }
    }

    fn update_info(&self) {
        if let Ok(mut i) = self.info.lock() {
            i.t = self.sim().time();
            i.step = self.sim().step_index();
            i.playing = self.playing;
            i.finished = self.sim().finished();
            i.clients = self.clients.len();
        }
    }

    fn status(&mut self) -> ServerMessage {
        let now = Instant::now();
        let (w0, t0, last) = self.rtf;
        let elapsed = (now - w0).as_secs_f64();
        let rtf = if elapsed >= 0.5 {
            let r = (self.sim().time() - t0) / elapsed;
            self.rtf = (now, self.sim().time(), r);
            r
        } else {
            last
        };
        ServerMessage::Status(Status {
            playing: self.playing,
            speed: self.speed,
            t: self.sim().time(),
            step: self.sim().step_index(),
            finished: self.sim().finished(),
            agents: self.sim().agent_count() as u64,
            real_time_factor: if self.playing { rtf } else { 0.0 },
            breakpoints: self.breakpoints.clone(),
            run_hash: self.run.run_hash(),
        })
    }

    fn send_status(&mut self) {
        let s = self.status();
        self.send(&s);
    }

    fn history(&self) -> ServerMessage {
        ServerMessage::History {
            changes: self
                .run
                .history()
                .iter()
                .map(|c| ChangeView {
                    seq: c.seq,
                    step: c.step,
                    t: c.t,
                    path: c.path.clone(),
                    before: c.before.clone(),
                    after: c.after.clone(),
                    undone: self.undone.contains(&c.seq),
                })
                .collect(),
        }
    }

    fn kpi_frame(&mut self, take_windows: bool) -> Kpi {
        let sim = self.run.sim();
        let now = sim.kpis_now();
        let waiting = now
            .iter()
            .find(|(k, _)| *k == "waiting_to_enter_at_end")
            .map_or(0, |(_, v)| *v as u64);
        let kpis = now
            .into_iter()
            .map(|(name, v)| KpiValue {
                name: name.to_string(),
                value: num(v),
            })
            .collect();
        let windows = if take_windows {
            let closed = sim.live_intervals_closed();
            let w = rows(sim.live_detector_records(self.windows_sent, closed));
            self.windows_sent = closed;
            w
        } else {
            Vec::new()
        };
        let a = self.run.sim().agents();
        let mut hists: Vec<Vec<u32>> = vec![Vec::new(); VehicleClass::ALL.len()];
        for i in 0..a.len() {
            let b = ((a.v[i] / SPEED_BIN) as usize).min(SPEED_BINS_MAX - 1);
            let h = &mut hists[usize::from(a.class[i])];
            if h.len() <= b {
                h.resize(b + 1, 0);
            }
            h[b] += 1;
        }
        let speed_hist = VehicleClass::ALL
            .iter()
            .zip(hists)
            .filter(|(_, h)| !h.is_empty())
            .map(|(c, counts)| ClassHist {
                class: c.key().to_string(),
                bin_width: SPEED_BIN,
                counts,
            })
            .collect();
        self.ts_stride = 1;
        let ts = self.ts_ribbon.map(|_| TsSamples {
            ribbon: self.ts.ribbon.clone(),
            t: std::mem::take(&mut self.ts.t),
            s: std::mem::take(&mut self.ts.s),
            v: std::mem::take(&mut self.ts.v),
        });
        let sim = self.run.sim();
        Kpi {
            t: sim.time(),
            step: sim.step_index(),
            kpis,
            windows,
            speed_hist,
            ts,
            agents: sim.agent_count() as u64,
            waiting,
        }
    }

    fn send_inspector(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let ins = inspect(self.sim(), id, self.hello.origin);
        self.send(&ServerMessage::Inspector(Box::new(ins)));
    }
}

/// Drop every other sample time (samples are in time order).
fn thin(ts: &mut TsSamples) {
    let mut k = 0usize;
    let mut last = f64::NAN;
    let mut keep = Vec::with_capacity(ts.t.len());
    for &t in &ts.t {
        if t != last {
            k += 1;
            last = t;
        }
        keep.push(k % 2 == 1);
    }
    let mut it = keep.iter();
    ts.t.retain(|_| *it.next().unwrap_or(&false));
    let mut it = keep.iter();
    ts.s.retain(|_| *it.next().unwrap_or(&false));
    let mut it = keep.iter();
    ts.v.retain(|_| *it.next().unwrap_or(&false));
}

/// Detector records as protocol rows.
pub fn rows(recs: Vec<marg_core::DetectorRecord>) -> Vec<DetectorRow> {
    recs.into_iter()
        .map(|r| DetectorRow {
            detector_id: r.detector_id,
            kind: r.kind.to_string(),
            t_start: r.t_start,
            t_end: r.t_end,
            in_warmup: r.in_warmup,
            vehicle_type: r.vehicle_type.to_string(),
            class: r.class.to_string(),
            count: r.count,
            time_mean_speed: num(r.time_mean_speed),
            space_mean_speed: num(r.space_mean_speed),
            density: num(r.density),
            flow: num(r.flow),
            tts: num(r.tts),
            ttd: num(r.ttd),
            lateral_mean: num(r.lateral_mean),
            lateral_sd: num(r.lateral_sd),
            lateral_y0: num(r.lateral_y0),
            lateral_bin: num(r.lateral_bin),
            lateral_hist: (!r.lateral_hist.is_empty()).then_some(r.lateral_hist),
        })
        .collect()
}

/// An agent's `agents.parquet` row as the protocol sends it.
pub fn agent_row(r: marg_core::AgentRecord) -> AgentRow {
    AgentRow {
        agent_id: r.agent_id.to_string(),
        vehicle_type: r.vehicle_type.to_string(),
        class: r.class.to_string(),
        entry: r.entry,
        t_arrival: r.t_arrival,
        t_entry: r.t_entry,
        t_exit: r.t_exit,
        spawn_delay: r.spawn_delay,
        distance: r.distance,
        travel_time: r.travel_time,
        free_flow_time: r.free_flow_time,
        delay: r.delay,
        stops: r.stops,
        exited: r.exited,
        entry_y: r.entry_y,
        length: r.length,
        width: r.width,
        v0: r.v0,
        time_gap: r.time_gap,
        standstill_gap: r.standstill_gap,
        max_accel: r.max_accel,
        comfortable_decel: r.comfortable_decel,
    }
}

/// The inspector panel for agent `id`.
pub fn inspect(sim: &Simulation, id: u64, origin: [f64; 2]) -> Inspector {
    let note = "Persona, traits and the logit breakdown of decisions arrive with P8.".to_string();
    let Some(i) = sim.agent_index(id) else {
        return Inspector {
            id: id.to_string(),
            found: false,
            t: sim.time(),
            record: None,
            state: None,
            neighbours: Vec::new(),
            free_accel: None,
            note,
        };
    };
    let a = sim.agents();
    let net = sim.network();
    let ribbon = net.ribbon(a.ribbon[i] as usize);
    let (wx, wy, h) = ribbon.to_world(a.s[i], a.y[i]);
    let neighbours = sim
        .interactions(i)
        .into_iter()
        .map(|x| {
            let j = x.j as usize;
            Neighbour {
                id: a.id[j].to_string(),
                vehicle_type: VehicleType::ALL[usize::from(a.vtype[j])].key().to_string(),
                role: if x.weight.is_nan() { "side" } else { "leader" }.to_string(),
                ds: x.ds,
                gap_s: x.gap_s,
                lateral_gap: x.lateral_gap,
                clearance: x.clearance,
                weight: num(x.weight),
                leader_accel: num(x.leader_accel),
            }
        })
        .collect();
    let idm = Idm {
        v0: a.v0[i],
        time_gap: a.time_gap[i],
        standstill_gap: a.standstill_gap[i],
        max_accel: a.max_accel[i],
        comfortable_decel: a.comfortable_decel[i],
        exponent: sim.params().accel_exponent,
    };
    Inspector {
        id: id.to_string(),
        found: true,
        t: sim.time(),
        record: Some(agent_row(sim.agent_record(i))),
        state: Some(AgentState {
            ribbon: ribbon.id.clone(),
            s: a.s[i],
            y: a.y[i],
            v: a.v[i],
            acc: a.acc[i],
            x_world: wx - origin[0],
            y_world: wy - origin[1],
            heading: h,
        }),
        neighbours,
        free_accel: Some(idm.free_accel(a.v[i])),
        note,
    }
}

/// The handshake message for a session.
pub fn hello(id: &str, run: &LiveRun, live_window_s: f64) -> Hello {
    let sim = run.sim();
    let net = sim.network();
    let sc = run.scenario();
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for r in &sc.network.ribbons {
        for p in &r.polyline {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let origin = [lo[0].floor(), lo[1].floor()];
    let rel = |(x, y, _): (f64, f64, f64)| [x - origin[0], y - origin[1]];
    let band = sim.params().median_band_width;
    let ribbons: Vec<RibbonView> = net
        .ribbons
        .iter()
        .map(|r| {
            let st = r.outline_stations();
            let right: Vec<_> = st
                .iter()
                .map(|&s| rel(r.to_world(s, r.y_bounds(s).0)))
                .collect();
            let left: Vec<_> = st
                .iter()
                .map(|&s| rel(r.to_world(s, r.y_bounds(s).1)))
                .collect();
            let band_edge: Vec<_> = st
                .iter()
                .map(|&s| {
                    let (y0, y1) = r.y_bounds(s);
                    rel(r.to_world(s, (y0 + band).min(y1)))
                })
                .collect();
            let mut outline = right.clone();
            outline.extend(left.iter().rev());
            let mut median_band = right;
            median_band.extend(band_edge.iter().rev());
            RibbonView {
                id: r.id.clone(),
                length: r.length(),
                centre: st.iter().map(|&s| rel(r.to_world(s, 0.0))).collect(),
                outline,
                median_band,
                next: r.next.map(|n| net.ribbon(n).id.clone()),
            }
        })
        .collect();
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in ribbons.iter().flat_map(|r| r.outline.iter()) {
        bounds[0] = bounds[0].min(p[0]);
        bounds[1] = bounds[1].min(p[1]);
        bounds[2] = bounds[2].max(p[0]);
        bounds[3] = bounds[3].max(p[1]);
    }
    let detectors = sc
        .detectors
        .iter()
        .filter_map(|d| {
            let r = net.ribbon(net.index(d.ribbon()).ok()?);
            let (kind, s_from, s_to, shape) = match d {
                marg_params::spec::DetectorSpec::Screenline { s, .. } => {
                    let (y0, y1) = r.y_bounds(*s);
                    (
                        "screenline",
                        *s,
                        *s,
                        vec![rel(r.to_world(*s, y0)), rel(r.to_world(*s, y1))],
                    )
                }
                marg_params::spec::DetectorSpec::Area { s_from, s_to, .. } => {
                    let mut st: Vec<f64> = r
                        .outline_stations()
                        .into_iter()
                        .filter(|s| s > s_from && s < s_to)
                        .collect();
                    st.insert(0, *s_from);
                    st.push(*s_to);
                    let mut shape: Vec<_> = st
                        .iter()
                        .map(|&s| rel(r.to_world(s, r.y_bounds(s).0)))
                        .collect();
                    shape.extend(
                        st.iter()
                            .rev()
                            .map(|&s| rel(r.to_world(s, r.y_bounds(s).1))),
                    );
                    ("area", *s_from, *s_to, shape)
                }
            };
            Some(DetectorView {
                id: d.id().to_string(),
                kind: kind.to_string(),
                ribbon: d.ribbon().to_string(),
                s_from,
                s_to,
                shape,
            })
        })
        .collect();
    let mode = |key, label: &str, unit: &str, categorical, available, note: &str| ColourMode {
        key,
        label: label.to_string(),
        unit: unit.to_string(),
        categorical,
        available,
        note: note.to_string(),
    };
    Hello {
        protocol: PROTOCOL_VERSION,
        marg_version: marg_core::VERSION.to_string(),
        session: id.to_string(),
        seed: run.seed(),
        scenario: sc.clone(),
        initial_run_hash: run.initial_run_hash(),
        origin,
        bounds,
        ribbons,
        detectors,
        classes: VehicleClass::ALL
            .iter()
            .map(|c| c.key().to_string())
            .collect(),
        vehicle_types: VehicleType::ALL
            .iter()
            .map(|t| TypeView {
                key: t.key().to_string(),
                class: t.class().key().to_string(),
            })
            .collect(),
        colour_modes: vec![
            mode(ColourBy::Class, "Class", "", true, true, ""),
            mode(ColourBy::Speed, "Speed", "m/s", false, true, ""),
            mode(
                ColourBy::SpeedRatio,
                "Speed / desired speed",
                "",
                false,
                true,
                "",
            ),
            mode(ColourBy::Accel, "Acceleration", "m/s²", false, true, ""),
            mode(ColourBy::Lateral, "Lateral position", "m", false, true, ""),
            mode(
                ColourBy::DesiredSpeed,
                "Desired speed v0",
                "m/s",
                false,
                true,
                "",
            ),
            mode(
                ColourBy::Persona,
                "Persona",
                "",
                true,
                false,
                "personas arrive in P8",
            ),
            mode(
                ColourBy::Kappa,
                "Aggressiveness κ",
                "",
                false,
                false,
                "personas arrive in P8",
            ),
            mode(
                ColourBy::Violation,
                "Violation state",
                "",
                true,
                false,
                "violations arrive in P8",
            ),
            mode(
                ColourBy::Zone,
                "Zone",
                "",
                true,
                false,
                "zones arrive in P9",
            ),
        ],
        live_window_s,
        dt_s: sim.dt(),
        duration_s: sc.duration_s,
        warmup_s: sc.warmup_s,
        frame_columns: FRAME_COLUMNS.iter().map(|c| c.to_string()).collect(),
    }
}
