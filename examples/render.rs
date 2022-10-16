#[cfg(any(feature = "h264", feature = "vp8"))]
mod enabled {
    use mirage::{Clip, ClipFrame, Codec, Config, EncodeOutcome, Generator, Pattern};
    use std::env;
    use std::error::Error;
    use std::fs::{self, File};
    use std::io::{self, BufWriter, Write};
    use std::path::{Path, PathBuf};

    const SOURCE_STEPS: usize = 180;
    const SOURCE_ID: u32 = 7;
    const REFRESH_INDEX: u64 = 90;
    const MAX_BYTES: usize = 2 * 1024 * 1024;

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut args = env::args_os().skip(1);
        let directory = PathBuf::from(args.next().unwrap_or_else(|| "target/demo".into()));
        let codec_arg = args.next();
        let codec = match codec_arg.as_deref() {
            None => Codec::H264,
            Some(value) if value == "h264" => Codec::H264,
            Some(value) if value == "vp8" => Codec::Vp8,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "codec must be h264 or vp8",
                )
                .into())
            }
        };
        if args.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: render [new-directory] [h264|vp8]",
            )
            .into());
        }
        let config = Config::default()
            .with_codec(codec)
            .with_identity(SOURCE_ID)?;
        let mut generator = Generator::with_scenes(
            config,
            vec![
                (0, Pattern::Checkerboard),
                (60, Pattern::MovingRectangle),
                (
                    120,
                    Pattern::Texture {
                        seed: 7,
                        changing: false,
                    },
                ),
            ],
        )?;
        fs::create_dir(&directory)?;
        let mut clip = Clip::new(config, SOURCE_STEPS, MAX_BYTES);
        for index in 0..SOURCE_STEPS as u64 {
            if index == REFRESH_INDEX {
                generator.request_refresh()?;
            }
            clip.push(generator.generate()?)?;
        }
        write_fixture(&directory, config, clip.frames())?;
        let (refresh_start, source_index) = clip
            .frames()
            .iter()
            .enumerate()
            .find_map(|(position, frame)| match &frame.outcome {
                EncodeOutcome::Emitted { metadata, .. }
                    if metadata.source_index >= REFRESH_INDEX && frame.is_refresh() =>
                {
                    Some((position, metadata.source_index))
                }
                _ => None,
            })
            .ok_or(mirage::Error::UnexpectedRefresh)?;
        let suffix = directory.join("refresh");
        fs::create_dir(&suffix)?;
        write_fixture(&suffix, config, &clip.frames()[refresh_start..])?;
        println!("refresh suffix starts at source index {}", source_index);
        println!(
            "source {}, {}x{}, {} encoded bytes",
            SOURCE_ID,
            config.width(),
            config.height(),
            clip.encoded_bytes()
        );
        Ok(())
    }

    fn write_fixture(
        directory: &Path,
        config: Config,
        frames: &[ClipFrame],
    ) -> Result<(), Box<dyn Error>> {
        let (filename, codec) = match config.codec() {
            Codec::H264 => ("video.h264", "h264"),
            Codec::Vp8 => ("video.ivf", "vp8"),
        };
        let mut video = BufWriter::new(File::create(directory.join(filename))?);
        if config.codec() == Codec::Vp8 {
            let count = frames
                .iter()
                .filter(|frame| matches!(frame.outcome, EncodeOutcome::Emitted { .. }))
                .count();
            video.write_all(b"DKIF")?;
            video.write_all(&0u16.to_le_bytes())?;
            video.write_all(&32u16.to_le_bytes())?;
            video.write_all(b"VP80")?;
            video.write_all(&(config.width() as u16).to_le_bytes())?;
            video.write_all(&(config.height() as u16).to_le_bytes())?;
            video.write_all(&mirage::FRAMES_PER_SECOND.to_le_bytes())?;
            video.write_all(&1u32.to_le_bytes())?;
            video.write_all(&(count as u32).to_le_bytes())?;
            video.write_all(&0u32.to_le_bytes())?;
        }
        let mut timeline = BufWriter::new(File::create(directory.join("frames.tsv"))?);
        writeln!(timeline, "source_index\ttimestamp_ns\toutcome\toffset\tlength\tkind\tsps\tpps\tidr\trefresh_requested\tcodec")?;
        let mut emitted = 0;
        let mut skipped = 0;
        let mut offset = if config.codec() == Codec::Vp8 { 32 } else { 0 };
        for frame in frames {
            let (metadata, outcome, length, kind) = match &frame.outcome {
                EncodeOutcome::Emitted {
                    metadata,
                    bytes,
                    kind,
                } => {
                    if config.codec() == Codec::Vp8 {
                        video.write_all(&(bytes.len() as u32).to_le_bytes())?;
                        video.write_all(&metadata.source_index.to_le_bytes())?;
                        offset += 12;
                    }
                    video.write_all(bytes)?;
                    emitted += 1;
                    let kind = match kind {
                        mirage::FrameKind::Idr => "Idr",
                        mirage::FrameKind::I => "I",
                        mirage::FrameKind::P => "P",
                        mirage::FrameKind::IpMixed => "IpMixed",
                        mirage::FrameKind::Vp8Key => "Vp8Key",
                        mirage::FrameKind::Vp8Inter => "Vp8Inter",
                    };
                    (metadata, "emitted", bytes.len(), kind)
                }
                EncodeOutcome::Skipped { metadata } => {
                    skipped += 1;
                    (metadata, "skipped", 0, "-")
                }
            };
            writeln!(
                timeline,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                metadata.source_index,
                metadata.timestamp.as_nanos(),
                outcome,
                offset,
                length,
                kind,
                u8::from(frame.has_sps),
                u8::from(frame.has_pps),
                u8::from(frame.has_idr),
                u8::from(metadata.source_index == REFRESH_INDEX),
                codec
            )?;
            offset += length;
        }
        video.flush()?;
        timeline.flush()?;
        println!(
            "{}: {} emitted, {} skipped ({} source steps)",
            directory.display(),
            emitted,
            skipped,
            frames.len()
        );
        Ok(())
    }
}

#[cfg(any(feature = "h264", feature = "vp8"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enabled::run()
}

#[cfg(not(any(feature = "h264", feature = "vp8")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "render requires the h264 or vp8 feature",
    )
    .into())
}
