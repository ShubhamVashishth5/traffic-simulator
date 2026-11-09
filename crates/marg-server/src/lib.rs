//! Live WebSocket sessions and run-file serving for the web viewer (plan P4).
//!
//! `marg serve` runs [`serve`]. Endpoints:
//!
//! | Method and path | What |
//! | --- | --- |
//! | `GET /version` | protocol and simulator version |
//! | `GET /register` | the register files, grouped as in `params/base/` |
//! | `GET /scenarios` | scenario files the server can start |
//! | `POST /sessions` | start a session: `{scenario \| yaml, seed, threads, live_window_s}` |
//! | `GET /sessions` | running sessions |
//! | `DELETE /sessions/{id}` | stop a session |
//! | `GET /sessions/{id}/ws?protocol=1` | the session's WebSocket ([`protocol`]) |
//! | `GET /runs` | run folders and experiments under the runs directory |
//! | `GET /files/{path}` | files under the runs directory (HTTP range requests, for DuckDB-WASM) |
//! | anything else | the built web app (`web/dist`) |

pub mod frames;
pub mod protocol;
pub mod session;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use marg_io::{LiveRun, LiveSource};
use marg_params::{Register, Scenario};
use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use serde::{Deserialize, Serialize};
use session::{Inbound, Outbound, SessionConfig, SessionInfo};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

/// Server settings.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub params_dir: PathBuf,
    pub calibration_dir: Option<PathBuf>,
    pub scenarios_dir: PathBuf,
    /// Run folders served under `/files` and listed by `/runs`; live sessions
    /// are saved to `<runs_dir>/live/`.
    pub runs_dir: PathBuf,
    /// Built web app (`web/dist`); `None` serves a hint page instead.
    pub web_dir: Option<PathBuf>,
    /// Default live detector window (s).
    pub live_window_s: f64,
    /// Time–space diagram sampling interval (s).
    pub ts_every_s: f64,
    /// Default worker threads per session.
    pub threads: usize,
}

impl ServerConfig {
    /// Defaults relative to a repository checkout.
    pub fn new(params_dir: PathBuf) -> Self {
        Self {
            params_dir,
            calibration_dir: None,
            scenarios_dir: PathBuf::from("scenarios"),
            runs_dir: PathBuf::from("runs"),
            web_dir: None,
            live_window_s: 30.0,
            ts_every_s: 1.0,
            threads: 1,
        }
    }
}

struct SessionHandle {
    tx: std::sync::mpsc::Sender<Inbound>,
    info: Arc<Mutex<SessionInfo>>,
}

/// Shared server state.
pub struct AppState {
    cfg: ServerConfig,
    sessions: Mutex<BTreeMap<String, SessionHandle>>,
    next_session: AtomicU64,
    next_conn: AtomicU64,
}

impl AppState {
    pub fn new(cfg: ServerConfig) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            sessions: Mutex::new(BTreeMap::new()),
            next_session: AtomicU64::new(1),
            next_conn: AtomicU64::new(1),
        })
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Ok(s) = self.sessions.lock() {
            for h in s.values() {
                let _ = h.tx.send(Inbound::Shutdown);
            }
        }
    }
}

/// The router (also used by tests).
pub fn router(state: Arc<AppState>) -> Router {
    let mut app = Router::new()
        .route("/version", get(version))
        .route("/register", get(register))
        .route("/scenarios", get(scenarios))
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", axum::routing::delete(delete_session))
        .route("/sessions/{id}/ws", get(ws))
        .route("/runs", get(runs))
        .nest_service("/files", ServeDir::new(&state.cfg.runs_dir));
    app = match &state.cfg.web_dir {
        Some(web) if web.join("index.html").is_file() => app.fallback_service(
            ServeDir::new(web).not_found_service(ServeFile::new(web.join("index.html"))),
        ),
        _ => app.fallback(no_web),
    };
    app.with_state(state)
}

/// Serve until Ctrl-C.
pub async fn serve(cfg: ServerConfig, addr: std::net::SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("marg viewer on http://{}", listener.local_addr()?);
    let app = router(AppState::new(cfg));
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

fn bad(m: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, m.to_string())
}

async fn no_web() -> Html<&'static str> {
    Html(
        "<!doctype html><title>marg</title><p>The web viewer is not built. Run \
         <code>(cd web &amp;&amp; npm ci &amp;&amp; npm run build)</code> and start \
         <code>marg serve --web web/dist</code>, or use <code>npm run dev</code>.</p>",
    )
}

async fn version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "marg_version": marg_core::VERSION,
    }))
}

async fn register(State(st): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    let files = Register::load_files(&st.cfg.params_dir).map_err(bad)?;
    Ok(Json(serde_json::json!({ "files": files })))
}

