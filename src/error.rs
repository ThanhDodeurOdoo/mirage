use std::fmt;

#[derive(Debug)]
pub enum Error {
    InvalidDimensions { width: u32, height: u32 },
    InvalidBitrate(u32),
    FrameSizeOverflow,
    TimelineExhausted,
    InvalidAnnexB,
    EmptyNalUnit,
    InvalidNalHeader,
    Encoder(openh264::Error),
    Faulted,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(f, "unsupported dimensions: {}x{}", width, height)
            }
            Self::InvalidBitrate(bitrate) => write!(f, "unsupported bitrate: {} bps", bitrate),
            Self::FrameSizeOverflow => f.write_str("frame size overflow"),
            Self::TimelineExhausted => f.write_str("timeline exhausted"),
            Self::InvalidAnnexB => f.write_str("missing Annex B start prefix"),
            Self::EmptyNalUnit => f.write_str("empty H.264 NAL unit"),
            Self::InvalidNalHeader => f.write_str("H.264 forbidden zero bit is set"),
            Self::Encoder(error) => write!(f, "encoder failed: {}", error),
            Self::Faulted => f.write_str("generator is faulted"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encoder(error) => Some(error),
            _ => None,
        }
    }
}
