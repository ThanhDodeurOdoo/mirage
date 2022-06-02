use crate::frame::YuvFrame;
use crate::{Config, EncodeOutcome, Error, FrameKind, FrameMetadata};
use std::convert::TryFrom;

pub(crate) struct Encoder(vpx_encode::Encoder);

impl Encoder {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        let bitrate_bps = config.bitrate_bps();
        let bitrate = bitrate_bps / 1000 + u32::from(bitrate_bps % 1000 != 0);
        if bitrate == 0 || bitrate > i32::MAX as u32 / 1000 {
            return Err(Error::InvalidBitrate(bitrate_bps));
        }
        vpx_encode::Encoder::new(vpx_encode::Config {
            width: config.width(),
            height: config.height(),
            timebase: [1, crate::FRAMES_PER_SECOND as i32],
            bitrate,
            codec: vpx_encode::VideoCodecId::VP8,
        })
        .map(Self)
        .map_err(Error::VpxEncoder)
    }

    pub(crate) fn max_index(&self) -> u64 {
        self.0.max_pts() as u64
    }

    pub(crate) fn encode(
        &mut self,
        frame: &YuvFrame,
        metadata: FrameMetadata,
    ) -> Result<EncodeOutcome, Error> {
        let pts = i64::try_from(metadata.source_index).map_err(|_| Error::TimelineExhausted)?;
        if metadata.source_index > self.max_index() {
            return Err(Error::TimelineExhausted);
        }
        let mut packets = self
            .0
            .encode(pts, frame.data())
            .map_err(Error::VpxEncoder)?;
        let packet = packets.next().ok_or(Error::UnexpectedVpxOutput)?;
        if packet.pts != pts || packet.data.len() < 3 || packets.next().is_some() {
            return Err(Error::UnexpectedVpxOutput);
        }
        if packet.key != (packet.data[0] & 1 == 0) {
            return Err(Error::UnexpectedVpxOutput);
        }
        let kind = if packet.key {
            if packet.data.len() < 10 || packet.data[3..6] != [0x9d, 0x01, 0x2a] {
                return Err(Error::UnexpectedVpxOutput);
            }
            let width = u16::from_le_bytes([packet.data[6], packet.data[7]]) & 0x3fff;
            let height = u16::from_le_bytes([packet.data[8], packet.data[9]]) & 0x3fff;
            if u32::from(width) != frame.view().width()
                || u32::from(height) != frame.view().height()
            {
                return Err(Error::UnexpectedVpxOutput);
            }
            FrameKind::Vp8Key
        } else {
            FrameKind::Vp8Inter
        };
        Ok(EncodeOutcome::Emitted {
            metadata,
            bytes: packet.data,
            kind,
        })
    }
}