#[derive(Serialize)]
struct ScenarioListing {
    file: String,
    name: String,
    description: String,
    duration_s: f64,
}

async fn scenarios(State(st): State<Arc<AppState>>) -> Json<Vec<ScenarioListing>> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&st.cfg.scenarios_dir) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
            .collect();
        files.sort();
        for f in files {
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let Ok(sc) = Scenario::from_yaml(&text) else {
                continue;
            };
            out.push(ScenarioListing {
                file: f.file_name().unwrap_or_default().to_string_lossy().into(),
                name: sc.name,
                description: sc.description.trim().to_string(),
                duration_s: sc.duration_s,
            });
        }
    }
    Json(out)
}

/// `POST /sessions` body.
#[derive(Debug, Deserialize)]
pub struct NewSession {
    /// A file name in the scenarios directory…
    #[serde(default)]
    pub scenario: Option<String>,
    /// …or the scenario YAML itself.
    #[serde(default)]
    pub yaml: Option<String>,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub threads: Option<usize>,
    #[serde(default)]
    pub live_window_s: Option<f64>,
}

fn default_seed() -> u64 {
    1
}

/// `POST /sessions` reply.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionCreated {
    pub id: String,
    pub ws: String,
    pub scenario: String,
    pub initial_run_hash: String,
}

/// Start a session thread; returns its id.
pub fn start_session(st: &Arc<AppState>, req: NewSession) -> Result<SessionCreated, String> {
    let yaml = match (&req.scenario, &req.yaml) {
        (_, Some(y)) => y.clone(),
        (Some(file), None) => {
            let name = Path::new(file);
            if name.components().count() != 1 || file.contains("..") {
                return Err("`scenario` must be a file name in the scenarios directory".into());
            }
            std::fs::read_to_string(st.cfg.scenarios_dir.join(name))
                .map_err(|e| format!("reading scenario {file}: {e}"))?
        }
        (None, None) => return Err("give `scenario` (a file name) or `yaml`".into()),
    };
    let scenario = Scenario::from_yaml(&yaml).map_err(|e| format!("scenario: {e}"))?;
    let live_window_s = req.live_window_s.unwrap_or(st.cfg.live_window_s);
    if !(live_window_s.is_finite() && live_window_s >= scenario.dt_s) {
        return Err("live_window_s must be at least dt_s".into());
    }
    let source = LiveSource {
        params_dir: st.cfg.params_dir.clone(),
        calibration_dir: st.cfg.calibration_dir.clone(),
    };
    let threads = req.threads.unwrap_or(st.cfg.threads);
    let run =
        LiveRun::new(source, scenario.clone(), req.seed, threads).map_err(|e| e.to_string())?;
    let n = st.next_session.fetch_add(1, Ordering::Relaxed);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let id = format!("{}-{secs}-{n}", scenario.name);
    let initial_run_hash = run.initial_run_hash();
    let info = Arc::new(Mutex::new(SessionInfo {
        id: id.clone(),
        scenario: scenario.name.clone(),
        seed: req.seed,
        ..Default::default()
    }));
    let (tx, rx) = std::sync::mpsc::channel();
    let cfg = SessionConfig {
        live_window_s,
        ts_every_s: st.cfg.ts_every_s,
        runs_dir: st.cfg.runs_dir.clone(),
        replay_threads: threads,
    };
    let (sid, sinfo) = (id.clone(), info.clone());
    std::thread::Builder::new()
        .name(format!("session-{id}"))
        .spawn(move || session::run(sid, run, cfg, rx, sinfo))
        .map_err(|e| e.to_string())?;
    st.sessions
        .lock()
        .map_err(|_| "session table poisoned".to_string())?
        .insert(id.clone(), SessionHandle { tx, info });
    Ok(SessionCreated {
        ws: format!("/sessions/{id}/ws?protocol={PROTOCOL_VERSION}"),
        id,
        scenario: scenario.name,
        initial_run_hash,
    })
}

async fn create_session(
    State(st): State<Arc<AppState>>,
    Json(req): Json<NewSession>,
) -> Result<Json<SessionCreated>, ApiError> {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || start_session(&st2, req))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(bad)
}

async fn list_sessions(State(st): State<Arc<AppState>>) -> Json<Vec<SessionInfo>> {
    let s = st.sessions.lock().map(|s| {
        s.values()
            .filter_map(|h| h.info.lock().ok().map(|i| i.clone()))
            .collect()
    });
    Json(s.unwrap_or_default())
}

