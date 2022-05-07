//! Rust interface to libvpx encoder
//!
//! This crate provides a Rust API to use
//! [libvpx](https://en.wikipedia.org/wiki/Libvpx) for encoding images.
//!
//! It it based entirely on code from [srs](https://crates.io/crates/srs).
//! Compared to the original `srs`, this code has been simplified for use as a
//! library and updated to add support for both the VP8 codec and (optionally)
//! the VP9 codec.
//!
//! # Optional features
//!
//! Compile with the cargo feature `vp9` to enable support for the VP9 codec.
//!
//! # Example
//!
//! An example of using `vpx-encode` can be found in the [`record-screen`]()
//! program. The source code for `record-screen` is in the [vpx-encode git
//! repository]().
//!
//! # Contributing
//!
//! All contributions are appreciated.

// vpx_sys is provided by the `env-libvpx-sys` crate

#![cfg_attr(feature = "backtrace", feature(backtrace))]

use std::{
    mem::MaybeUninit,
    os::raw::{c_int, c_uint, c_ulong},
};

#[cfg(feature = "backtrace")]
use std::backtrace::Backtrace;
use std::{ptr, slice};

use thiserror::Error;

#[cfg(feature = "vp9")]
use vpx_sys::vp8e_enc_control_id::*;
use vpx_sys::vpx_codec_cx_pkt_kind::VPX_CODEC_CX_FRAME_PKT;
use vpx_sys::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum VideoCodecId {
    VP8,
    #[cfg(feature = "vp9")]
    VP9,
}

impl Default for VideoCodecId {
    #[cfg(not(feature = "vp9"))]
    fn default() -> VideoCodecId {
        VideoCodecId::VP8
    }

    #[cfg(feature = "vp9")]
    fn default() -> VideoCodecId {
        VideoCodecId::VP9
    }
}

pub struct Encoder {
    ctx: vpx_codec_ctx_t,
    width: usize,
    height: usize,
    frame_len: usize,
    max_pts: i64,
    last_pts: Option<i64>,
}

