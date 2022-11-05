use mirage::{AudioGenerator, AudioPattern, OpusConfig, OpusGenerator};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let directory = PathBuf::from(args.next().unwrap_or_else(|| "target/audio".into()));
    let dtx = match args.next() {
        None => false,
        Some(value) if value == "continuous" => false,
        Some(value) if value == "dtx" => true,
        _ => {
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, "expected continuous or dtx").into(),
            )
        }
    };
    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: audio [new-directory] [continuous|dtx]",
        )
        .into());
    }
    let source = AudioGenerator::with_bursts(
        AudioPattern::Tone {
            amplitude: 8000,
            period: 120,
        },
        vec![48_000..72_000],
    )?;
    let mut generator = OpusGenerator::new(source, OpusConfig::default().with_dtx(dtx))?;
    fs::create_dir(&directory)?;
    let mut output = BufWriter::new(File::create(directory.join("audio.bit"))?);
    let mut index = BufWriter::new(File::create(directory.join("packets.tsv"))?);
    writeln!(
        index,
        "first_sample_index\tsample_count\ttimestamp_ns\toffset\tlength\tlookahead"
    )?;
    let (mut offset, mut payload_bytes, mut small_packets) = (0, 0, 0);
    for _ in 0..150 {
        let packet = generator.generate()?;
        // opus_demo framing: big-endian length, optional range check (zero), payload.
        output.write_all(&(packet.bytes.len() as u32).to_be_bytes())?;
        output.write_all(&0u32.to_be_bytes())?;
        output.write_all(&packet.bytes)?;
        offset += 8;
        writeln!(
            index,
            "{}\t{}\t{}\t{}\t{}\t{}",
            packet.metadata.first_sample_index,
            packet.metadata.sample_count,
            packet.metadata.timestamp.as_nanos(),
            offset,
            packet.bytes.len(),
            generator.lookahead(),
        )?;
        offset += packet.bytes.len();
        payload_bytes += packet.bytes.len();
        small_packets += usize::from(packet.bytes.len() <= 2);
    }
    output.flush()?;
    index.flush()?;
    println!(
        "48000 Hz mono, 150 packets, 144000 source samples, lookahead {}",
        generator.lookahead()
    );
    println!(
        "dtx {}, {} payload bytes, {} packets of at most two bytes",
        dtx, payload_bytes, small_packets
    );
    Ok(())
}
