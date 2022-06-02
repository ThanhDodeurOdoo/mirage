use crate::encoder::Encoder;
use crate::frame::YuvFrame;
use crate::h264::confirm_refresh;
use crate::identity::render_identity;
use crate::pattern::{render_checkerboard, render_moving_rectangle, render_texture};
use crate::{timeline, Codec, Config, EncodeOutcome, Error, FrameMetadata, Pattern, RawFrame};

enum State {
    Initial,
    Ready,
    Faulted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refresh {
    None,
    Requested,
    AwaitingEmission,
}

pub struct Generator {
    encoder: Encoder,
    frame: YuvFrame,
    config: Config,
    scenes: Vec<(u64, Pattern)>,
    next_index: u64,
    state: State,
    refresh: Refresh,
}

impl Generator {
    /// # Errors
    ///
    /// [`Error::CodecUnavailable`] for disabled features or [`Error::InvalidBitrate`] for
    /// native limits. `Error::Encoder` and `Error::VpxEncoder` retain native failures.
    pub fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            encoder: Encoder::new(config)?,
            frame: YuvFrame::new(config),
            config,
            scenes: Vec::new(),
            next_index: 0,
            state: State::Initial,
            refresh: Refresh::None,
        })
    }

    /// Skips advance source time too.
    ///
    /// # Errors
    ///
    /// Native encoder errors, [`Error::UnexpectedVpxOutput`], [`Error::UnexpectedRefresh`]
    /// or [`crate::inspect_picture`] errors fault the generator. Later calls return [`Error::Faulted`].
    /// [`Error::TimelineExhausted`] preserves the last frame.
    pub fn generate(&mut self) -> Result<EncodeOutcome, Error> {
        self.generate_with(Encoder::encode)
    }

    /// The last scene continues. Cuts keep source indices and do not request refresh.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidScenes`] for empty lists, nonzero first starts or non-increasing starts.
    /// Construction errors from [`Self::new`].
    pub fn with_scenes(config: Config, scenes: Vec<(u64, Pattern)>) -> Result<Self, Error> {
        if scenes.first().map(|scene| scene.0) != Some(0)
            || scenes.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(Error::InvalidScenes);
        }
        let mut generator = Self::new(config)?;
        generator.scenes = scenes;
        Ok(generator)
    }

    /// Restarts encoding at the next step, preserving source time. Requests coalesce through skips.
    ///
    /// # Errors
    ///
    /// [`Error::Faulted`], [`Error::TimelineExhausted`] or [`Error::UnsupportedCodec`]
    /// for VP8, without mutation.
    pub fn request_refresh(&mut self) -> Result<(), Error> {
        self.next_metadata()?;
        if self.config.codec() != Codec::H264 {
            return Err(Error::UnsupportedCodec(self.config.codec()));
        }
        if self.refresh == Refresh::None {
            self.refresh = Refresh::Requested;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// [`Error::Faulted`] or [`Error::TimelineExhausted`].
    pub fn next_metadata(&self) -> Result<FrameMetadata, Error> {
        if matches!(self.state, State::Faulted) {
            return Err(Error::Faulted);
        }
        if self.next_index > self.encoder.max_index() {
            return Err(Error::TimelineExhausted);
        }
        timeline::step(self.next_index).map(|(metadata, _)| metadata)
    }

    /// Latest compact I420 pixels. `None` before the first step or when faulted.
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
        let metadata = self.next_metadata()?;
        let next_index = self.next_index + 1;
        if self.refresh == Refresh::Requested {
            match Encoder::new(self.config) {
                Ok(encoder) => {
                    self.encoder = encoder;
                    self.refresh = Refresh::AwaitingEmission;
                }
                Err(error) => {
                    self.state = State::Faulted;
                    return Err(error);
                }
            }
        }
        let pattern = self
            .scenes
            .iter()
            .rev()
            .find(|scene| scene.0 <= self.next_index)
            .map(|scene| scene.1)
            .unwrap_or(self.config.pattern());
        match pattern {
            Pattern::Checkerboard => render_checkerboard(&mut self.frame),
            Pattern::MovingRectangle => render_moving_rectangle(&mut self.frame, self.next_index),
            Pattern::Texture { seed, changing } => {
                render_texture(
                    &mut self.frame,
                    seed,
                    if changing { self.next_index } else { 0 },
                );
            }
        }
        if let Some(source_id) = self.config.identity_source() {
            render_identity(&mut self.frame, source_id, self.next_index);
        }
        let outcome = encode(&mut self.encoder, &self.frame, metadata).and_then(|outcome| {
            if self.refresh == Refresh::AwaitingEmission {
                if let EncodeOutcome::Emitted { bytes, .. } = &outcome {
                    confirm_refresh(bytes)?;
                    self.refresh = Refresh::None;
                }
            }
            Ok(outcome)
        });
        match outcome {
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

#[cfg(all(test, feature = "h264"))]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::time::Duration;

    #[test]
    fn scenes_follow_source_steps_through_skips() -> Result<(), Error> {
        let config = Config::default();
        let mut generator = Generator::with_scenes(
            config,
            vec![
                (0, Pattern::Checkerboard),
                (
                    1,
                    Pattern::Texture {
                        seed: 7,
                        changing: true,
                    },
                ),
                (2, Pattern::MovingRectangle),
            ],
        )?;
        generator.generate()?;
        generator.generate_with(|_, _, metadata| Ok(EncodeOutcome::Skipped { metadata }))?;
        let mut expected = YuvFrame::new(config);
        render_texture(&mut expected, 7, 1);
        assert_eq!(generator.raw_frame().unwrap().y(), expected.view().y());
        for index in 2..4 {
            generator.generate()?;
            render_moving_rectangle(&mut expected, index);
            assert_eq!(generator.raw_frame().unwrap().y(), expected.view().y());
            assert_eq!(generator.refresh, Refresh::None);
        }
        Ok(())
    }

    #[test]
    fn identity_survives_scenes_skips_and_refresh() -> Result<(), Error> {
        let config = Config::default().with_identity(u32::MAX)?;
        let mut generator = Generator::with_scenes(
            config,
            vec![(0, Pattern::Checkerboard), (256, Pattern::MovingRectangle)],
        )?;
        generator.next_index = 255;
        generator.generate_with(|_, _, metadata| Ok(EncodeOutcome::Skipped { metadata }))?;
        assert_eq!(generator.raw_frame().unwrap().y()[40 * 320 + 16], 235);
        generator.request_refresh()?;
        generator.generate()?;
        let mut expected = YuvFrame::new(config);
        render_moving_rectangle(&mut expected, 256);
        render_identity(&mut expected, u32::MAX, 256);
        assert_eq!(generator.raw_frame().unwrap().y(), expected.view().y());
        assert_eq!(generator.raw_frame().unwrap().y()[40 * 320 + 16], 16);
        Ok(())
    }

    #[test]
    fn rejects_invalid_scene_boundaries() {
        for starts in [vec![], vec![1], vec![0, 0], vec![0, 3, 2]] {
            let scenes = starts
                .into_iter()
                .map(|index| (index, Pattern::Checkerboard))
                .collect();
            assert!(matches!(
                Generator::with_scenes(Config::default(), scenes),
                Err(Error::InvalidScenes)
            ));
        }
    }

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
            assert!(matches!(generator.request_refresh(), Err(Error::Faulted)));
            assert!(matches!(generator.next_metadata(), Err(Error::Faulted)));
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
            generator.request_refresh()?;
            generator.next_index = if completed { u64::MAX - 1 } else { u64::MAX };
            if completed {
                let metadata = match generator.generate()? {
                    EncodeOutcome::Emitted { metadata, .. }
                    | EncodeOutcome::Skipped { metadata } => metadata,
                };
                assert_eq!(metadata.source_index, u64::MAX - 1);
            }
            let pixels = generator.frame.view().y().to_vec();
            let refresh = generator.refresh;
            for _ in 0..2 {
                assert!(matches!(
                    generator.next_metadata(),
                    Err(Error::TimelineExhausted)
                ));
                assert!(matches!(
                    generator.request_refresh(),
                    Err(Error::TimelineExhausted)
                ));
                let result = generator.generate_with(|_, _, _| panic!("encoded past exhaustion"));
                assert!(matches!(result, Err(Error::TimelineExhausted)));
                assert_eq!(generator.next_index, u64::MAX);
                assert_eq!(generator.frame.view().y(), pixels.as_slice());
                assert_eq!(generator.raw_frame().is_some(), completed);
                assert!(!matches!(generator.state, State::Faulted));
                assert_eq!(generator.refresh, refresh);
            }
        }
        Ok(())
    }

    #[test]
    fn refresh_waits_through_skips_without_resetting_source() -> Result<(), Error> {
        let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle)?;
        let mut generator = Generator::new(config)?;
        assert_eq!(generator.next_metadata()?, timeline::step(0)?.0);
        assert!(generator.raw_frame().is_none());
        generator.generate()?;
        generator.request_refresh()?;
        generator.request_refresh()?;
        let pixels = generator.raw_frame().unwrap().y().to_vec();
        for _ in 0..2 {
            assert_eq!(generator.next_metadata()?, timeline::step(1)?.0);
            assert_eq!(generator.refresh, Refresh::Requested);
            assert_eq!(generator.raw_frame().unwrap().y(), pixels.as_slice());
        }
        generator.generate_with(|_, _, metadata| Ok(EncodeOutcome::Skipped { metadata }))?;
        assert_eq!(generator.refresh, Refresh::AwaitingEmission);
        generator.request_refresh()?;
        assert_eq!(generator.next_metadata()?, timeline::step(2)?.0);
        assert_eq!(generator.refresh, Refresh::AwaitingEmission);
        match generator.generate()? {
            EncodeOutcome::Emitted {
                metadata, bytes, ..
            } => {
                assert_eq!(metadata, timeline::step(2)?.0);
                confirm_refresh(&bytes)?;
            }
            EncodeOutcome::Skipped { .. } => panic!("first native picture was skipped"),
        }
        assert_eq!(generator.refresh, Refresh::None);
        let mut expected = YuvFrame::new(config);
        render_moving_rectangle(&mut expected, 2);
        assert_eq!(generator.raw_frame().unwrap().y(), expected.view().y());
        Ok(())
    }

    #[test]
    fn unexpected_refresh_output_faults_the_generator() -> Result<(), Error> {
        let late_pps = [
            0, 0, 1, 103, 66, 192, 31, 0, 0, 1, 101, 128, 0, 0, 1, 104, 128,
        ];
        assert!(matches!(
            confirm_refresh(&late_pps),
            Err(Error::UnexpectedRefresh)
        ));
        let mut generator = Generator::new(Config::default())?;
        generator.generate()?;
        generator.request_refresh()?;
        let error = generator
            .generate_with(|_, _, metadata| {
                Ok(EncodeOutcome::Emitted {
                    metadata,
                    bytes: vec![0, 0, 1, 65, 128],
                    kind: crate::FrameKind::I,
                })
            })
            .unwrap_err();
        assert!(matches!(error, Error::UnexpectedRefresh));
        assert!(generator.raw_frame().is_none());
        assert!(matches!(generator.request_refresh(), Err(Error::Faulted)));
        assert!(matches!(generator.next_metadata(), Err(Error::Faulted)));
        assert!(matches!(generator.generate(), Err(Error::Faulted)));
        Ok(())
    }
}
