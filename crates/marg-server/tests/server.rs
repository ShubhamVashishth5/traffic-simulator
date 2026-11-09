//! The live server end to end (plan P4.1, P4.2, P4.4, P4.6): a session over
//! WebSocket, replayed headless; the inspector against `agents.parquet`; the
//! dashboard windows against the observers; frames and the version check.

use arrow::array::{Array, Float64Array, StringArray, UInt64Array};
use futures_util::{SinkExt, StreamExt};
use marg_server::{AppState, ServerConfig, router};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

const SCENARIO: &str = r#"
name: ws_test
duration_s: 200
warmup_s: 20
network:
  ribbons:
    - {id: main, polyline: [[100, 50], [700, 50]], width: [{s: 0, y_min: -5.25, y_max: 5.25}]}
demand:
  entries:
    - {id: w, ribbon: main, flow: [{from_s: 0, veh_per_h: 2400}],
       composition: {hatchback: 0.5, sedan: 0.1, suv: 0.05, motorcycle: 0.25, auto_rickshaw: 0.1}}
detectors:
  - {kind: screenline, id: s400, ribbon: main, s: 400, interval_s: 60}
  - {kind: area, id: a, ribbon: main, s_from: 100, s_to: 500, interval_s: 60}
kpi: {throughput_detector: s400}
outputs:
  trajectories: {every_s: 1.0}
"#;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("marg-server-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

async fn start(runs: &Path) -> SocketAddr {
    let mut cfg = ServerConfig::new(repo().join("params/base"));
    cfg.scenarios_dir = repo().join("scenarios");
    cfg.runs_dir = runs.to_path_buf();
    let app = router(AppState::new(cfg));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

/// Minimal HTTP/1.1 client: `(status, body)`.
async fn http(addr: SocketAddr, method: &str, path: &str, body: Option<Value>) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text[9..12].parse().unwrap();
    let (_, rest) = text.split_once("\r\n\r\n").unwrap_or(("", ""));
    // Bodies may be chunked; strip chunk sizes for JSON replies.
    let body = if text
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut r = rest;
        while let Some((size, tail)) = r.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            out.push_str(&tail[..n]);
            r = &tail[n + 2..];
        }
        out
    } else {
        rest.to_string()
    };
    (status, body)
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn session(addr: SocketAddr, body: Value) -> (String, Ws) {
    let (status, reply) = http(addr, "POST", "/sessions", Some(body)).await;
    assert_eq!(status, 200, "{reply}");
    let created: Value = serde_json::from_str(&reply).unwrap();
    let url = format!("ws://{addr}{}", created["ws"].as_str().unwrap());
    let (ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    (created["id"].as_str().unwrap().to_string(), ws)
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

/// Read messages until `pred` accepts a JSON one; frames are counted.
async fn until(
    ws: &mut Ws,
    frames: &mut Vec<Vec<u8>>,
    mut pred: impl FnMut(&Value) -> bool,
) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let msg = tokio::time::timeout_at(deadline, ws.next())
            .await
            .expect("timed out waiting for a message")
            .unwrap()
            .unwrap();
        match msg {
            Message::Text(t) => {
                let v: Value = serde_json::from_str(&t).unwrap();
                if pred(&v) {
                    return v;
                }
            }
            Message::Binary(b) => frames.push(b.to_vec()),
            _ => {}
        }
    }
}

fn is(kind: &'static str) -> impl FnMut(&Value) -> bool {
    move |v| v["type"] == kind
}

fn decode(frame: &[u8]) -> arrow::record_batch::RecordBatch {
    let mut r =
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(frame), None).unwrap();
    r.next().unwrap().unwrap()
}

