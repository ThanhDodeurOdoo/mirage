use crate::frame::YuvFrame;
use crate::{Config, Error};
use openh264::encoder::{EncodedBitStream, EncoderConfig};

pub(crate) struct Encoder(openh264::encoder::Encoder);

impl Encoder {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        let config = EncoderConfig::new(config.width(), config.height())
            .set_bitrate_bps(config.bitrate_bps());
        openh264::encoder::Encoder::with_config(config)
            .map(Self)
            .map_err(Error::Encoder)
    }

    pub(crate) fn encode(&mut self, frame: &YuvFrame) -> Result<EncodedBitStream<'_>, Error> {
        self.0.encode(frame).map_err(Error::Encoder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::render_checkerboard;
    use crate::Pattern;

    #[test]
    fn encodes_configured_checkerboards() -> Result<(), Error> {
        for &(width, height, bitrate) in &[(18, 18, 50_000), (320, 240, 1_000_000)] {
            let config = Config::new(width, height, bitrate, Pattern::Checkerboard)?;
            let mut frame = YuvFrame::new(config);
            render_checkerboard(&mut frame);
            let mut encoder = Encoder::new(config)?;
            assert!(!encoder.encode(&frame)?.to_vec().is_empty());
        }
        Ok(())
    }
}
