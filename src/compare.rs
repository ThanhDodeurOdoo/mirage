use crate::{EncodeOutcome, Error, FrameIdentity};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub time: Duration,
    pub identity: Option<FrameIdentity>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub matched: usize,
    pub repeats: usize,
    pub reordered: usize,
    pub wrong_source: usize,
    pub unknown: usize,
    pub unreadable: usize,
    pub unobserved: usize,
    pub after_cutoff: usize,
}

pub fn compare_frames(
    source_id: u32,
    expected: &[EncodeOutcome],
    observations: &[Observation],
    cutoff: Duration,
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
            compare_frames(7, &expected, &observations, Duration::from_millis(8))?,
            Comparison {
                matched: 3,
                repeats: 1,
                reordered: 1,
                wrong_source: 1,
                unknown: 2,
                unreadable: 1,
                unobserved: 1,
                after_cutoff: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn rejects_ambiguous_record_order() {
        let cutoff = Duration::from_secs(1);
        assert!(matches!(
            compare_frames(7, &[outcome(2, true), outcome(2, false)], &[], cutoff),
            Err(Error::InvalidExpectedFrames)
        ));
        assert!(matches!(
            compare_frames(7, &[], &[observed(None, 2), observed(None, 1)], cutoff),
            Err(Error::InvalidObservationTimes)
        ));
    }
}
