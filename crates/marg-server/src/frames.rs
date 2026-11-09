//! Binary agent frames (plan P4.2): one Arrow IPC stream per WebSocket
//! message, holding the schema (with frame metadata) and one record batch.
//!
//! Positions are `Float32` relative to the session origin (`Hello.origin`),
//! which keeps centimetre precision even for UTM coordinates. Agents outside
//! the viewer's viewport (plus a margin) are not sent.

use crate::protocol::{ColourBy, FRAME_COLUMNS, PROTOCOL_VERSION};
use arrow::array::{ArrayRef, Float32Array, UInt8Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use marg_core::Simulation;
use std::collections::HashMap;
use std::sync::Arc;

/// Margin around the viewport inside which agents are still sent (m): the
/// longest vehicle, so nothing pops in at the edge.
pub const CULL_MARGIN_M: f64 = 25.0;

/// What a frame should contain.
#[derive(Debug, Clone, Copy)]
pub struct FrameRequest {
    pub origin: [f64; 2],
    pub viewport: Option<[f64; 4]>,
    pub colour: ColourBy,
}

/// An encoded frame.
pub struct Frame {
    pub bytes: Vec<u8>,
    /// Agents in the frame.
    pub sent: usize,
    /// Agents in the network.
    pub total: usize,
}

fn colour_value(sim: &Simulation, i: usize, by: ColourBy) -> f32 {
    let a = sim.agents();
    (match by {
        ColourBy::Class => f64::from(a.class[i]),
        ColourBy::Speed => a.v[i],
        ColourBy::SpeedRatio => a.v[i] / a.v0[i],
        ColourBy::Accel => a.acc[i],
        ColourBy::Lateral => a.y[i],
        ColourBy::DesiredSpeed => a.v0[i],
        ColourBy::Persona | ColourBy::Kappa | ColourBy::Violation | ColourBy::Zone => f64::NAN,
    }) as f32
}

/// Build and encode the frame for the current state.
pub fn encode(sim: &Simulation, req: &FrameRequest) -> Result<Frame, arrow::error::ArrowError> {
    let a = sim.agents();
    let net = sim.network();
    let n = a.len();
    let mut id = Vec::with_capacity(n);
    let (mut x, mut y, mut heading) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    let (mut length, mut width, mut class, mut colour) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    let vp = req.viewport.map(|[x0, y0, x1, y1]| {
        [
            x0.min(x1) - CULL_MARGIN_M,
            y0.min(y1) - CULL_MARGIN_M,
            x0.max(x1) + CULL_MARGIN_M,
            y0.max(y1) + CULL_MARGIN_M,
        ]
    });
    for i in 0..n {
        let (wx, wy, h) = net.ribbon(a.ribbon[i] as usize).to_world(a.s[i], a.y[i]);
        let (fx, fy) = (wx - req.origin[0], wy - req.origin[1]);
        if let Some([x0, y0, x1, y1]) = vp
            && !(fx >= x0 && fx <= x1 && fy >= y0 && fy <= y1)
        {
            continue;
        }
        id.push(a.id[i]);
        x.push(fx as f32);
        y.push(fy as f32);
        heading.push(h as f32);
        length.push(a.length[i] as f32);
        width.push(a.width[i] as f32);
        class.push(a.class[i]);
        colour.push(colour_value(sim, i, req.colour));
    }
    let sent = id.len();
    let meta: HashMap<String, String> = [
        ("protocol", PROTOCOL_VERSION.to_string()),
        ("t", sim.time().to_string()),
        ("step", sim.step_index().to_string()),
        ("total", n.to_string()),
        (
            "colour",
            serde_json::to_value(req.colour)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let f32_field = |name: &str| Field::new(name, DataType::Float32, false);
    let schema = Arc::new(
        Schema::new(vec![
            Field::new(FRAME_COLUMNS[0], DataType::UInt64, false),
            f32_field(FRAME_COLUMNS[1]),
            f32_field(FRAME_COLUMNS[2]),
            f32_field(FRAME_COLUMNS[3]),
            f32_field(FRAME_COLUMNS[4]),
            f32_field(FRAME_COLUMNS[5]),
            Field::new(FRAME_COLUMNS[6], DataType::UInt8, false),
            Field::new(FRAME_COLUMNS[7], DataType::Float32, true),
        ])
        .with_metadata(meta),
    );
    let cols: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from(id)),
        Arc::new(Float32Array::from(x)),
        Arc::new(Float32Array::from(y)),
        Arc::new(Float32Array::from(heading)),
        Arc::new(Float32Array::from(length)),
        Arc::new(Float32Array::from(width)),
        Arc::new(UInt8Array::from(class)),
        Arc::new(Float32Array::from(colour)),
    ];
    let batch = RecordBatch::try_new(schema.clone(), cols)?;
    let mut w = StreamWriter::try_new(Vec::with_capacity(64 + sent * 34), &schema)?;
    w.write(&batch)?;
    w.finish()?;
    Ok(Frame {
        bytes: w.into_inner()?,
        sent,
        total: n,
    })
}