fn read_parquet(path: &Path) -> arrow::record_batch::RecordBatch {
    let f = std::fs::File::open(path).unwrap();
    let r = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(f)
        .unwrap()
        .build()
        .unwrap();
    let batches: Vec<_> = r.map(|b| b.unwrap()).collect();
    arrow::compute::concat_batches(&batches[0].schema(), &batches).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_session_replays_headless_and_the_inspector_matches_agents_parquet() {
    let runs = tmp("replay");
    let addr = start(&runs).await;
    let (id, mut ws) = session(addr, json!({"yaml": SCENARIO, "seed": 4})).await;
    let mut frames = Vec::new();
    let hello = until(&mut ws, &mut frames, is("hello")).await;
    assert_eq!(hello["data"]["protocol"], 1);
    assert_eq!(hello["data"]["origin"], json!([100.0, 50.0]));
    assert_eq!(hello["data"]["ribbons"][0]["id"], "main");

    send(
        &mut ws,
        json!({"cmd": "view", "fps": 60, "colour": "speed"}),
    )
    .await;
    send(&mut ws, json!({"cmd": "step", "n": 400})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "step"
    })
    .await;
    // Change car desired speeds live (one with resampling), and a refused change.
    let fast = json!({"dist": "trunc_normal", "mean": 78.0, "sd": 15.52, "min": 15, "max": 120});
    send(&mut ws, json!({"cmd": "set_param", "path": "vehicles.free_flow_speed.hatchback", "value": fast, "resample": true})).await;
    send(
        &mut ws,
        json!({"cmd": "set_param", "path": "vehicles.free_flow_speed.sedan", "value": fast}),
    )
    .await;
    send(
        &mut ws,
        json!({"cmd": "set_param", "path": "core.lookahead_distance", "value": 99}),
    )
    .await;
    let refused = until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["ok"] == false
    })
    .await;
    assert!(
        refused["data"]["message"]
            .as_str()
            .unwrap()
            .contains("not live")
    );
    let history = until(&mut ws, &mut frames, |v| {
        v["type"] == "history"
            && v["data"]["changes"]
                .as_array()
                .is_some_and(|c| c.len() == 2)
    })
    .await;
    assert_eq!(
        history["data"]["changes"][1]["path"],
        "vehicles.free_flow_speed.sedan"
    );

    // Select an agent that is in the network now.
    send(&mut ws, json!({"cmd": "step", "n": 1})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "step"
    })
    .await;
    let last = decode(frames.last().unwrap());
    assert!(last.num_rows() > 10);
    let ids = last
        .column_by_name("id")
        .unwrap()
        .as_any()
        .downcast_ref::<UInt64Array>()
        .unwrap();
    let pick = ids.value(ids.len() / 2);
    send(
        &mut ws,
        json!({"cmd": "select_agent", "id": pick.to_string()}),
    )
    .await;
    let ins = until(&mut ws, &mut frames, is("inspector")).await;
    assert_eq!(ins["data"]["found"], true);
    assert_eq!(ins["data"]["id"], pick.to_string());
    assert!(
        !ins["data"]["neighbours"].as_array().unwrap().is_empty()
            || ins["data"]["state"]["s"].as_f64().unwrap() > 300.0
    );

    // Play flat out for a moment, undo the sedan change, then a breakpoint at t = 120 s.
    send(&mut ws, json!({"cmd": "undo"})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "history"
            && v["data"]["changes"]
                .as_array()
                .is_some_and(|c| c.len() == 3)
    })
    .await;
    send(
        &mut ws,
        json!({"cmd": "breakpoint", "on": {"on": "time", "t": 120.0}}),
    )
    .await;
    send(&mut ws, json!({"cmd": "speed", "x": 0})).await;
    send(&mut ws, json!({"cmd": "play"})).await;
    let bp = until(&mut ws, &mut frames, is("breakpoint")).await;
    assert!(
        (bp["data"]["t"].as_f64().unwrap() - 120.0).abs() < 1e-6,
        "{bp}"
    );
    send(
        &mut ws,
        json!({"cmd": "toggle_signal_state", "junction": "j", "state": "dark"}),
    )
    .await;
    let stub = until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "toggle_signal_state"
    })
    .await;
    assert_eq!(stub["data"]["ok"], false);
    send(&mut ws, json!({"cmd": "step", "n": 5})).await;
    send(&mut ws, json!({"cmd": "save"})).await;
    let saved = until(&mut ws, &mut frames, |v| {
        v["type"] == "saved" || v["type"] == "error"
    })
    .await;
    assert_eq!(saved["type"], "saved", "{saved}");
    assert_eq!(saved["data"]["replay_matches"], true, "{saved}");
    assert_eq!(
        saved["data"]["output_hash"],
        saved["data"]["replay_output_hash"]
    );
    assert_eq!(saved["data"]["steps"], 1205);

    // The command log: every command, stamped; four of them change the run.
    let dir = runs.join(saved["data"]["path"].as_str().unwrap());
    let log = marg_io::live::read_commands(&dir.join("commands.parquet")).unwrap();
    let names: Vec<&str> = log.iter().map(|c| c.cmd.as_str()).collect();
    assert_eq!(
        names,
        [
            "step",
            "set_param",
            "resample",
            "set_param",
            "set_param",
            "step",
            "select_agent",
            "set_param",
            "breakpoint",
            "speed",
            "play",
            "toggle_signal_state",
            "step",
            "save"
        ]
    );
    assert_eq!(log.iter().filter(|c| c.changes_state()).count(), 4);
    assert!(log[1].step == 400 && (log[1].t - 40.0).abs() < 1e-9);
    assert!(log[7].args.contains("undo_of"));

    // The inspector shows what agents.parquet of that run holds.
    let agents = read_parquet(&dir.join("agents.parquet"));
    let col = |n: &str| agents.column_by_name(n).unwrap().clone();
    let id_col = col("agent_id");
    let id_col = id_col.as_any().downcast_ref::<UInt64Array>().unwrap();
    let row = (0..agents.num_rows())
        .find(|&i| id_col.value(i) == pick)
        .unwrap();
    let rec = &ins["data"]["record"];
    for f in [
        "t_arrival",
        "t_entry",
        "spawn_delay",
        "entry_y",
        "length",
        "width",
        "v0",
        "time_gap",
        "standstill_gap",
        "max_accel",
        "comfortable_decel",
    ] {
        let c = col(f);
        let x = c
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(row);
        assert_eq!(rec[f].as_f64().unwrap(), x, "{f}");
    }
    for f in ["vehicle_type", "class", "entry"] {
        let c = col(f);
        assert_eq!(
            rec[f].as_str().unwrap(),
            c.as_any().downcast_ref::<StringArray>().unwrap().value(row),
            "{f}"
        );
    }
    let (status, list) = http(addr, "GET", "/runs", None).await;
    assert_eq!(status, 200);
    assert!(list.contains(&id), "{list}");
    let (status, _) = http(
        addr,
        "GET",
        &format!(
            "/files/{}/agents.parquet",
            saved["data"]["path"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let _ = std::fs::remove_dir_all(&runs);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dashboard_windows_equal_the_observers_for_the_same_interval() {
    let runs = tmp("windows");
    let addr = start(&runs).await;
    let (_, mut ws) = session(
        addr,
        json!({"yaml": SCENARIO, "seed": 8, "live_window_s": 20}),
    )
    .await;
    let mut frames = Vec::new();
    until(&mut ws, &mut frames, is("hello")).await;
    send(&mut ws, json!({"cmd": "speed", "x": 0})).await;
    send(&mut ws, json!({"cmd": "play"})).await;
    let mut windows: Vec<Value> = Vec::new();
    let mut last_kpis;
    loop {
        let v = until(&mut ws, &mut frames, |v| {
            v["type"] == "kpi" || v["type"] == "status"
        })
        .await;
        if v["type"] == "kpi" {
            windows.extend(v["data"]["windows"].as_array().unwrap().iter().cloned());
            last_kpis = v["data"]["kpis"].clone();
            if v["data"]["t"].as_f64().unwrap() >= 200.0 - 1e-6 {
                break;
            }
        }
    }
    // Headless, with the detectors at the live window's interval.
    let reg = marg_params::Register::load_dir(&repo().join("params/base")).unwrap();
    let sc =
        marg_params::Scenario::from_yaml(&SCENARIO.replace("interval_s: 60", "interval_s: 20"))
            .unwrap();
    let spec = marg_params::resolve(&reg, &sc, &Default::default(), Default::default()).unwrap();
    let mut sim = marg_core::Simulation::new(&spec, 8, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    let mut expected: Vec<Value> = serde_json::to_value(marg_server::session::rows(out.detectors))
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    // Live windows arrive interval by interval; the table is detector by detector.
    let key = |v: &Value| {
        (
            v["detector_id"].to_string(),
            v["t_start"].as_f64().unwrap().to_bits(),
            v["vehicle_type"].to_string(),
        )
    };
    windows.sort_by_key(key);
    expected.sort_by_key(key);
    assert!(windows.len() > 50);
    assert_eq!(windows, expected);
    // And the KPI strip at the end equals kpis.parquet's computation.
    for (k, v) in &out.kpis {
        let live = last_kpis
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == *k)
            .unwrap();
        if v.is_finite() {
            assert_eq!(live["value"].as_f64().unwrap(), *v, "{k}");
        } else {
            assert!(live["value"].is_null(), "{k}");
        }
    }
    let _ = std::fs::remove_dir_all(&runs);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_are_compact_culled_and_versioned() {
    let runs = tmp("frames");
    let addr = start(&runs).await;
    let (_, mut ws) = session(addr, json!({"scenario": "corridor_2km.yaml", "seed": 2})).await;
    let mut frames = Vec::new();
    until(&mut ws, &mut frames, is("hello")).await;
    send(&mut ws, json!({"cmd": "step", "n": 3000})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "step"
    })
    .await;
    send(&mut ws, json!({"cmd": "view", "colour": "speed"})).await;
    send(&mut ws, json!({"cmd": "step", "n": 1})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "step"
    })
    .await;
    let n_frames = frames.len();
    until(&mut ws, &mut frames, is("kpi")).await;
    let full = frames[n_frames.max(1) - 1].clone();
    let b = decode(&full);
    let meta = b.schema().metadata().clone();
    assert_eq!(meta["protocol"], "1");
    let total: usize = meta["total"].parse().unwrap();
    assert_eq!(b.num_rows(), total);
    assert!(total > 100, "{total} agents");
    // 33 bytes per agent (u64 id, 5 × f32, u8 class, f32 colour) plus a fixed schema header.
    assert!(
        full.len() < 34 * total + 2048,
        "{} bytes for {total} agents",
        full.len()
    );
    // Cull to the first 500 m.
    send(
        &mut ws,
        json!({"cmd": "view", "viewport": [0.0, -20.0, 500.0, 20.0]}),
    )
    .await;
    send(&mut ws, json!({"cmd": "step", "n": 1})).await;
    until(&mut ws, &mut frames, |v| {
        v["type"] == "ack" && v["data"]["cmd"] == "step"
    })
    .await;
    until(&mut ws, &mut frames, is("kpi")).await;
    let culled = decode(frames.last().unwrap());
    assert!(culled.num_rows() < total / 2 && culled.num_rows() > 10);
    let x = culled.column_by_name("x").unwrap();
    let x = x
        .as_any()
        .downcast_ref::<arrow::array::Float32Array>()
        .unwrap();
    assert!((0..x.len()).all(|i| x.value(i) <= 525.0));
    // A client speaking another protocol version is refused.
    let (status, body) = http(addr, "GET", "/sessions/x/ws?protocol=99", None).await;
    assert_eq!(status, 400, "{body}");
    let _ = std::fs::remove_dir_all(&runs);
}

/// P4.2 benchmark: 20,000 agents at 30 fps on localhost under 30 MB/s.
/// `cargo test --release -p marg-server --test server frame_rate -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
async fn frame_rate_benchmark_20k_agents() {
    let runs = tmp("bench");
    let addr = start(&runs).await;
    let yaml = std::fs::read_to_string(repo().join("benches/viewer_20k.yaml")).unwrap();
    let (_, mut ws) = session(addr, json!({"yaml": yaml, "seed": 1, "threads": 4})).await;
    let mut frames = Vec::new();
    until(&mut ws, &mut frames, is("hello")).await;
    let warm = std::time::Instant::now();
    let mut agents = 0;
    while agents < 20_000 {
        send(&mut ws, json!({"cmd": "step", "n": 500})).await;
        until(&mut ws, &mut frames, |v| {
            v["type"] == "ack" && v["data"]["cmd"] == "step"
        })
        .await;
        let st = until(&mut ws, &mut frames, is("status")).await;
        agents = st["data"]["agents"].as_u64().unwrap();
        assert!(
            st["data"]["t"].as_f64().unwrap() < 3000.0,
            "never reached 20,000 agents ({agents})"
        );
    }
    eprintln!(
        "warm-up: {agents} agents after {:.1} s wall",
        warm.elapsed().as_secs_f64()
    );
    send(
        &mut ws,
        json!({"cmd": "view", "fps": 30, "colour": "speed"}),
    )
    .await;
    // 1× real time gives 10 new states a second at dt = 0.1 s (the viewer interpolates between
    // them); 5× gives 50, decimated to the viewer's 30 fps (plan/02: 30 fps at 1× to 5×).
    let mut reports = Vec::new();
    for speed in [1.0, 5.0] {
        send(&mut ws, json!({"cmd": "speed", "x": speed})).await;
        send(&mut ws, json!({"cmd": "play"})).await;
        // Skip the first second, then measure for ten.
        let t0 = tokio::time::Instant::now() + Duration::from_secs(1);
        let t1 = t0 + Duration::from_secs(10);
        let (mut n, mut bytes, mut rows, mut min_rows) = (0u64, 0u64, 0u64, u64::MAX);
        let mut sim_t = (f64::NAN, f64::NAN);
        while tokio::time::Instant::now() < t1 {
            let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if let Message::Binary(b) = msg
                && tokio::time::Instant::now() >= t0
            {
                let batch = decode(&b);
                let t: f64 = batch.schema().metadata()["t"].parse().unwrap();
                if sim_t.0.is_nan() {
                    sim_t.0 = t;
                }
                sim_t.1 = t;
                let r = batch.num_rows() as u64;
                n += 1;
                bytes += b.len() as u64;
                rows += r;
                min_rows = min_rows.min(r);
            }
        }
        send(&mut ws, json!({"cmd": "pause"})).await;
        let report = json!({
            "speed": speed,
            "frames_per_s": n as f64 / 10.0,
            "mb_per_s": bytes as f64 / 10.0 / 1e6,
            "mean_agents_per_frame": rows as f64 / n as f64,
            "min_agents_per_frame": min_rows,
            "bytes_per_agent": bytes as f64 / rows as f64,
            "real_time_factor": (sim_t.1 - sim_t.0) / 10.0,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        reports.push(report);
    }
    let fast = &reports[1];
    assert!(
        fast["min_agents_per_frame"].as_u64().unwrap() >= 20_000,
        "{fast}"
    );
    assert!(fast["frames_per_s"].as_f64().unwrap() >= 29.0, "{fast}");
    assert!(fast["mb_per_s"].as_f64().unwrap() < 30.0, "{fast}");
    assert!(
        reports[0]["frames_per_s"].as_f64().unwrap() >= 9.5,
        "every state at 1×"
    );
    let _ = std::fs::remove_dir_all(&runs);
}
