use crate::Error;

const MIN_DIMENSION: u32 = 16;
const MAX_WIDTH: u32 = 1920;
const MAX_HEIGHT: u32 = 1080;

pub const FRAMES_PER_SECOND: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Checkerboard,
    MovingRectangle,
    Texture { seed: u64, changing: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    width: u32,
    height: u32,
    bitrate_bps: u32,
    pattern: Pattern,
}

impl Config {
    pub fn new(width: u32, height: u32, bitrate_bps: u32, pattern: Pattern) -> Result<Self, Error> {
        if width < MIN_DIMENSION
            || width > MAX_WIDTH
            || height < MIN_DIMENSION
            || height > MAX_HEIGHT
            || width % 2 != 0
            || height % 2 != 0
        {
            return Err(Error::InvalidDimensions { width, height });
        }
        if bitrate_bps == 0 || bitrate_bps > i32::MAX as u32 {
            return Err(Error::InvalidBitrate(bitrate_bps));
        }
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_add(pixels / 2))
            .ok_or(Error::FrameSizeOverflow)?;
        Ok(Self {
            width,
            height,
            bitrate_bps,
            pattern,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn bitrate_bps(&self) -> u32 {
        self.bitrate_bps
    }

    pub fn pattern(&self) -> Pattern {
        self.pattern
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 320,
            height: 240,
            bitrate_bps: 1_000_000,
            pattern: Pattern::Checkerboard,
        }
    }
}
