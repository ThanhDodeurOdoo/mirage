//! experiment in synthetic video generation
//!
//! generates pixels and encodes video in memory. Callers drive
//! each step and handle file output, network transport and real-time scheduling.

#![forbid(unsafe_code)]

mod config;
mod error;
mod frame;

pub use config::{Config, Pattern, FRAMES_PER_SECOND};
pub use error::Error;
pub use frame::{EncodeOutcome, FrameKind, FrameMetadata, RawFrame};
