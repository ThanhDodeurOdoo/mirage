use crate::{AudioGenerator, AudioMetadata, Error, RawAudio};
use audiopus::{coder::Encoder, Application, Bitrate, Channels, SampleRate};

const MAX_PACKET_BYTES: usize = 1_275;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpusConfig {
    bitrate_bps: u32,
}

impl OpusConfig {
    /// # Errors
    ///
    /// [`Error::InvalidBitrate`] outside the mono encoder range 500..=300000.
    pub fn new(bitrate_bps: u32) -> Result<Self, Error> {
        if !(500..=300_000).contains(&bitrate_bps) {
            return Err(Error::InvalidBitrate(bitrate_bps));
        }
        Ok(Self { bitrate_bps })
    }

    pub fn bitrate_bps(&self) -> u32 {
        self.bitrate_bps
    }
}

impl Default for OpusConfig {
    fn default() -> Self {
        Self {
            bitrate_bps: 24_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPacket {
    pub metadata: AudioMetadata,
    pub bytes: Vec<u8>,
}

pub struct OpusGenerator {
    source: AudioGenerator,
    encoder: Encoder,
    lookahead: u32,
    faulted: bool,
    generated: bool,
}

impl OpusGenerator {
    /// # Errors
    ///
    /// [`Error::OpusEncoder`] retains native construction or configuration failures.
    pub fn new(source: AudioGenerator, config: OpusConfig) -> Result<Self, Error> {
        let mut encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
            .map_err(Error::OpusEncoder)?;
        encoder
            .set_bitrate(Bitrate::BitsPerSecond(config.bitrate_bps as i32))
            .map_err(Error::OpusEncoder)?;
        encoder.disable_inband_fec().map_err(Error::OpusEncoder)?;
        let lookahead = encoder.lookahead().map_err(Error::OpusEncoder)?;
        Ok(Self {
            source,
            encoder,
            lookahead,
            faulted: false,
            generated: false,
        })
    }

    /// # Errors
    ///
    /// [`Error::OpusEncoder`] faults the generator and hides the raw block.
    /// Later calls return [`Error::Faulted`]. [`Error::TimelineExhausted`] preserves the last block.
    pub fn generate(&mut self) -> Result<AudioPacket, Error> {
        self.generate_with(Encoder::encode)
    }

    /// # Errors
    ///
    /// [`Error::Faulted`] or [`Error::TimelineExhausted`].
    pub fn next_metadata(&self) -> Result<AudioMetadata, Error> {
        if self.faulted {
            return Err(Error::Faulted);
        }
        self.source.next_metadata()
    }

    /// `None` before the first packet or after a native failure.
    pub fn raw_block(&self) -> Option<RawAudio<'_>> {
        if self.faulted || !self.generated {
            None
        } else {
            self.source.raw_block()
        }
    }

    /// Codec delay in 48 kHz samples.
    pub fn lookahead(&self) -> u32 {
        self.lookahead
    }

    fn generate_with(
        &mut self,
        encode: impl FnOnce(&Encoder, &[i16], &mut [u8]) -> audiopus::Result<usize>,
    ) -> Result<AudioPacket, Error> {
        self.next_metadata()?;
        let metadata = self.source.generate()?;
        let mut bytes = vec![0; MAX_PACKET_BYTES];
        let samples = self.source.raw_block().unwrap();
        match encode(&self.encoder, samples.samples(), &mut bytes) {
            Ok(length) => {
                bytes.truncate(length);
                self.generated = true;
                Ok(AudioPacket { metadata, bytes })
            }
            Err(error) => {
                self.faulted = true;
                Err(Error::OpusEncoder(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioPattern;
    use std::error::Error as _;

    const OPUS_GET_DTX_REQUEST: i32 = 4017;

    #[test]
    fn native_failure_hides_pcm_and_faults_the_stream() -> Result<(), Error> {
        let mut generator = OpusGenerator::new(
            AudioGenerator::new(AudioPattern::Silence)?,
            OpusConfig::default(),
        )?;
        assert!(!generator.encoder.inband_fec().unwrap());
        assert_eq!(
            generator
                .encoder
                .encoder_ctl_request(OPUS_GET_DTX_REQUEST)
                .unwrap(),
            0
        );
        assert_eq!(
            generator.encoder.bitrate().unwrap(),
            Bitrate::BitsPerSecond(24_000)
        );
        generator.generate()?;
        assert!(generator.raw_block().is_some());
        let native = audiopus::Error::Opus(audiopus::ErrorCode::InternalError);
        let error = generator
            .generate_with(|_, samples, _| {
                assert_eq!(samples.len(), 960);
                Err(native)
            })
            .unwrap_err();
        assert!(matches!(error, Error::OpusEncoder(value) if value == native));
        assert_eq!(
            error.source().unwrap().downcast_ref::<audiopus::Error>(),
            Some(&native)
        );
        assert!(generator.raw_block().is_none());
        assert!(matches!(generator.next_metadata(), Err(Error::Faulted)));
        assert!(matches!(
            generator.generate_with(|_, _, _| panic!("encoded after failure")),
            Err(Error::Faulted)
        ));
        assert!(matches!(generator.generate(), Err(Error::Faulted)));
        Ok(())
    }
}
