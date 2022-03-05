use mirage::{Config, EncodeOutcome, Error, FrameKind, FrameMetadata, Generator, Pattern};
use std::time::Duration;

fn metadata(outcome: EncodeOutcome) -> FrameMetadata {
    match outcome {
        EncodeOutcome::Emitted {
            metadata, bytes, ..
        } => {
            assert!(!bytes.is_empty());
            metadata
        }
        EncodeOutcome::Skipped { metadata } => metadata,
    }
}

#[test]
fn native_steps_reuse_storage_and_preserve_source_time() -> Result<(), Error> {
    let config = Config::new(16, 16, 1_000_000, Pattern::Checkerboard)?;
    let mut generator = Generator::new(config)?;
    assert!(generator.raw_frame().is_none());
    let mut addresses = None;
    for index in 0..=600 {
        let source = metadata(generator.generate()?);
        assert_eq!(source.source_index, index);
        match index {
            0 => assert_eq!(source.timestamp, Duration::from_secs(0)),
            1 => assert_eq!(source.timestamp, Duration::new(0, 16_666_666)),
            60 => assert_eq!(source.timestamp, Duration::from_secs(1)),
            600 => assert_eq!(source.timestamp, Duration::from_secs(10)),
            _ => {}
        }
        let raw = generator.raw_frame().unwrap();
        assert_eq!((raw.width(), raw.height()), (16, 16));
        assert_eq!((raw.y().len(), raw.u().len(), raw.v().len()), (256, 64, 64));
        assert_eq!((raw.y()[0], raw.y()[8], raw.y()[128]), (16, 235, 235));
        assert!(raw.u().iter().chain(raw.v()).all(|&value| value == 128));
        let current = (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr());
        match addresses {
            Some(expected) => assert_eq!(current, expected),
            None => addresses = Some(current),
        }
    }
    Ok(())
}

#[test]
fn interleaved_generators_keep_independent_sources_and_pixels() -> Result<(), Error> {
    let mut checker = Generator::new(Config::new(16, 16, 1_000_000, Pattern::Checkerboard)?)?;
    let mut moving = Generator::new(Config::new(16, 16, 1_000_000, Pattern::MovingRectangle)?)?;
    assert_eq!(metadata(moving.generate()?).source_index, 0);
    assert_eq!(moving.raw_frame().unwrap().y()[0], 235);
    assert!(checker.raw_frame().is_none());
    assert_eq!(metadata(checker.generate()?).source_index, 0);
    let checker_pixels = checker.raw_frame().unwrap().y().to_vec();
    assert_eq!(checker_pixels[0], 16);
    for index in 1..=120 {
        assert_eq!(metadata(moving.generate()?).source_index, index);
        assert_eq!(checker.raw_frame().unwrap().y(), checker_pixels.as_slice());
        if index == 60 {
            assert_eq!(metadata(checker.generate()?).source_index, 1);
        }
    }
    let raw = moving.raw_frame().unwrap();
    assert_eq!(raw.y()[0], 16);
    assert_eq!(raw.y()[13 * 16 + 15], 235);
    assert_eq!(metadata(checker.generate()?).source_index, 2);
    assert_eq!(checker.raw_frame().unwrap().y(), checker_pixels.as_slice());
    Ok(())
}

#[test]
fn emitted_output_survives_later_steps_and_generator_drop() -> Result<(), Error> {
    let config = Config::new(18, 18, 1_000_000, Pattern::MovingRectangle)?;
    let mut generator = Generator::new(config)?;
    let first = generator.generate()?;
    let expected = match &first {
        EncodeOutcome::Emitted { bytes, kind, .. } => {
            assert_eq!(*kind, FrameKind::Idr);
            assert!(!bytes.is_empty());
            bytes.clone()
        }
        EncodeOutcome::Skipped { .. } => panic!("first picture was skipped"),
    };
    for index in 1..=16 {
        assert_eq!(metadata(generator.generate()?).source_index, index);
    }
    drop(generator);
    match first {
        EncodeOutcome::Emitted {
            metadata, bytes, ..
        } => {
            assert_eq!(metadata.source_index, 0);
            assert_eq!(metadata.timestamp, Duration::from_secs(0));
            assert_eq!(bytes, expected);
        }
        EncodeOutcome::Skipped { .. } => unreachable!(),
    }
    Ok(())
}

#[test]
fn texture_changes_only_when_requested() -> Result<(), Error> {
    for &changing in &[false, true] {
        let config = Config::new(18, 18, 1_000_000, Pattern::Texture { seed: 42, changing })?;
        let mut first = Generator::new(config)?;
        let mut second = Generator::new(config)?;
        first.generate()?;
        let start = first.raw_frame().unwrap().y().to_vec();
        first.generate()?;
        second.generate()?;
        assert_eq!(second.raw_frame().unwrap().y(), start.as_slice());
        assert_eq!(first.raw_frame().unwrap().y() != start.as_slice(), changing);
        second.generate()?;
        assert_eq!(
            first.raw_frame().unwrap().y(),
            second.raw_frame().unwrap().y()
        );
    }
    Ok(())
}
