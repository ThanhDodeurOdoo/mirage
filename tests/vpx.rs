use mirage::{
    Clip, Codec, Config as VideoConfig, EncodeOutcome, Error, FrameKind, Generator, Pattern,
};
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

#[test]
fn vp8_keeps_source_steps_and_rejects_h264_inspection() -> Result<(), Error> {
    #[cfg(not(feature = "h264"))]
    assert!(matches!(
        Generator::new(VideoConfig::default()),
        Err(Error::CodecUnavailable(Codec::H264))
    ));
    let config = VideoConfig::default()
        .with_codec(Codec::Vp8)
        .with_identity(7)?;
    let mut generator = Generator::with_scenes(
        config,
        vec![(0, Pattern::Checkerboard), (3, Pattern::MovingRectangle)],
    )?;
    let first = generator.generate()?;
    let saved = match &first {
        EncodeOutcome::Emitted {
            metadata,
            kind,
            bytes,
        } => {
            assert_eq!(metadata.source_index, 0);
            assert_eq!(*kind, FrameKind::Vp8Key);
            bytes.clone()
        }
        _ => panic!("VP8 emitted no picture"),
    };
    assert!(matches!(
        first.h264_headers(),
        Err(Error::UnsupportedCodec(Codec::Vp8))
    ));
    let address = generator.raw_frame().unwrap().y().as_ptr();
    for index in 1..=6 {
        match generator.generate()? {
            EncodeOutcome::Emitted { metadata, kind, .. } => {
                assert_eq!(metadata.source_index, index);
                assert_eq!(kind.codec(), Codec::Vp8);
                assert_eq!(
                    metadata.timestamp.as_nanos(),
                    u128::from(index) * 1_000_000_000 / 60
                );
            }
            _ => panic!("VP8 unexpectedly skipped a source step"),
        }
        let raw = generator.raw_frame().unwrap();
        assert_eq!(raw.y().as_ptr(), address);
        assert_eq!(
            mirage::read_identity(raw.y(), 320, 240, 320)?
                .unwrap()
                .source_index,
            index
        );
    }
    drop(generator);
    match &first {
        EncodeOutcome::Emitted { bytes, .. } => assert_eq!(bytes, &saved),
        _ => unreachable!(),
    }
    let mut wrong_codec = Clip::new(VideoConfig::default(), 1, saved.len());
    assert!(matches!(
        wrong_codec.push(first),
        Err(Error::UnsupportedCodec(Codec::Vp8))
    ));
    assert!(wrong_codec.frames().is_empty());
    let mut clip = Clip::new(config, 3, saved.len());
    let address = saved.as_ptr();
    clip.push(EncodeOutcome::Emitted {
        metadata: mirage::FrameMetadata {
            source_index: 0,
            timestamp: std::time::Duration::from_secs(0),
        },
        bytes: saved,
        kind: FrameKind::Vp8Key,
    })?;
    assert!(clip.frames()[0].is_refresh());
    assert!(!clip.frames()[0].has_sps && !clip.frames()[0].has_pps && !clip.frames()[0].has_idr);
    match &clip.frames()[0].outcome {
        EncodeOutcome::Emitted { bytes, .. } => assert_eq!(bytes.as_ptr(), address),
        _ => unreachable!(),
    }
    let skip = EncodeOutcome::Skipped {
        metadata: mirage::FrameMetadata {
            source_index: 2,
            timestamp: std::time::Duration::from_millis(33),
        },
    };
    clip.push(skip)?;
    assert!(!clip.frames()[1].is_refresh());
    assert_eq!(clip.frames().len(), 2);
    assert_eq!(
        clip.encoded_bytes(),
        match &clip.frames()[0].outcome {
            EncodeOutcome::Emitted { bytes, .. } => bytes.len(),
            _ => unreachable!(),
        }
    );
    let bitrate =
        VideoConfig::new(16, 16, i32::MAX as u32, Pattern::Checkerboard)?.with_codec(Codec::Vp8);
    assert!(matches!(
        Generator::new(bitrate),
        Err(Error::InvalidBitrate(_))
    ));
    Ok(())
}
