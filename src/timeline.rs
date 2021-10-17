use crate::{Error, FrameMetadata, FRAMES_PER_SECOND};
use std::time::Duration;

pub(crate) fn step(source_index: u64) -> Result<(FrameMetadata, u64), Error> {
    let next_index = source_index
        .checked_add(1)
        .ok_or(Error::TimelineExhausted)?;
    let cadence = u64::from(FRAMES_PER_SECOND);
    let seconds = source_index / cadence;
    let nanos = (source_index % cadence) * 1_000_000_000 / cadence;
    let metadata = FrameMetadata {
        source_index,
        timestamp: Duration::new(seconds, nanos as u32),
    };
    Ok((metadata, next_index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_follow_absolute_indices() {
        for &(index, seconds, nanos) in &[
            (0, 0, 0),
            (1, 0, 16_666_666),
            (2, 0, 33_333_333),
            (59, 0, 983_333_333),
            (60, 1, 0),
            (61, 1, 16_666_666),
            (600, 10, 0),
        ] {
            let (metadata, next) = step(index).unwrap();
            assert_eq!(metadata.source_index, index);
            assert_eq!(metadata.timestamp, Duration::new(seconds, nanos));
            assert_eq!(next, index + 1);
        }
    }

    #[test]
    fn sequential_steps_do_not_accumulate_rounding() {
        let mut index = 0;
        for second in 0..=10 {
            let (metadata, _) = step(index).unwrap();
            assert_eq!(metadata.timestamp, Duration::from_secs(second));
            for _ in 0..FRAMES_PER_SECOND {
                let (metadata, next) = step(index).unwrap();
                assert!(step(next).unwrap().0.timestamp > metadata.timestamp);
                index = next;
            }
        }
    }

    #[test]
    fn exhaustion_does_not_wrap() {
        let (metadata, next) = step(u64::MAX - 1).unwrap();
        assert_eq!(metadata.source_index, u64::MAX - 1);
        assert_eq!(
            metadata.timestamp,
            Duration::new(307_445_734_561_825_860, 233_333_333)
        );
        assert_eq!(next, u64::MAX);
        assert!(matches!(step(next), Err(Error::TimelineExhausted)));
    }
}
