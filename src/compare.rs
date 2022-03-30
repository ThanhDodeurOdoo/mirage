use crate::{EncodeOutcome, Error, FrameIdentity};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub time: Duration,
    pub identity: Option<FrameIdentity>,
}

#[derive(Clone, Copy, Debug)]
pub struct RefreshRequest {
    pub time: Duration,
    /// Caller-confirmed refresh boundary.
    pub confirmed_index: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub matched: usize,
    pub repeats: usize,
    /// First matches below the highest previously matched index.
    pub reordered: usize,
    pub wrong_source: usize,
    /// Indices absent from expected emissions, including skipped steps.
    pub unknown: usize,
    pub unreadable: usize,
    pub unobserved: usize,
    /// Late observations count only here.
    pub after_cutoff: usize,
    /// Time from request to the first subsequent match at or beyond the confirmed index,
    /// including repeats. `None` means no request or no recovery by cutoff.
    pub recovery: Option<Duration>,
}

/// Use one monotonic epoch and only outcomes produced by the inclusive cutoff.
///
/// # Errors
///
/// [`Error::InvalidExpectedFrames`] for non-increasing source indices,
/// [`Error::InvalidObservationTimes`] for decreasing observation times or
/// [`Error::InvalidRefreshRequest`] for a late request or a non-emitted refresh index.
pub fn compare_frames(
    source_id: u32,
    expected: &[EncodeOutcome],
    observations: &[Observation],
    cutoff: Duration,
    refresh: Option<RefreshRequest>,
) -> Result<Comparison, Error> {
    if expected
        .windows(2)
        .any(|pair| source_index(&pair[0]) >= source_index(&pair[1]))
    {
        return Err(Error::InvalidExpectedFrames);
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].time > pair[1].time)
    {
        return Err(Error::InvalidObservationTimes);
    }
    if let Some(refresh) = refresh {
        if refresh.time > cutoff || !expected.iter().any(|outcome| {
            matches!(outcome, EncodeOutcome::Emitted { metadata, .. } if metadata.source_index == refresh.confirmed_index)
        }) {
            return Err(Error::InvalidRefreshRequest);
        }
    }
    let mut report = Comparison {
        unobserved: expected
            .iter()
            .filter(|outcome| matches!(outcome, EncodeOutcome::Emitted { .. }))
            .count(),
        ..Comparison::default()
    };
    let mut seen = vec![false; expected.len()];
    let mut highest = None;
    for observation in observations {
        if observation.time > cutoff {
            report.after_cutoff += 1;
            continue;
        }
        let identity = match observation.identity {
            Some(identity) => identity,
            None => {
                report.unreadable += 1;
                continue;
            }
        };
        if identity.source_id != source_id {
            report.wrong_source += 1;
            continue;
        }
        let position = match expected.binary_search_by_key(&identity.source_index, source_index) {
            Ok(position) if matches!(expected[position], EncodeOutcome::Emitted { .. }) => position,
            _ => {
                report.unknown += 1;
                continue;
            }
        };
        if let Some(refresh) = refresh {
            if report.recovery.is_none()
                && identity.source_index >= refresh.confirmed_index
                && observation.time >= refresh.time
            {
                report.recovery = Some(observation.time - refresh.time);
            }
        }
        if seen[position] {
            report.repeats += 1;
        } else {
            report.matched += 1;
            report.unobserved -= 1;
            if highest.map_or(false, |highest| position < highest) {
                report.reordered += 1;
            }
            highest = Some(highest.map_or(position, |highest: usize| highest.max(position)));
            seen[position] = true;
        }
    }
    Ok(report)
}

