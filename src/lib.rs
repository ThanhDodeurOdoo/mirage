//! Tiny test pictures, real H.264. No camera needed.
//!
//! Pick a [`Pattern::Checkerboard`] or a [`Pattern::MovingRectangle`] with a
//! little frame counter. [`Generator`] draws grayscale I420 pixels and encodes
//! them with OpenH264. You handle pacing, files and networking.
//!
//! # Give it a spin
//!
//! ```
//! use mirage::{Config, EncodeOutcome, Generator, Pattern};
//! let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle)?;
//! let mut video = Generator::new(config)?;
//! for _ in 0..60 {
//!     match video.generate()? {
//!         EncodeOutcome::Emitted { metadata, bytes, .. } => {
//!             println!("frame {}: {} H.264 bytes", metadata.source_index, bytes.len());
//!         }
//!         EncodeOutcome::Skipped { metadata } => {
//!             println!("frame {} skipped", metadata.source_index);
//!         }
//!     }
//! }
//! let pixels = video.raw_frame().unwrap();
//! assert_eq!(pixels.y().len(), 320 * 240);
//! # Ok::<(), mirage::Error>(())
//! ```
//!
//! [`Config::default`] selects a 320 x 240 checkerboard with a 1 Mbps target.
//! Custom dimensions must be even, from 16 x 16 through 1920 x 1080. Bitrate
//! is a target in bits per second, from 1 through `i32::MAX`.
//!
//! Each completed call advances the source index, including skips. Indices start
//! at zero and timestamps follow 60 Hz, whatever speed you call `generate`.
//! Those timestamps are metadata, not timing embedded in the H.264 bytes.
//!
//! Emitted bytes are yours to keep across later calls. [`Generator::raw_frame`]
//! borrows the latest compact I420 planes: Y at full size, U and V at half width
//! and height. It returns `None` before the first completed step or after an
//! encoder failure. Finish using that borrow before generating more pixels.
//!
//! # When things go wrong
//!
//! [`Config::new`] can return [`Error::InvalidDimensions`], [`Error::InvalidBitrate`]
//! or [`Error::FrameSizeOverflow`]. Construction and encoding can return
//! [`Error::Encoder`]. A failed encoder leaves the generator faulted, so later
//! calls return [`Error::Faulted`]. Create a new generator to start over.
//! [`Error::TimelineExhausted`] leaves the last frame untouched.

#![forbid(unsafe_code)]

mod config;
mod encoder;
mod error;
mod frame;
mod generator;
mod pattern;
mod timeline;

pub use config::{Config, Pattern, FRAMES_PER_SECOND};
pub use error::Error;
pub use frame::{EncodeOutcome, FrameKind, FrameMetadata, RawFrame};
pub use generator::Generator;
