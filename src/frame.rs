#[cfg(any(feature = "h264", feature = "vp8", test))]
use crate::Config;
use crate::{inspect_picture, Codec, Error, PictureHeaders};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct RawFrame<'a> {
    width: u32,
    height: u32,
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
}

impl<'a> RawFrame<'a> {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn y(&self) -> &'a [u8] {
        self.y
    }

    pub fn u(&self) -> &'a [u8] {
        self.u
    }

    pub fn v(&self) -> &'a [u8] {
        self.v
    }
}

#[cfg(any(feature = "h264", feature = "vp8", test))]
pub(crate) const BLACK: u8 = 16;
#[cfg(any(feature = "h264", feature = "vp8", test))]
pub(crate) const WHITE: u8 = 235;
#[cfg(any(feature = "h264", feature = "vp8", test))]
const NEUTRAL_CHROMA: u8 = 128;

#[cfg(any(feature = "h264", feature = "vp8", test))]
pub(crate) struct YuvFrame {
    width: u32,
    height: u32,
    y_len: usize,
    data: Vec<u8>,
}

#[cfg(any(feature = "h264", feature = "vp8", test))]
impl YuvFrame {
    pub(crate) fn new(config: Config) -> Self {
        let y_len = config.width() as usize * config.height() as usize;
        let mut frame = Self {
            width: config.width(),
            height: config.height(),
            y_len,
            data: vec![NEUTRAL_CHROMA; y_len + y_len / 2],
        };
        frame.y_mut().fill(BLACK);
        frame
    }

    pub(crate) fn view(&self) -> RawFrame<'_> {
        let u_end = self.y_len + self.y_len / 4;
        RawFrame {
            width: self.width,
            height: self.height,
            y: &self.data[..self.y_len],
            u: &self.data[self.y_len..u_end],
            v: &self.data[u_end..],
        }
    }

    pub(crate) fn y_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.y_len]
    }

    #[cfg(feature = "vp8")]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMetadata {
    pub source_index: u64,
    pub timestamp: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Idr,
    I,
    P,
    IpMixed,
    Vp8Key,
    Vp8Inter,
}

impl FrameKind {
    pub fn codec(self) -> Codec {
        match self {
            Self::Idr | Self::I | Self::P | Self::IpMixed => Codec::H264,
            Self::Vp8Key | Self::Vp8Inter => Codec::Vp8,
        }
    }
}

#[derive(Debug)]
pub enum EncodeOutcome {
    Emitted {
        metadata: FrameMetadata,
        bytes: Vec<u8>,
        kind: FrameKind,
    },
    Skipped {
        metadata: FrameMetadata,
    },
}

impl EncodeOutcome {
    /// # Errors
    ///
    /// [`Error::UnsupportedCodec`] for VP8 or errors from [`inspect_picture`].
    pub fn h264_headers(&self) -> Result<Option<PictureHeaders<'_>>, Error> {
        match self {
            Self::Emitted { bytes, kind, .. } => {
                if kind.codec() != Codec::H264 {
                    return Err(Error::UnsupportedCodec(kind.codec()));
                }
                inspect_picture(bytes).map(Some)
            }
            Self::Skipped { .. } => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pattern;
    #[cfg(feature = "h264")]
    use openh264::formats::YUVSource;

    #[test]
    fn compact_plane_layout() {
        for &(width, height, y_len, chroma_len) in &[
            (16, 16, 256, 64),
            (18, 18, 324, 81),
            (320, 240, 76_800, 19_200),
            (1920, 1080, 2_073_600, 518_400),
        ] {
            let config = Config::new(width, height, 1_000_000, Pattern::Checkerboard).unwrap();
            let frame = YuvFrame::new(config);
            let raw = frame.view();
            assert_eq!((raw.width(), raw.height()), (width, height));
            #[cfg(feature = "h264")]
            assert_eq!(
                (frame.width(), frame.height()),
                (width as i32, height as i32)
            );
            assert_eq!(
                (raw.y().len(), raw.u().len(), raw.v().len()),
                (y_len, chroma_len, chroma_len)
            );
            #[cfg(feature = "h264")]
            assert_eq!(frame.y_stride(), width as i32);
            #[cfg(feature = "h264")]
            assert_eq!(
                (frame.u_stride(), frame.v_stride()),
                ((width / 2) as i32, (width / 2) as i32)
            );
            assert_eq!(raw.u().as_ptr() as usize, raw.y().as_ptr() as usize + y_len);
            assert_eq!(
                raw.v().as_ptr() as usize,
                raw.u().as_ptr() as usize + chroma_len
            );
            assert!(raw.y().iter().all(|&value| value == 16));
            assert!(raw.u().iter().all(|&value| value == 128));
            assert!(raw.v().iter().all(|&value| value == 128));
        }
    }

    #[test]
    fn reuses_storage_when_luma_changes() {
        let mut frame = YuvFrame::new(Config::default());
        let raw = frame.view();
        let addresses = (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr());
        for &luma in &[235, 16, 89] {
            frame.y_mut().fill(luma);
            let raw = frame.view();
            assert!(raw.y().iter().all(|&value| value == luma));
            assert!(raw.u().iter().all(|&value| value == 128));
            assert!(raw.v().iter().all(|&value| value == 128));
            assert_eq!(
                (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr()),
                addresses
            );
        }
    }
}
