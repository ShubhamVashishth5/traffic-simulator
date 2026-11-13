//! Ribbon road geometry (ADR-0002), and later road objects and zone fields.
//!
//! A ribbon is a reference polyline with arc length `s` and a lateral
//! coordinate `y` (positive to the left of travel) bounded by a width profile.
//! Agents live in `(ribbon, s, y)`; world coordinates are only for output,
//! the viewer and cross-ribbon interaction.

pub mod junction;
pub mod network;
pub mod ribbon;

pub use junction::{Arm, ConflictArea, ConflictKind, Control, Junction, Movement, Phase, Turn};
pub use network::{NetError, Network};
pub use ribbon::Ribbon;
