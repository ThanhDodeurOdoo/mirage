use crate::encoder::Encoder;
use crate::frame::YuvFrame;
use crate::pattern::{render_checkerboard, render_moving_rectangle};
use crate::{timeline, Config, EncodeOutcome, Error, FrameMetadata, Pattern, RawFrame};

enum State {
    Initial,
    Ready,
    Faulted,
}

pub struct Generator {
    encoder: Encoder,
    frame: YuvFrame,
    pattern: Pattern,
    next_index: u64,
    state: State,
}

impl Generator {
    pub fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            encoder: Encoder::new(config)?,
            frame: YuvFrame::new(config),
            pattern: config.pattern(),
            next_index: 0,
            state: State::Initial,
        })
    }

    pub fn generate(&mut self) -> Result<EncodeOutcome, Error> {
        self.generate_with(Encoder::encode)
    }

    pub fn raw_frame(&self) -> Option<RawFrame<'_>> {
        match self.state {
            State::Ready => Some(self.frame.view()),
            State::Initial | State::Faulted => None,
        }
    }

    fn generate_with(
        &mut self,
        encode: impl FnOnce(&mut Encoder, &YuvFrame, FrameMetadata) -> Result<EncodeOutcome, Error>,
    ) -> Result<EncodeOutcome, Error> {
        if matches!(self.state, State::Faulted) {
            return Err(Error::Faulted);
        }
        let (metadata, next_index) = timeline::step(self.next_index)?;
        match self.pattern {
            Pattern::Checkerboard => render_checkerboard(&mut self.frame),
            Pattern::MovingRectangle => render_moving_rectangle(&mut self.frame, self.next_index),
        }
        match encode(&mut self.encoder, &self.frame, metadata) {
            Ok(outcome) => {
                self.next_index = next_index;
                self.state = State::Ready;
                Ok(outcome)
            }
            Err(error) => {
                self.state = State::Faulted;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::time::Duration;

    #[test]
    fn skipped_steps_advance_and_expose_pixels() -> Result<(), Error> {
        let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle)?;
        let mut generator = Generator::new(config)?;
        for index in 0..2 {
            let outcome = generator.generate_with(|_, frame, metadata| {
                assert_eq!(metadata.source_index, index);
                assert_eq!(frame.view().y()[0], if index == 0 { 235 } else { 16 });
                Ok(EncodeOutcome::Skipped { metadata })
            })?;
            assert!(matches!(
                outcome,
                EncodeOutcome::Skipped { metadata } if metadata.source_index == index
            ));
            assert_eq!(generator.next_index, index + 1);
            assert!(generator.raw_frame().is_some());
        }
        let metadata = match generator.generate()? {
            EncodeOutcome::Emitted { metadata, .. } | EncodeOutcome::Skipped { metadata } => {
                metadata
            }
        };
        assert_eq!(metadata.source_index, 2);
        assert_eq!(metadata.timestamp, Duration::new(0, 33_333_333));
        Ok(())
    }

    #[test]
    fn visual_counter_wraps_across_skipped_steps() -> Result<(), Error> {
        let config = Config::new(16, 16, 1_000_000, Pattern::MovingRectangle)?;
        let mut generator = Generator::new(config)?;
        generator.next_index = 254;
        for &(index, nanos, row) in &[
            (
                254,
                233_333_333,
                [
                    16, 16, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235, 235,
                ],
            ),
            (255, 250_000_000, [235; 16]),
            (256, 266_666_666, [16; 16]),
        ] {
            let outcome = if index == 255 {
                let skipped = generator
                    .generate_with(|_, _, metadata| Ok(EncodeOutcome::Skipped { metadata }))?;
                assert!(matches!(skipped, EncodeOutcome::Skipped { .. }));
                skipped
            } else {
                generator.generate()?
            };
            let metadata = match outcome {
                EncodeOutcome::Emitted { metadata, .. } | EncodeOutcome::Skipped { metadata } => {
                    metadata
                }
            };
            assert_eq!(metadata.source_index, index);
            assert_eq!(metadata.timestamp, Duration::new(4, nanos));
            assert_eq!(generator.raw_frame().unwrap().y()[14 * 16..], row.repeat(2));
        }
        Ok(())
    }

    #[test]
    fn encoder_failure_hides_pixels_and_faults_the_stream() -> Result<(), Error> {
        let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle)?;
        let mut generator = Generator::new(config)?;
        generator.generate()?;
        assert_eq!(generator.raw_frame().unwrap().y()[0], 235);
        let failure = openh264::Error::msg("injected encode failure");
        let message = failure.to_string();
        let error = generator
            .generate_with(|_, frame, _| {
                assert_eq!(frame.view().y()[0], 16);
                Err(Error::Encoder(failure))
            })
            .unwrap_err();
        assert!(matches!(error, Error::Encoder(_)));
        assert!(error.source().unwrap().is::<openh264::Error>());
        assert_eq!(error.source().unwrap().to_string(), message);
        assert!(generator.raw_frame().is_none());
        assert_eq!(generator.next_index, 1);
        let pixels = generator.frame.view().y().to_vec();
        for &index in &[1, u64::MAX] {
            generator.next_index = index;
            let result = generator.generate_with(|_, _, _| panic!("encoded after failure"));
            assert!(matches!(result, Err(Error::Faulted)));
            assert!(matches!(generator.generate(), Err(Error::Faulted)));
            assert_eq!(generator.frame.view().y(), pixels.as_slice());
            assert!(generator.raw_frame().is_none());
        }
        Ok(())
    }

    #[test]
    fn exhaustion_preserves_pixels_and_state() -> Result<(), Error> {
        for &completed in &[false, true] {
            let mut generator = Generator::new(Config::default())?;
            generator.next_index = if completed { u64::MAX - 1 } else { u64::MAX };
            if completed {
                let metadata = match generator.generate()? {
                    EncodeOutcome::Emitted { metadata, .. }
                    | EncodeOutcome::Skipped { metadata } => metadata,
                };
                assert_eq!(metadata.source_index, u64::MAX - 1);
            }
            let pixels = generator.frame.view().y().to_vec();
            for _ in 0..2 {
                let result = generator.generate_with(|_, _, _| panic!("encoded past exhaustion"));
                assert!(matches!(result, Err(Error::TimelineExhausted)));
                assert_eq!(generator.next_index, u64::MAX);
                assert_eq!(generator.frame.view().y(), pixels.as_slice());
                assert_eq!(generator.raw_frame().is_some(), completed);
                assert!(!matches!(generator.state, State::Faulted));
            }
        }
        Ok(())
    }
}
