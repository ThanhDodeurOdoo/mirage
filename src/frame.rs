use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct RawFrame<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) y: &'a [u8],
    pub(crate) u: &'a [u8],
    pub(crate) v: &'a [u8],
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
