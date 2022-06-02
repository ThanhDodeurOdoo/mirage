use crate::frame::YuvFrame;
use crate::{Config, EncodeOutcome, Error, FrameKind, FrameMetadata};
use openh264::encoder::{EncoderConfig, FrameType};
use openh264::formats::YUVSource;

// Compact I420: Y, U, V with strides width, width/2, width/2.
impl YUVSource for YuvFrame {
    fn width(&self) -> i32 {
        self.view().width() as i32
    }

    fn height(&self) -> i32 {
        self.view().height() as i32
    }

    fn y(&self) -> &[u8] {
        self.view().y()
    }

    fn u(&self) -> &[u8] {
        self.view().u()
    }

    fn v(&self) -> &[u8] {
        self.view().v()
    }

    fn y_stride(&self) -> i32 {
        self.view().width() as i32
    }

    fn u_stride(&self) -> i32 {
        (self.view().width() / 2) as i32
    }

    fn v_stride(&self) -> i32 {
        self.u_stride()
    }
}

pub(crate) struct Encoder(openh264::encoder::Encoder);

impl Encoder {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        let config = EncoderConfig::new(config.width(), config.height())
            .set_bitrate_bps(config.bitrate_bps());
        openh264::encoder::Encoder::with_config(config)
            .map(Self)
            .map_err(Error::Encoder)
    }

    pub(crate) fn encode(
        &mut self,
        frame: &YuvFrame,
        metadata: FrameMetadata,
    ) -> Result<EncodeOutcome, Error> {
        let stream = self.0.encode(frame).map_err(Error::Encoder)?;
        package(metadata, stream.frame_type(), || stream.to_vec())
    }
}

fn package(
    metadata: FrameMetadata,
    frame_type: FrameType,
    copy_bytes: impl FnOnce() -> Vec<u8>,
) -> Result<EncodeOutcome, Error> {
    let kind = match frame_type {
        FrameType::IDR => FrameKind::Idr,
        FrameType::I => FrameKind::I,
        FrameType::P => FrameKind::P,
        FrameType::IPMixed => FrameKind::IpMixed,
        FrameType::Skip => return Ok(EncodeOutcome::Skipped { metadata }),
        FrameType::Invalid => {
            return Err(Error::Encoder(openh264::Error::msg(
                "encoder returned an invalid frame type",
            )))
        }
    };
    let bytes = copy_bytes();
    if bytes.is_empty() {
        return Err(Error::Encoder(openh264::Error::msg(
            "encoder returned an empty picture",
        )));
    }
    Ok(EncodeOutcome::Emitted {
        metadata,
        bytes,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{render_checkerboard, render_moving_rectangle};
    use crate::Pattern;
    use std::error::Error as _;
    use std::time::Duration;

    fn source_metadata() -> FrameMetadata {
        FrameMetadata {
            source_index: 61,
            timestamp: Duration::new(1, 16_666_666),
        }
    }

    #[test]
    fn encodes_configured_checkerboards() -> Result<(), Error> {
        for &(width, height, bitrate) in &[(18, 18, 50_000), (320, 240, 1_000_000)] {
            let config = Config::new(width, height, bitrate, Pattern::Checkerboard)?;
            let mut frame = YuvFrame::new(config);
            render_checkerboard(&mut frame);
            let mut encoder = Encoder::new(config)?;
            match encoder.encode(&frame, source_metadata())? {
                EncodeOutcome::Emitted {
                    metadata,
                    bytes,
                    kind,
                } => {
                    assert_eq!(metadata, source_metadata());
                    assert_eq!(kind, FrameKind::Idr);
                    let nal_types = crate::nal_units(&bytes)
                        .map(|nal| nal.map(|nal| nal.nal_type()))
                        .collect::<Result<Vec<_>, _>>()?;
                    assert_eq!(&nal_types[..2], &[7, 8]);
                    assert!(nal_types[2..].contains(&5));
                    let headers = crate::inspect_picture(&bytes)?;
                    assert!(headers.sps.is_some());
                    assert!(headers.pps.is_some());
                    assert!(headers.has_idr);
                }
                EncodeOutcome::Skipped { .. } => panic!("first picture was skipped"),
            }
        }
        Ok(())
    }

    #[test]
    fn retained_bytes_survive_later_encoding() -> Result<(), Error> {
        let config = Config::default();
        let mut encoder = Encoder::new(config)?;
        let mut frame = YuvFrame::new(config);
        render_checkerboard(&mut frame);
        let first = match encoder.encode(&frame, source_metadata())? {
            EncodeOutcome::Emitted { bytes, .. } => bytes,
            EncodeOutcome::Skipped { .. } => panic!("first picture was skipped"),
        };
        let expected = first.clone();
        for index in 1..=16 {
            render_moving_rectangle(&mut frame, index);
            encoder.encode(&frame, crate::timeline::step(index)?.0)?;
        }
        drop(encoder);
        assert_eq!(first, expected);
        Ok(())
    }

    #[test]
    fn preserves_emitted_kinds_and_bytes() {
        for &(native, expected) in &[
            (FrameType::IDR, FrameKind::Idr),
            (FrameType::I, FrameKind::I),
            (FrameType::P, FrameKind::P),
            (FrameType::IPMixed, FrameKind::IpMixed),
        ] {
            match package(source_metadata(), native, || vec![0, 0, 0, 1, 65]).unwrap() {
                EncodeOutcome::Emitted {
                    metadata,
                    bytes,
                    kind,
                } => {
                    assert_eq!(metadata, source_metadata());
                    assert_eq!(bytes, [0, 0, 0, 1, 65]);
                    assert_eq!(kind, expected);
                }
                EncodeOutcome::Skipped { .. } => panic!("emitted picture became a skip"),
            }
            let error = package(source_metadata(), native, Vec::new).unwrap_err();
            assert!(matches!(error, Error::Encoder(_)));
            assert!(error.source().unwrap().is::<openh264::Error>());
        }
    }

    #[test]
    fn skips_and_invalid_types_do_not_copy_bytes() {
        let skipped = package(source_metadata(), FrameType::Skip, || {
            panic!("copied a skip")
        })
        .unwrap();
        assert!(matches!(
            skipped,
            EncodeOutcome::Skipped { metadata } if metadata == source_metadata()
        ));
        let invalid = package(source_metadata(), FrameType::Invalid, || {
            panic!("copied invalid output")
        });
        assert!(matches!(invalid, Err(Error::Encoder(_))));
    }
}
