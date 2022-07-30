use crate::Error;
use std::time::Duration;

pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_BLOCK_SAMPLES: u32 = 960;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioPattern {
    Silence,
    /// Square wave with the positive half rounded up for odd sample periods.
    Tone {
        amplitude: i16,
        period: u32,
    },
}

impl AudioPattern {
    fn validate(self) -> Result<(), Error> {
        if let Self::Tone { amplitude, period } = self {
            if amplitude < 0 {
                return Err(Error::InvalidAudioAmplitude(amplitude));
            }
            if period < 2 {
                return Err(Error::InvalidAudioPeriod(period));
            }
        }
        Ok(())
    }

    fn sample_at(self, sample_index: u64) -> i16 {
        match self {
            Self::Silence => 0,
            Self::Tone { amplitude, period } => {
                let period = u64::from(period);
                if sample_index % period < (period + 1) / 2 {
                    amplitude
                } else {
                    -amplitude
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioMetadata {
    pub first_sample_index: u64,
    pub sample_count: u32,
    pub timestamp: Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct RawAudio<'a> {
    samples: &'a [i16],
    metadata: AudioMetadata,
}

impl<'a> RawAudio<'a> {
    pub fn samples(&self) -> &'a [i16] {
        self.samples
    }

    pub fn metadata(&self) -> AudioMetadata {
        self.metadata
    }
}

pub struct AudioGenerator {
    pattern: AudioPattern,
    samples: [i16; AUDIO_BLOCK_SAMPLES as usize],
    next_sample_index: u64,
    metadata: Option<AudioMetadata>,
}

impl AudioGenerator {
    /// # Errors
    ///
    /// [`Error::InvalidAudioAmplitude`] for negative amplitude or
    /// [`Error::InvalidAudioPeriod`] for periods below two samples.
    pub fn new(pattern: AudioPattern) -> Result<Self, Error> {
        pattern.validate()?;
        Ok(Self {
            pattern,
            samples: [0; AUDIO_BLOCK_SAMPLES as usize],
            next_sample_index: 0,
            metadata: None,
        })
    }

    /// # Errors
    ///
    /// [`Error::TimelineExhausted`] if the next sample index would exceed `u64`.
    pub fn next_metadata(&self) -> Result<AudioMetadata, Error> {
        self.next_sample_index
            .checked_add(u64::from(AUDIO_BLOCK_SAMPLES))
            .ok_or(Error::TimelineExhausted)?;
        let rate = u64::from(AUDIO_SAMPLE_RATE);
        Ok(AudioMetadata {
            first_sample_index: self.next_sample_index,
            sample_count: AUDIO_BLOCK_SAMPLES,
            timestamp: Duration::new(
                self.next_sample_index / rate,
                ((self.next_sample_index % rate) * 1_000_000_000 / rate) as u32,
            ),
        })
    }

    /// # Errors
    ///
    /// [`Error::TimelineExhausted`] preserves the last completed block.
    pub fn generate(&mut self) -> Result<AudioMetadata, Error> {
        let metadata = self.next_metadata()?;
        for (offset, sample) in self.samples.iter_mut().enumerate() {
            *sample = self
                .pattern
                .sample_at(metadata.first_sample_index + offset as u64);
        }
        self.next_sample_index += u64::from(AUDIO_BLOCK_SAMPLES);
        self.metadata = Some(metadata);
        Ok(metadata)
    }

    pub fn raw_block(&self) -> Option<RawAudio<'_>> {
        self.metadata.map(|metadata| RawAudio {
            samples: &self.samples,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_advances_the_absolute_sample_timeline() -> Result<(), Error> {
        let mut generator = AudioGenerator::new(AudioPattern::Silence)?;
        assert!(generator.raw_block().is_none());
        for block in 0..=50 {
            let expected = AudioMetadata {
                first_sample_index: block * 960,
                sample_count: 960,
                timestamp: Duration::from_millis(block * 20),
            };
            assert_eq!(generator.next_metadata()?, expected);
            assert_eq!(generator.generate()?, expected);
            let raw = generator.raw_block().unwrap();
            assert_eq!(raw.metadata(), expected);
            assert_eq!(raw.samples(), &[0; 960]);
        }
        Ok(())
    }

    #[test]
    fn odd_period_phase_crosses_blocks_independently() -> Result<(), Error> {
        let tone = AudioPattern::Tone {
            amplitude: i16::MAX,
            period: 7,
        };
        let mut generator = AudioGenerator::new(tone)?;
        let mut independent = AudioGenerator::new(tone)?;
        generator.generate()?;
        let address = generator.raw_block().unwrap().samples().as_ptr();
        for block in 0..4 {
            if block > 0 {
                generator.generate()?;
            }
            let raw = generator.raw_block().unwrap();
            assert_eq!(raw.samples().as_ptr(), address);
            for (offset, &sample) in raw.samples().iter().enumerate() {
                let phase = (block * 960 + offset) % 7;
                assert_eq!(sample, if phase < 4 { 32767 } else { -32767 });
            }
        }
        independent.generate()?;
        assert_eq!(independent.raw_block().unwrap().samples()[0], 32767);
        Ok(())
    }

    #[test]
    fn rejected_patterns_and_exhaustion_preserve_the_completed_block() -> Result<(), Error> {
        let tone = AudioPattern::Tone {
            amplitude: 17,
            period: 3,
        };
        let mut generator = AudioGenerator::new(tone)?;
        generator.generate()?;
        for pattern in [
            AudioPattern::Tone {
                amplitude: -1,
                period: 3,
            },
            AudioPattern::Tone {
                amplitude: i16::MIN,
                period: 3,
            },
            AudioPattern::Tone {
                amplitude: 17,
                period: 0,
            },
            AudioPattern::Tone {
                amplitude: 17,
                period: 1,
            },
        ] {
            assert!(AudioGenerator::new(pattern).is_err());
        }
        generator.next_sample_index = u64::MAX - 960;
        generator.generate()?;
        let samples = generator.samples;
        let metadata = generator.metadata;
        assert_eq!(generator.next_sample_index, u64::MAX);
        assert!(matches!(
            generator.next_metadata(),
            Err(Error::TimelineExhausted)
        ));
        assert!(matches!(
            generator.generate(),
            Err(Error::TimelineExhausted)
        ));
        assert_eq!(generator.samples, samples);
        assert_eq!(generator.metadata, metadata);
        assert_eq!(generator.raw_block().unwrap().metadata(), metadata.unwrap());
        Ok(())
    }
}
