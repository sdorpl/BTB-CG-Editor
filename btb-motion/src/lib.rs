//! btb-motion: deklaratywny model animacji (Director / Actor / Channel / Key / Event) i jego
//! deterministyczna ewaluacja `t -> wartości`. Czysty Rust, bez zależności od egui, więc ten sam
//! kod można skompilować natywnie (edytor) i do WASM (output.html w OBS).
//!
//! Wzorowany na modelu Stage z Viz Artist 5.5, ale czas jest w sekundach (f32), a nie w polach.
pub mod channel;
pub mod curve;
pub mod director;
pub mod ease;
pub mod path;
pub mod text;
pub mod transport;

pub use channel::{Channel, Extend, Key};
pub use curve::{Ease, Handle};
pub use director::{Actor, Dir, Director, Event, EventKind, Sample, TextTrack, UnitSample};
pub use path::{PathChannel, PathKey};
pub use text::{Mode as TextMode, Props as TextProps, TextSpec, Units};
pub use transport::{Fired, Transport};
