use mirage::{Config, Error, Pattern, FRAMES_PER_SECOND};
#[cfg(feature = "h264")]
use std::error::Error as _;

#[test]
fn default_config() {
    let config = Config::default();
    assert_eq!(config.width(), 320);
    assert_eq!(config.height(), 240);
    assert_eq!(config.bitrate_bps(), 1_000_000);
    assert_eq!(config.pattern(), Pattern::Checkerboard);
    assert_eq!(FRAMES_PER_SECOND, 60);
    assert_eq!(
        Config::new(320, 240, 1_000_000, Pattern::Checkerboard).unwrap(),
        config
    );
}

#[test]
fn accepts_even_dimensions_and_bitrate_limits() {
    for &(width, height) in &[(16, 16), (18, 18), (1920, 18), (18, 1080), (1920, 1080)] {
        for &bitrate in &[1, i32::MAX as u32] {
            for &pattern in &[Pattern::Checkerboard, Pattern::MovingRectangle] {
                let config = Config::new(width, height, bitrate, pattern).unwrap();
                assert_eq!((config.width(), config.height()), (width, height));
                assert_eq!(config.bitrate_bps(), bitrate);
                assert_eq!(config.pattern(), pattern);
            }
        }
    }
}

#[test]
fn rejects_invalid_dimensions() {
    for &(width, height) in &[
        (0, 240),
        (320, 0),
        (14, 240),
        (320, 14),
        (17, 240),
        (320, 17),
        (1922, 240),
        (320, 1082),
        (u32::MAX, 240),
        (320, u32::MAX),
    ] {
        assert!(matches!(
            Config::new(width, height, 1_000_000, Pattern::Checkerboard),
            Err(Error::InvalidDimensions { width: w, height: h }) if w == width && h == height
        ));
    }
}

#[test]
fn rejects_invalid_bitrates() {
    for &bitrate in &[0, i32::MAX as u32 + 1, u32::MAX] {
        assert!(matches!(
            Config::new(320, 240, bitrate, Pattern::Checkerboard),
            Err(Error::InvalidBitrate(value)) if value == bitrate
        ));
    }
}

#[cfg(feature = "h264")]
#[test]
fn preserves_encoder_error_source() {
    let error = Error::Encoder(openh264::Error::msg("test failure"));
    assert!(error.source().unwrap().is::<openh264::Error>());
}
