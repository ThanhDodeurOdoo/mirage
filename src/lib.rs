//! Caller-driven H.264. Generator API planned:
//! - `Generator::new(Config) -> Result<Generator, Error>`: `Error::Encoder`.
//! - `generate(&mut self) -> Result<EncodeOutcome, Error>`:
//!   `Error::Encoder`, `Error::Faulted` or `Error::TimelineExhausted`.
//! - `raw_frame(&self) -> Option<RawFrame<'_>>`: borrowed pixels.
//!
//! Fixed config, no I/O. Start at frame zero, advance on emitted or skipped output.
//! Encoder failure faults the stream. Create a new generator to restart.
//! Raw pixels are absent before a completed step and after encoder failure.
//! Raw borrows prevent generation. Emitted bytes outlive later calls.

#![forbid(unsafe_code)]

mod config;
mod error;
mod frame;
mod pattern;
mod timeline;

pub use config::{Config, Pattern, FRAMES_PER_SECOND};
pub use error::Error;
pub use frame::{EncodeOutcome, FrameKind, FrameMetadata, RawFrame};
