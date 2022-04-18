#[cfg(any(feature = "h264", test))]
use crate::FrameMetadata;
use crate::{Error, FRAMES_PER_SECOND};
use std::convert::TryFrom;
#[cfg(any(feature = "h264", test))]
use std::time::Duration;

/// `floor(source_index * clock_rate / 60)` ticks, with caller-managed epoch and wrapping.
///
/// # Errors
///
/// Returns [`Error::InvalidClockRate`] for zero Hz or [`Error::ClockOverflow`] beyond `u64`.
pub fn clock_ticks(source_index: u64, clock_rate: u32) -> Result<u64, Error> {
    if clock_rate == 0 {
        return Err(Error::InvalidClockRate);
    }
    let ticks = u128::from(source_index) * u128::from(clock_rate) / u128::from(FRAMES_PER_SECOND);
    u64::try_from(ticks).map_err(|_| Error::ClockOverflow)
}

#[cfg(any(feature = "h264", test))]
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

    #[test]
    fn clock_ticks_round_once_and_reject_overflow() -> Result<(), Error> {
        assert_eq!(clock_ticks(1, 90_000)?, 1_500);
        assert_eq!(clock_ticks(600, 90_000)?, 900_000);
        assert_eq!(clock_ticks(1, 1_000)?, 16);
        assert_eq!(clock_ticks(3, 1_000)?, 50);
        assert_eq!(clock_ticks(u64::MAX, 60)?, u64::MAX);
        assert!(matches!(clock_ticks(1, 0), Err(Error::InvalidClockRate)));
        assert!(matches!(
            clock_ticks(u64::MAX, 61),
            Err(Error::ClockOverflow)
        ));
        Ok(())
    }
}
