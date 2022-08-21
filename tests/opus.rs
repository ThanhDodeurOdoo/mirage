#![forbid(unsafe_code)]

use audiopus::{coder::Encoder, packet, Application, Bitrate, Channels, SampleRate};
use mirage::{AudioGenerator, AudioPattern, Error, OpusConfig, OpusGenerator};
use std::time::Duration;

#[test]
fn packets_retain_exact_bytes_and_sample_timing() -> Result<(), Box<dyn std::error::Error>> {
    let source = AudioGenerator::new(AudioPattern::Tone {
        amplitude: 4_000,
        period: 120,
    })?;
    let mut generator = OpusGenerator::new(source, OpusConfig::default())?;
    assert!(generator.raw_block().is_none());
    let mut reference = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)?;
    reference.set_bitrate(Bitrate::BitsPerSecond(24_000))?;
    assert_eq!(generator.lookahead(), reference.lookahead()?);
    let mut retained = Vec::new();
    for block in 0..3 {
        let expected = generator.next_metadata()?;
        let encoded = generator.generate()?;
        assert_eq!(encoded.metadata, expected);
        assert_eq!(encoded.metadata.first_sample_index, block * 960);
        assert_eq!(encoded.metadata.sample_count, 960);
        assert_eq!(
            encoded.metadata.timestamp,
            Duration::from_millis(block * 20)
        );
        let raw = generator.raw_block().unwrap();
        assert_eq!(raw.metadata(), encoded.metadata);
        assert_eq!(raw.samples().len(), 960);
        let mut output = [0; 1_275];
        let length = reference.encode(raw.samples(), &mut output)?;
        assert_eq!(encoded.bytes, output[..length]);
        retained.push(encoded);
    }
    drop(generator);
    for encoded in retained {
        assert_eq!(
            packet::nb_samples(&encoded.bytes, SampleRate::Hz48000)?,
            960
        );
        assert_eq!(packet::nb_channels(&encoded.bytes)?, Channels::Mono);
    }
    Ok(())
}

#[test]
fn validates_mono_bitrate_bounds() -> Result<(), Error> {
    assert_eq!(OpusConfig::default().bitrate_bps(), 24_000);
    for bitrate in [0, 499, 300_001, u32::MAX] {
        assert!(matches!(
            OpusConfig::new(bitrate),
            Err(Error::InvalidBitrate(value)) if value == bitrate
        ));
    }
    for bitrate in [500, 300_000] {
        let config = OpusConfig::new(bitrate)?;
        assert_eq!(config.bitrate_bps(), bitrate);
        let mut generator =
            OpusGenerator::new(AudioGenerator::new(AudioPattern::Silence)?, config)?;
        assert!(!generator.generate()?.bytes.is_empty());
    }
    Ok(())
}
