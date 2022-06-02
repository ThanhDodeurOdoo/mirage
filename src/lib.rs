//! Small test pictures, real video. No camera needed.
//!
//! Generate I420 and encoded video in memory at 60 Hz. You handle pacing, files and networking.
//! H.264 is enabled by default. For VP8, enable `vp8` and select [`Codec::Vp8`].
//!
//! ```rust
//! # #[cfg(feature = "h264")]
//! # {
//! use mirage::{Config, EncodeOutcome, Generator};
//! let mut video = Generator::new(Config::default())?;
//! for _ in 0..60 {
//!     if let EncodeOutcome::Emitted { bytes, .. } = video.generate()? {
//!         println!("{} H.264 bytes", bytes.len());
//!     }
//! }
//! # }
//! # Ok::<(), mirage::Error>(())
//! ```
//!
//! - Choose pictures with [`Pattern`] or `Generator::with_scenes`.
//! - Request a refresh with `Generator::request_refresh`.
//! - Identify decoded frames with [`Config::with_identity`] and [`read_identity`].
//! - Compare delivery and recovery with [`compare_frames`].

#![forbid(unsafe_code)]
#![deny(clippy::missing_errors_doc, clippy::missing_panics_doc)]

mod clip;
mod compare;
mod config;
#[cfg(any(feature = "h264", feature = "vp8"))]
mod encoder;
mod error;
mod frame;
#[cfg(any(feature = "h264", feature = "vp8"))]
mod generator;
mod h264;
mod identity;
#[cfg(any(feature = "h264", feature = "vp8", test))]
mod pattern;
mod timeline;

pub use clip::{Clip, ClipFrame};
pub use compare::{compare_frames, Comparison, Observation, RefreshRequest};
pub use config::{Codec, Config, Pattern, FRAMES_PER_SECOND};
pub use error::Error;
pub use frame::{EncodeOutcome, FrameKind, FrameMetadata, RawFrame};
#[cfg(any(feature = "h264", feature = "vp8"))]
pub use generator::Generator;
pub use h264::{inspect_picture, nal_units, NalUnit, NalUnits, PictureHeaders, Sps};
pub use identity::{read_identity, FrameIdentity};
pub use timeline::clock_ticks;
