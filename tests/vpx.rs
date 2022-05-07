use vpx_encode::{Config, Encoder, VideoCodecId};

#[test]
fn encode_compact_i420_and_retain_packets() -> Result<(), vpx_encode::Error> {
    let config = Config {
        width: 64,
        height: 48,
        timebase: [1, 60],
        bitrate: 100,
        codec: VideoCodecId::VP8,
    };
    let mut encoder = Encoder::new(config)?;
    let mut pixels = vec![128; 64 * 48 * 3 / 2];
    pixels[..64 * 48].fill(16);
    assert!(encoder.encode(0, &pixels[..10]).is_err());
    let first = encoder.encode(0, &pixels)?.collect::<Vec<_>>();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].pts, 0);
    assert!(first[0].key);
    assert!(!first[0].data.is_empty());
    let saved = first[0].data.clone();
    pixels[..64 * 48].fill(235);
    let second = encoder.encode(1, &pixels)?.collect::<Vec<_>>();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].pts, 1);
    assert!(encoder.encode(i64::MAX, &pixels).is_err());
    drop(encoder);
    assert_eq!(first[0].data, saved);
    assert!(Encoder::new(Config { width: 0, ..config }).is_err());
    assert!(Encoder::new(Config {
        timebase: [0, 60],
        ..config
    })
    .is_err());
    Ok(())
}
