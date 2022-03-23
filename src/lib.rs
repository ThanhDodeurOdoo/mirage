//! Small test pictures, real H.264. No camera needed.
//!
//! Pick a [`Pattern::Checkerboard`], a [`Pattern::MovingRectangle`] with a
//! little frame counter or seeded [`Pattern::Texture`] noise.
//! [`Generator`] draws grayscale I420 pixels and encodes
//! them with OpenH264. You handle pacing, files and networking.
//!
//! # use like that
//!
//! ```rust
//! use mirage::{clock_ticks, read_identity, Config, EncodeOutcome, Generator, Pattern};
//! let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle)?.with_identity(42)?;
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
//! let identity = read_identity(pixels.y(), 320, 240, 320)?.unwrap();
//! assert_eq!((identity.source_id, identity.source_index), (42, 59));
//! let next = video.next_metadata()?;
//! assert_eq!(next.source_index, 60);
//! assert_eq!(clock_ticks(next.source_index, 90_000)?, 90_000);
//! # Ok::<(), mirage::Error>(())
//! ```
//!
//! [`Config::default`] selects a 320 x 240 checkerboard with a 1 Mbps target.
//! Custom dimensions must be even, from 16 x 16 through 1920 x 1080. Bitrate
//! is a target in bits per second, from 1 through `i32::MAX`.
//! `Pattern::Texture { seed: 42, changing: false }` keeps the same grainy picture.
//! Set `changing: true` for fresh noise at each source index. A fixed wrapping
//! `u64` hash produces repeatable luma in 16..=235 with neutral chroma. These
//! pixel guarantees do not fix encoded bytes, bitrate or skips.
//! [`Generator::with_scenes`] takes a `Vec<(u64, Pattern)>` of scene starts,
//! replacing the configured pattern. Empty lists, a nonzero first start or
//! non-increasing starts return [`Error::InvalidScenes`]. The last scene continues.
//! Cuts use global source indices, including skips, and do not request refresh.
//! Opt in with `config.with_identity(source_id)?` for a card carrying the full
//! `u32` source ID and `u64` frame index. It overlays every scene, including skips.
//! The fixed 8-pixel cells need at least 152 x 88 pixels or configuration returns
//! [`Error::IdentityCardTooSmall`]. References and a 16-bit checksum help detect
//! damage, but promise neither authentication nor recovery after scaling or at
//! every bitrate. Cards are disabled by default.
//! [`read_identity`] accepts borrowed luma, width, height and row stride.
//! Invalid dimensions, stride, length or arithmetic return [`Error::InvalidLumaLayout`].
//! A valid picture too small for the card, ambiguous contrast or failed checks
//! returns `None`. It samples unscaled cell centers and returns a copied identity.
//! A readable card says nothing about damage elsewhere in the picture.
//!
//! [`compare_frames`] compares one source's expected outcomes with timed
//! [`Observation`] records. Supply only outcomes produced by the cutoff.
//! Expected indices must increase strictly ([`Error::InvalidExpectedFrames`])
//! and observation times must not decrease ([`Error::InvalidObservationTimes`]).
//! Times and cutoff share a caller epoch, separate from nominal source times.
//! First emitted matches count once, repeats count separately and new matches
//! below the highest matched index also count as reordered. Wrong sources,
//! unknown or skipped indices and unreadable cards never count as matches.
//! Observations after the inclusive cutoff count only as `after_cutoff`.
//! Unobserved emissions are not proven packet loss, nor repeats duplicate RTP.
//!
//! Each completed call advances the source index, including skips. Indices start
//! at zero and timestamps follow 60 Hz, whatever speed you call `generate`.
//! Those timestamps are metadata, not timing embedded in the H.264 bytes.
//! [`Generator::next_metadata`] previews the next step without advancing or
//! changing pixels. It returns [`Error::Faulted`] or [`Error::TimelineExhausted`]
//! when generation cannot continue. Late calls never skip source steps.
//! [`clock_ticks`] maps an index directly to a positive integer rate in Hz,
//! rounding down once. Zero returns [`Error::InvalidClockRate`] and a result
//! beyond `u64` returns [`Error::ClockOverflow`]. Epochs and wrapping are yours.
//!
//! [`Generator::request_refresh`] queues an encoder restart at the next step.
//! Repeated requests coalesce until a picture is emitted. Skips still advance
//! source time. The restart resets codec references and rate control, while
//! keeping the source index and pattern. The next emission must have SPS/PPS
//! before IDR slices or generation fails with [`Error::UnexpectedRefresh`]
//! or a header error. Like encoder failures, this faults the generator.
//! Requests on a faulted or exhausted generator return [`Error::Faulted`] or
//! [`Error::TimelineExhausted`] without changing it.
//!
//! Emitted bytes are yours to keep across later calls. [`Generator::raw_frame`]
//! borrows the latest compact I420 planes: Y at full size, U and V at half width
//! and height. It returns `None` before the first completed step or after an
//! encoder failure. Finish using that borrow before generating more pixels.
//!
//! [`nal_units`] borrows NALs from one caller-delimited Annex B picture, including
//! each NAL header. It strips three- or four-byte prefixes and zero padding,
//! preserves unknown NAL types and leaves escaped payload bytes alone.
//! Empty input or missing framing returns [`Error::InvalidAnnexB`], an empty NAL
//! returns [`Error::EmptyNalUnit`] and a set forbidden bit returns
//! [`Error::InvalidNalHeader`]. Iteration stops after an error.
//! [`inspect_picture`] keeps the first SPS/PPS views and reports type-5 IDR NALs.
//! It reads the SPS profile, constraint byte and level, returning
//! [`Error::TruncatedSps`] if those bytes are missing from any SPS. It does not
//! parse slices or prove decoder compatibility. An ordinary I picture is not IDR.
//!
//! # When things go wrong
//!
//! [`Config::new`] can return [`Error::InvalidDimensions`], [`Error::InvalidBitrate`]
//! or [`Error::FrameSizeOverflow`]. Construction and encoding can return
//! [`Error::Encoder`]. A failed encoder leaves the generator faulted, so later
//! calls return [`Error::Faulted`]. Create a new generator to start over.
//! [`Error::TimelineExhausted`] leaves the last frame untouched.

#![forbid(unsafe_code)]

mod compare;
mod config;
mod encoder;
mod error;
mod frame;
mod generator;
mod h264;
mod identity;
mod pattern;
mod timeline;

pub use compare::{compare_frames, Comparison, Observation};
pub use config::{Config, Pattern, FRAMES_PER_SECOND};
pub use error::Error;
pub use frame::{EncodeOutcome, FrameKind, FrameMetadata, RawFrame};
pub use generator::Generator;
pub use h264::{inspect_picture, nal_units, NalUnit, NalUnits, PictureHeaders, Sps};
pub use identity::{read_identity, FrameIdentity};
pub use timeline::clock_ticks;
