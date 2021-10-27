use mirage::{Config, EncodeOutcome, Generator, Pattern};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

const SOURCE_STEPS: usize = 600;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let directory = PathBuf::from(args.next().unwrap_or_else(|| "target/demo".into()));
    if args.next().is_some() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "usage: render [new-directory]").into(),
        );
    }
    let defaults = Config::default();
    let config = Config::new(
        defaults.width(),
        defaults.height(),
        defaults.bitrate_bps(),
        Pattern::MovingRectangle,
    )?;
    let mut generator = Generator::new(config)?;
    fs::create_dir(&directory)?;
    let mut video = BufWriter::new(File::create(directory.join("video.h264"))?);
    let mut timeline = BufWriter::new(File::create(directory.join("frames.tsv"))?);
    writeln!(timeline, "source_index\ttimestamp_ns\toutcome")?;
    let mut emitted = 0;
    let mut skipped = 0;
    for _ in 0..SOURCE_STEPS {
        let (metadata, outcome) = match generator.generate()? {
            EncodeOutcome::Emitted {
                metadata, bytes, ..
            } => {
                video.write_all(&bytes)?;
                emitted += 1;
                (metadata, "emitted")
            }
            EncodeOutcome::Skipped { metadata } => {
                skipped += 1;
                (metadata, "skipped")
            }
        };
        writeln!(
            timeline,
            "{}\t{}\t{}",
            metadata.source_index,
            metadata.timestamp.as_nanos(),
            outcome
        )?;
    }
    video.flush()?;
    timeline.flush()?;
    println!(
        "{} emitted, {} skipped ({} source steps)",
        emitted, skipped, SOURCE_STEPS
    );
    Ok(())
}