fn source_index(outcome: &EncodeOutcome) -> u64 {
    match outcome {
        EncodeOutcome::Emitted { metadata, .. } | EncodeOutcome::Skipped { metadata } => {
            metadata.source_index
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameKind, FrameMetadata};

    fn outcome(source_index: u64, emitted: bool) -> EncodeOutcome {
        let metadata = FrameMetadata {
            source_index,
            timestamp: Duration::from_secs(0),
        };
        if emitted {
            EncodeOutcome::Emitted {
                metadata,
                bytes: vec![1],
                kind: FrameKind::P,
            }
        } else {
            EncodeOutcome::Skipped { metadata }
        }
    }

    fn observed(index: Option<u64>, millis: u64) -> Observation {
        Observation {
            time: Duration::from_millis(millis),
            identity: index.map(|source_index| FrameIdentity {
                source_id: 7,
                source_index,
            }),
        }
    }

    #[test]
    fn compares_emissions_without_treating_skips_as_missing() -> Result<(), Error> {
        let expected: Vec<_> = (0..5).map(|index| outcome(index, index != 1)).collect();
        let mut wrong_source = observed(Some(3), 4);
        wrong_source.identity.as_mut().unwrap().source_id = 8;
        let observations = [
            observed(Some(2), 1),
            observed(Some(0), 2),
            observed(Some(2), 3),
            wrong_source,
            observed(Some(1), 5),
            observed(Some(99), 6),
            observed(None, 7),
            observed(Some(3), 8),
            observed(Some(4), 9),
        ];
        assert_eq!(
            compare_frames(7, &expected, &observations, Duration::from_millis(8), None)?,
            Comparison {
                matched: 3,
                repeats: 1,
                reordered: 1,
                wrong_source: 1,
                unknown: 2,
                unreadable: 1,
                unobserved: 1,
                after_cutoff: 1,
                recovery: None,
            }
        );
        Ok(())
    }

    #[test]
    fn rejects_ambiguous_record_order() {
        let cutoff = Duration::from_secs(1);
        assert!(matches!(
            compare_frames(7, &[outcome(2, true), outcome(2, false)], &[], cutoff, None),
            Err(Error::InvalidExpectedFrames)
        ));
        assert!(matches!(
            compare_frames(
                7,
                &[],
                &[observed(None, 2), observed(None, 1)],
                cutoff,
                None
            ),
            Err(Error::InvalidObservationTimes)
        ));
        for refresh in [
            RefreshRequest {
                time: cutoff,
                confirmed_index: 1,
            },
            RefreshRequest {
                time: cutoff + Duration::from_secs(1),
                confirmed_index: 0,
            },
        ] {
            assert!(matches!(
                compare_frames(
                    7,
                    &[outcome(0, true), outcome(1, false)],
                    &[],
                    cutoff,
                    Some(refresh)
                ),
                Err(Error::InvalidRefreshRequest)
            ));
        }
    }

    #[test]
    fn recovery_uses_only_matching_media_after_the_request() -> Result<(), Error> {
        let expected: Vec<_> = (4..8).map(|index| outcome(index, index != 5)).collect();
        let refresh = Some(RefreshRequest {
            time: Duration::from_millis(10),
            confirmed_index: 6,
        });
        let observations = [
            observed(Some(7), 9),
            observed(Some(4), 10),
            observed(Some(5), 11),
            observed(Some(7), 12),
            observed(Some(6), 15),
        ];
        let unrecovered = compare_frames(
            7,
            &expected,
            &observations,
            Duration::from_millis(11),
            refresh,
        )?;
        assert_eq!(unrecovered.recovery, None);
        let recovered = compare_frames(
            7,
            &expected,
            &observations,
            Duration::from_millis(15),
            refresh,
        )?;
        assert_eq!(recovered.recovery, Some(Duration::from_millis(2)));
        assert_eq!(recovered.repeats, 1);
        assert_eq!(
            compare_frames(
                7,
                &expected,
                &[observed(Some(6), 10)],
                Duration::from_millis(10),
                refresh
            )?
            .recovery,
            Some(Duration::from_secs(0))
        );
        Ok(())
    }
}
