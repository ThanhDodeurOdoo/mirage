use crate::frame::YuvFrame;
use crate::{Codec, Config, EncodeOutcome, Error, FrameMetadata};

#[cfg(feature = "h264")]
mod h264;
#[cfg(feature = "vp8")]
mod vp8;

pub(crate) enum Encoder {
    #[cfg(feature = "h264")]
    H264(h264::Encoder),
    #[cfg(feature = "vp8")]
    Vp8(vp8::Encoder),
}

impl Encoder {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        match config.codec() {
            #[cfg(feature = "h264")]
            Codec::H264 => h264::Encoder::new(config).map(Self::H264),
            #[cfg(not(feature = "h264"))]
            Codec::H264 => Err(Error::CodecUnavailable(Codec::H264)),
            #[cfg(feature = "vp8")]
            Codec::Vp8 => vp8::Encoder::new(config).map(Self::Vp8),
            #[cfg(not(feature = "vp8"))]
            Codec::Vp8 => Err(Error::CodecUnavailable(Codec::Vp8)),
        }
    }

    pub(crate) fn max_index(&self) -> u64 {
        match self {
            #[cfg(feature = "h264")]
            Self::H264(_) => u64::MAX - 1,
            #[cfg(feature = "vp8")]
            Self::Vp8(encoder) => encoder.max_index(),
        }
    }

    pub(crate) fn encode(
        &mut self,
        frame: &YuvFrame,
        metadata: FrameMetadata,
    ) -> Result<EncodeOutcome, Error> {
        match self {
            #[cfg(feature = "h264")]
            Self::H264(encoder) => encoder.encode(frame, metadata),
            #[cfg(feature = "vp8")]
            Self::Vp8(encoder) => encoder.encode(frame, metadata),
        }
    }
}