async fn delete_session(
    State(st): State<Arc<AppState>>,
    UrlPath(id): UrlPath<String>,
) -> StatusCode {
    let removed = st.sessions.lock().ok().and_then(|mut s| s.remove(&id));
    match removed {
        Some(h) => {
            let _ = h.tx.send(Inbound::Shutdown);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    protocol: Option<u32>,
}

async fn ws(
    State(st): State<Arc<AppState>>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    if q.protocol != Some(PROTOCOL_VERSION) {
        return Err(bad(format!(
            "protocol version mismatch: the server speaks {PROTOCOL_VERSION}, the client asked for {:?}",
            q.protocol
        )));
    }
    let tx = st
        .sessions
        .lock()
        .ok()
        .and_then(|s| s.get(&id).map(|h| h.tx.clone()))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no session `{id}`")))?;
    let conn = st.next_conn.fetch_add(1, Ordering::Relaxed);
    Ok(upgrade
        .max_message_size(1 << 20)
        .on_upgrade(move |socket| client(socket, tx, conn)))
}

/// Client queue length; frames leave room for text (see `session`).
const CLIENT_QUEUE: usize = 128;

async fn client(socket: WebSocket, tx: std::sync::mpsc::Sender<Inbound>, conn: u64) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Outbound>(CLIENT_QUEUE);
    let errors = out_tx.clone();
    if tx.send(Inbound::Connected(conn, out_tx)).is_err() {
        return;
    }
    let writer = tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            let msg = match m {
                Outbound::Text(t) => Message::Text(t.into()),
                Outbound::Binary(b) => Message::Binary(b),
            };
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(t) => match serde_json::from_str::<ClientMessage>(&t) {
                Ok(m) => {
                    if tx.send(Inbound::Client(m)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let m = ServerMessage::Error {
                        message: format!("bad command: {e}"),
                    };
                    if let Ok(t) = serde_json::to_string(&m) {
                        let _ = errors.send(Outbound::Text(t)).await;
                    }
                }
            },
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = tx.send(Inbound::Disconnected(conn));
    writer.abort();
}

#[derive(Debug, Serialize)]
struct RunListing {
    /// Folder relative to the runs directory (also its `/files/` URL).
    path: String,
    scenario: String,
    seed: u64,
    run_hash: String,
    status: String,
    output_hash: String,
    experiment: Option<marg_io::RunMeta>,
    live: bool,
    tables: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ExperimentListing {
    path: String,
    name: String,
    runs: usize,
    arms: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Listing {
    root: String,
    runs: Vec<RunListing>,
    experiments: Vec<ExperimentListing>,
}

/// Folders deeper than this are not listed.
const LIST_DEPTH: usize = 10;

fn scan(root: &Path, dir: &Path, depth: usize, out: &mut Listing) {
    let rel = dir
        .strip_prefix(root)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/");
    if dir.join("manifest.json").is_file()
        && let Ok(m) = marg_io::RunManifest::read(dir)
    {
        let mut tables: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.strip_suffix(".parquet").map(str::to_string)
            })
            .collect();
        tables.sort();
        out.runs.push(RunListing {
            path: rel.clone(),
            scenario: m.scenario,
            seed: m.seed,
            run_hash: m.run_hash,
            status: format!("{:?}", m.status).to_lowercase(),
            output_hash: m.output_hash,
            experiment: m.experiment,
            live: m.live.is_some(),
            tables,
        });
    }
    if let Ok(entries) = marg_io::batch::read_run_list(&dir.join("runs.jsonl")) {
        let mut arms: Vec<String> = entries.iter().map(|e| e.arm.clone()).collect();
        arms.sort();
        arms.dedup();
        out.experiments.push(ExperimentListing {
            path: rel,
            name: entries
                .first()
                .map(|e| e.experiment.clone())
                .unwrap_or_default(),
            runs: entries.len(),
            arms,
        });
    }
    if depth >= LIST_DEPTH {
        return;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter(|p| {
            let n = p.file_name().unwrap_or_default().to_string_lossy();
            !n.starts_with('.') && !n.contains(".partial-") && n != "params" && n != "calibration"
        })
        .collect();
    subdirs.sort();
    for d in subdirs {
        scan(root, &d, depth + 1, out);
    }
}

async fn runs(State(st): State<Arc<AppState>>) -> Json<Listing> {
    let root = st.cfg.runs_dir.clone();
    let listing = tokio::task::spawn_blocking(move || {
        let mut out = Listing {
            root: root.display().to_string(),
            runs: Vec::new(),
            experiments: Vec::new(),
        };
        scan(&root, &root, 0, &mut out);
        out
    })
    .await
    .unwrap_or(Listing {
        root: String::new(),
        runs: Vec::new(),
        experiments: Vec::new(),
    });
    Json(listing)
}

/// JSON Schema of the viewer protocol (for `marg schema export`).
pub fn protocol_schema() -> schemars::Schema {
    schemars::schema_for!(protocol::ViewerProtocol)
}
