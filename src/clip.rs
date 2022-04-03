use crate::{inspect_picture, Config, EncodeOutcome, Error, FrameMetadata};

#[derive(Debug)]
pub struct ClipFrame {
    pub outcome: EncodeOutcome,
    pub has_sps: bool,
    pub has_pps: bool,
    pub has_idr: bool,
}

pub struct Clip {
    config: Config,
    max_steps: usize,
    max_bytes: usize,
    encoded_bytes: usize,
    frames: Vec<ClipFrame>,
}

impl Clip {
    pub fn new(config: Config, max_steps: usize, max_bytes: usize) -> Self {
        Self {
            config,
            max_steps,
            max_bytes,
            encoded_bytes: 0,
            frames: Vec::new(),
        }
    }

    /// Caller supplies outcomes from this clip's configured source.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidClipOrder`] for non-increasing indices or times,
    /// [`Error::ClipStepLimit`], [`Error::ClipByteLimit`] or [`inspect_picture`] errors.
    /// Rejection leaves the clip unchanged.
    pub fn push(&mut self, outcome: EncodeOutcome) -> Result<(), Error> {
        let current = metadata(&outcome);
        if let Some(last) = self.frames.last() {
            let previous = metadata(&last.outcome);
            if current.source_index <= previous.source_index
                || current.timestamp <= previous.timestamp
            {
                return Err(Error::InvalidClipOrder);
            }
        }
        if self.frames.len() >= self.max_steps {
            return Err(Error::ClipStepLimit);
        }
        let (byte_count, has_sps, has_pps, has_idr) = match &outcome {
            EncodeOutcome::Emitted { bytes, .. } => {
                if bytes.len() > self.max_bytes - self.encoded_bytes {
                    return Err(Error::ClipByteLimit);
                }
                let headers = inspect_picture(bytes)?;
                (
                    bytes.len(),
                    headers.sps.is_some(),
                    headers.pps.is_some(),
                    headers.has_idr,
                )
            }
            EncodeOutcome::Skipped { .. } => (0, false, false, false),
        };
        self.encoded_bytes += byte_count;
        self.frames.push(ClipFrame {
            outcome,
            has_sps,
            has_pps,
            has_idr,
        });
        Ok(())
    }

    pub fn config(&self) -> Config {
        self.config
    }

    /// Header flags are evidence, not a decodability guarantee.
    pub fn frames(&self) -> &[ClipFrame] {
        &self.frames
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
}

fn metadata(outcome: &EncodeOutcome) -> FrameMetadata {
    match outcome {
        EncodeOutcome::Emitted { metadata, .. } | EncodeOutcome::Skipped { metadata } => *metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameKind;
    use std::time::Duration;

    fn emitted(index: u64, bytes: Vec<u8>) -> EncodeOutcome {
        EncodeOutcome::Emitted {
            metadata: FrameMetadata {
                source_index: index,
                timestamp: Duration::from_millis(index),
            },
            bytes,
            kind: FrameKind::Idr,
        }
    }

    #[test]
    fn replay_borrows_bytes_and_preserves_gaps() -> Result<(), Error> {
        let bytes = vec![
            0, 0, 1, 103, 66, 192, 31, 0, 0, 1, 104, 128, 0, 0, 1, 101, 128,
        ];
        let address = bytes.as_ptr();
        let length = bytes.len();
        let config = Config::default().with_identity(7)?;
        let mut clip = Clip::new(config, 3, length);
        clip.push(emitted(45, bytes))?;
        clip.push(EncodeOutcome::Skipped {
            metadata: FrameMetadata {
                source_index: 46,
                timestamp: Duration::from_millis(46),
            },
        })?;
        clip.push(EncodeOutcome::Skipped {
            metadata: FrameMetadata {
                source_index: 48,
                timestamp: Duration::from_millis(48),
            },
        })?;
        assert_eq!(clip.config(), config);
        assert_eq!(clip.encoded_bytes(), length);
        let mut first = clip.frames().iter();
        let mut second = clip.frames().iter();
        let frame = first.next().unwrap();
        assert!(frame.has_sps && frame.has_pps && frame.has_idr);
        assert!(!clip.frames()[1].has_idr);
        match &frame.outcome {
            EncodeOutcome::Emitted { bytes, .. } => assert_eq!(bytes.as_ptr(), address),
            _ => panic!("expected emission"),
        }
        assert_eq!(metadata(&first.next().unwrap().outcome).source_index, 46);
        assert_eq!(metadata(&second.next().unwrap().outcome).source_index, 45);
        assert_eq!(
            metadata(&first.next().unwrap().outcome).timestamp,
            Duration::from_millis(48)
        );
        Ok(())
    }

    #[test]
    fn rejection_does_not_consume_clip_capacity() -> Result<(), Error> {
        let bytes = vec![0, 0, 1, 65, 128];
        let mut clip = Clip::new(Config::default(), 2, bytes.len());
        assert!(matches!(
            clip.push(emitted(0, vec![0])),
            Err(Error::InvalidAnnexB)
        ));
        clip.push(emitted(1, bytes.clone()))?;
        assert!(matches!(
            clip.push(emitted(1, bytes.clone())),
            Err(Error::InvalidClipOrder)
        ));
        assert!(matches!(
            clip.push(EncodeOutcome::Skipped {
                metadata: FrameMetadata {
                    source_index: 2,
                    timestamp: Duration::from_millis(1)
                }
            }),
            Err(Error::InvalidClipOrder)
        ));
        assert!(matches!(
            clip.push(emitted(2, bytes)),
            Err(Error::ClipByteLimit)
        ));
        let skipped = || EncodeOutcome::Skipped {
            metadata: FrameMetadata {
                source_index: 2,
                timestamp: Duration::from_millis(2),
            },
        };
        clip.push(skipped())?;
        assert!(matches!(
            clip.push(emitted(3, vec![0, 0, 1, 65, 128])),
            Err(Error::ClipStepLimit)
        ));
        assert_eq!(clip.frames().len(), 2);
        assert_eq!(clip.encoded_bytes(), 5);
        assert!(matches!(
            Clip::new(Config::default(), 0, 0).push(skipped()),
            Err(Error::ClipStepLimit)
        ));
        Ok(())
    }
}
