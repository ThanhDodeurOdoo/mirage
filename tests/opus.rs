#![forbid(unsafe_code)]
use audiopus::{coder::Encoder, packet, Application, Bitrate, Channels, SampleRate};
#[test]
fn encodes_known_mono_pcm() -> audiopus::Result<()> {
    let mut encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)?;
    encoder.set_bitrate(Bitrate::BitsPerSecond(24_000))?;
    let mut pcm = [0_i16; 960];
    for (index, sample) in pcm.iter_mut().enumerate() {
        *sample = if index % 120 < 60 { 4_000 } else { -4_000 };
    }
    let mut output = [0_u8; 1_275];
    let length = encoder.encode(&pcm, &mut output)?;
    let packet = &output[..length];
    assert!(length > 0 && length <= output.len());
    assert_eq!(packet::nb_samples(packet, SampleRate::Hz48000)?, 960);
    assert_eq!(packet::nb_channels(packet)?, Channels::Mono);
    println!("960 mono PCM samples at 48000 Hz, 400 Hz square wave +/-4000, {} packet bytes, lookahead {}", length, encoder.lookahead()?);
    Ok(())
}
