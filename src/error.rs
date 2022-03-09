use std::fmt;

#[derive(Debug)]
pub enum Error {
    InvalidDimensions { width: u32, height: u32 },
    InvalidBitrate(u32),
    InvalidScenes,
    FrameSizeOverflow,
    TimelineExhausted,
    InvalidClockRate,
    ClockOverflow,
    InvalidAnnexB,
    EmptyNalUnit,
    InvalidNalHeader,
    TruncatedSps,
    UnexpectedRefresh,
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
            Self::InvalidScenes => f.write_str("scenes must start at zero and increase strictly"),
            Self::FrameSizeOverflow => f.write_str("frame size overflow"),
            Self::TimelineExhausted => f.write_str("timeline exhausted"),
            Self::InvalidClockRate => f.write_str("clock rate must be positive"),
            Self::ClockOverflow => f.write_str("clock ticks exceed u64"),
            Self::InvalidAnnexB => f.write_str("missing Annex B start prefix"),
            Self::EmptyNalUnit => f.write_str("empty H.264 NAL unit"),
            Self::InvalidNalHeader => f.write_str("H.264 forbidden zero bit is set"),
            Self::TruncatedSps => f.write_str("missing H.264 SPS profile, constraints or level"),
            Self::UnexpectedRefresh => {
                f.write_str("expected SPS, PPS and IDR after encoder restart")
            }
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
