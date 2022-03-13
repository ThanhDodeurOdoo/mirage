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
    identity_source: Option<u32>,
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
            identity_source: None,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn with_identity(mut self, source_id: u32) -> Result<Self, Error> {
        if (self.width as usize) < crate::identity::MIN_WIDTH
            || (self.height as usize) < crate::identity::MIN_HEIGHT
        {
            return Err(Error::IdentityCardTooSmall);
        }
        self.identity_source = Some(source_id);
        Ok(self)
    }

    pub fn identity_source(&self) -> Option<u32> {
        self.identity_source
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
            identity_source: None,
        }
    }
}