#[derive(Debug, Error)]
#[error("VPX encode error: {msg}")]
pub struct Error {
    msg: String,
    #[cfg(feature = "backtrace")]
    #[backtrace]
    backtrace: Backtrace,
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Self {
            msg,
            #[cfg(feature = "backtrace")]
            backtrace: Backtrace::capture(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

macro_rules! call_vpx {
    ($x:expr) => {{
        let result = unsafe { $x }; // original expression
        let result_int = result as i32;
        // if result != VPX_CODEC_OK {
        if result_int != 0 {
            return Err(Error::from(format!(
                "Function call failed (error code {}).",
                result_int
            )));
        }
        result
    }};
}

macro_rules! call_vpx_ptr {
    ($x:expr) => {{
        let result = unsafe { $x }; // original expression
        if result.is_null() {
            return Err(Error::from("Bad pointer.".to_string()));
        }
        result
    }};
}

impl Encoder {
    /// # Errors
    /// Returns [`Error`] for unsupported dimensions, timebase, bitrate or native errors.
    pub fn new(config: Config) -> Result<Self> {
        if config.width == 0 || config.width > 16383 || config.height == 0 || config.height > 16383
        {
            return Err(Error::from("Unsupported dimensions".to_string()));
        }
        if config.timebase[0] < 1
            || config.timebase[0] > 1_000_000_000
            || config.timebase[1] < 1
            || config.timebase[1] > c_int::MAX / 10
        {
            return Err(Error::from("Unsupported timebase".to_string()));
        }
        if config.bitrate == 0 || config.bitrate > c_int::MAX as c_uint / 1000 {
            return Err(Error::from("Unsupported bitrate".to_string()));
        }
        let pixels = (config.width as usize)
            .checked_mul(config.height as usize)
            .ok_or_else(|| Error::from("Frame size overflow".to_string()))?;
        let frame_len = pixels
            .checked_add(pixels / 2)
            .ok_or_else(|| Error::from("Frame size overflow".to_string()))?;
        // libvpx 1.10 scales signed timestamps to 10 MHz before division.
        let ticks = config.timebase[0] as i64 * 10_000_000;
        let max_pts = (i64::MAX - (ticks / 2 - 1)) / ticks - 1;
        let i = match config.codec {
            VideoCodecId::VP8 => call_vpx_ptr!(vpx_codec_vp8_cx()),
            #[cfg(feature = "vp9")]
            VideoCodecId::VP9 => call_vpx_ptr!(vpx_codec_vp9_cx()),
        };

        if config.width % 2 != 0 {
            return Err(Error::from("Width must be divisible by 2".to_string()));
        }
        if config.height % 2 != 0 {
            return Err(Error::from("Height must be divisible by 2".to_string()));
        }

        let mut c = MaybeUninit::<vpx_codec_enc_cfg_t>::uninit();
        call_vpx!(vpx_codec_enc_config_default(i, c.as_mut_ptr(), 0));
        // A successful default call initializes every field, including the bit-depth enum.
        let mut c = unsafe { c.assume_init() };

        c.g_w = config.width;
        c.g_h = config.height;
        c.g_timebase.num = config.timebase[0];
        c.g_timebase.den = config.timebase[1];
        c.rc_target_bitrate = config.bitrate;

        c.g_threads = 8;
        c.g_error_resilient = VPX_ERROR_RESILIENT_DEFAULT;

        let ctx = MaybeUninit::zeroed();
        // The 1.10 context contains pointers, integers and an error enum with a zero variant.
        let mut ctx = unsafe { ctx.assume_init() };

        match config.codec {
            VideoCodecId::VP8 => {
                c.g_pass = vpx_enc_pass::VPX_RC_ONE_PASS;
                c.g_lag_in_frames = 0;
                c.rc_end_usage = vpx_rc_mode::VPX_CBR;
                c.rc_dropframe_thresh = 0;
                c.rc_resize_allowed = 0;
                c.ss_number_layers = 1;
                c.ts_number_layers = 1;
                call_vpx!(vpx_codec_enc_init_ver(
                    &mut ctx,
                    i,
                    &c,
                    0,
                    vpx_sys::VPX_ENCODER_ABI_VERSION as i32
                ));
            }
            #[cfg(feature = "vp9")]
            VideoCodecId::VP9 => {
                call_vpx!(vpx_codec_enc_init_ver(
                    &mut ctx,
                    i,
                    &c,
                    0,
                    vpx_sys::VPX_ENCODER_ABI_VERSION as i32
                ));
                // set encoder internal speed settings
                call_vpx!(vpx_codec_control_(
                    &mut ctx,
                    VP8E_SET_CPUUSED as _,
                    6 as c_int
                ));
                // set row level multi-threading
                call_vpx!(vpx_codec_control_(
                    &mut ctx,
                    VP9E_SET_ROW_MT as _,
                    1 as c_int
                ));
            }
        };

        Ok(Self {
            ctx,
            width: config.width as usize,
            height: config.height as usize,
            frame_len,
            max_pts,
            last_pts: None,
        })
    }

    /// # Errors
    /// Returns [`Error`] for noncompact I420, unsupported or nonincreasing PTS or native errors.
    pub fn encode(&mut self, pts: i64, data: &[u8]) -> Result<Packets> {
        if data.len() != self.frame_len {
            return Err(Error::from("Invalid I420 length".to_string()));
        }
        if pts < 0 || pts > self.max_pts || self.last_pts.map_or(false, |last| pts <= last) {
            return Err(Error::from("Unsupported timestamp".to_string()));
        }
        let image = MaybeUninit::zeroed();
        // All image enums have zero variants and wrapping initializes the descriptor.
        let mut image = unsafe { image.assume_init() };

        call_vpx_ptr!(vpx_img_wrap(
            &mut image,
            vpx_img_fmt::VPX_IMG_FMT_I420,
            self.width as _,
            self.height as _,
            1,
            data.as_ptr() as _,
        ));
        if image.stride[0] != self.width as c_int
            || image.stride[1] != (self.width / 2) as c_int
            || image.stride[2] != (self.width / 2) as c_int
        {
            return Err(Error::from("Invalid I420 strides".to_string()));
        }
        call_vpx!(vpx_codec_encode(
            &mut self.ctx,
            &image,
            pts,
            1, // Duration
            0, // Flags
            vpx_sys::VPX_DL_REALTIME as c_ulong,
        ));
        self.last_pts = Some(pts);
        Ok(Packets {
            ctx: &mut self.ctx,
            iter: ptr::null(),
        })
    }

    pub fn finish(mut self) -> Result<Finish> {
        call_vpx!(vpx_codec_encode(
            &mut self.ctx,
            ptr::null(),
            -1, // PTS
            1,  // Duration
            0,  // Flags
            vpx_sys::VPX_DL_REALTIME as c_ulong,
        ));

        Ok(Finish {
            enc: self,
            iter: ptr::null(),
        })
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            let result = vpx_codec_destroy(&mut self.ctx);
            if result != vpx_sys::VPX_CODEC_OK {
                panic!("failed to destroy vpx codec");
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    /// Compressed data.
    pub data: Vec<u8>,
    /// Whether the frame is a keyframe.
    pub key: bool,
    /// Presentation timestamp (in timebase units).
    pub pts: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// The width (in pixels).
    pub width: c_uint,
    /// The height (in pixels).
    pub height: c_uint,
    /// The timebase numerator and denominator (in seconds).
    pub timebase: [c_int; 2],
    /// The target bitrate (in kilobits per second).
    pub bitrate: c_uint,
    /// The codec
    pub codec: VideoCodecId,
}

pub struct Packets<'a> {
    ctx: &'a mut vpx_codec_ctx_t,
    iter: vpx_codec_iter_t,
}

impl<'a> Iterator for Packets<'a> {
    type Item = Frame;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            unsafe {
                let pkt = vpx_codec_get_cx_data(self.ctx, &mut self.iter);
                if pkt.is_null() {
                    return None;
                } else if (*pkt).kind == VPX_CODEC_CX_FRAME_PKT {
                    let f = &(*pkt).data.frame;
                    return Some(Frame {
                        // Frame packets are nonempty and valid until the next codec call.
                        data: slice::from_raw_parts(f.buf as _, f.sz as usize).to_vec(),
                        key: (f.flags & VPX_FRAME_IS_KEY) != 0,
                        pts: f.pts,
                    });
                } else {
                    // Ignore the packet.
                }
            }
        }
    }
}

pub struct Finish {
    enc: Encoder,
    iter: vpx_codec_iter_t,
}

impl Finish {
    pub fn next(&mut self) -> Result<Option<Frame>> {
        let mut tmp = Packets {
            ctx: &mut self.enc.ctx,
            iter: self.iter,
        };

        if let Some(packet) = tmp.next() {
            self.iter = tmp.iter;
            Ok(Some(packet))
        } else {
            call_vpx!(vpx_codec_encode(
                tmp.ctx,
                ptr::null(),
                -1, // PTS
                1,  // Duration
                0,  // Flags
                vpx_sys::VPX_DL_REALTIME as c_ulong,
            ));

            tmp.iter = ptr::null();
            if let Some(packet) = tmp.next() {
                self.iter = tmp.iter;
                Ok(Some(packet))
            } else {
                Ok(None)
            }
        }
    }
}
