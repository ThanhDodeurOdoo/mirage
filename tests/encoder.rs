use openh264::encoder::{Encoder, EncoderConfig};

#[test]
fn create_encoder() -> Result<(), openh264::Error> {
    let config = EncoderConfig::new(320, 240);
    let _encoder = Encoder::with_config(config)?;
    Ok(())
}
