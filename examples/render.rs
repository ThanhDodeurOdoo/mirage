use mirage::{Clip, ClipFrame, Config, EncodeOutcome, Generator, Pattern};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

const SOURCE_STEPS: usize = 180;
const SOURCE_ID: u32 = 7;
const REFRESH_INDEX: u64 = 90;
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let directory = PathBuf::from(args.next().unwrap_or_else(|| "target/demo".into()));
    if args.next().is_some() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "usage: render [new-directory]").into(),
        );
    }
    let config = Config::default().with_identity(SOURCE_ID)?;
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
    write_fixture(&directory, clip.frames())?;
    println!(
        "source {}, {}x{}, {} encoded bytes",
        SOURCE_ID,
        config.width(),
        config.height(),
        clip.encoded_bytes()
    );
    Ok(())
}

fn write_fixture(directory: &Path, frames: &[ClipFrame]) -> Result<(), Box<dyn Error>> {
    let mut video = BufWriter::new(File::create(directory.join("video.h264"))?);
    let mut timeline = BufWriter::new(File::create(directory.join("frames.tsv"))?);
    writeln!(timeline, "source_index\ttimestamp_ns\toutcome\toffset\tlength\tkind\tsps\tpps\tidr\trefresh_requested")?;
    let mut emitted = 0;
    let mut skipped = 0;
    let mut offset = 0;
    for frame in frames {
        let (metadata, outcome, length, kind) = match &frame.outcome {
            EncodeOutcome::Emitted {
                metadata,
                bytes,
                kind,
            } => {
                video.write_all(bytes)?;
                emitted += 1;
                let kind = match kind {
                    mirage::FrameKind::Idr => "Idr",
                    mirage::FrameKind::I => "I",
                    mirage::FrameKind::P => "P",
                    mirage::FrameKind::IpMixed => "IpMixed",
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
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            metadata.source_index,
            metadata.timestamp.as_nanos(),
            outcome,
            offset,
            length,
            kind,
            u8::from(frame.has_sps),
            u8::from(frame.has_pps),
            u8::from(frame.has_idr),
            u8::from(metadata.source_index == REFRESH_INDEX)
        )?;
        offset += length;
    }
    video.flush()?;
    timeline.flush()?;
    println!(
        "{} emitted, {} skipped ({} source steps)",
        emitted,
        skipped,
        frames.len()
    );
    Ok(())
}
