use mirage::{
    clock_ticks, compare_frames, read_identity, EncodeOutcome, FrameKind, FrameMetadata,
    Observation,
};
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::time::Duration;

const HEADER: &str =
    "source_index\ttimestamp_ns\toutcome\toffset\tlength\tkind\tsps\tpps\tidr\trefresh_requested";

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 5 {
        return Err(invalid(
            "usage: inspect <luma-file> <frames.tsv> <width> <height> <source-id>",
        )
        .into());
    }
    let width: usize = args[2]
        .to_str()
        .ok_or_else(|| invalid("invalid width"))?
        .parse()?;
    let height: usize = args[3]
        .to_str()
        .ok_or_else(|| invalid("invalid height"))?
        .parse()?;
    let source_id: u32 = args[4]
        .to_str()
        .ok_or_else(|| invalid("invalid source ID"))?
        .parse()?;
    let frame_size = width
        .checked_mul(height)
        .filter(|&size| size > 0)
        .ok_or_else(|| invalid("luma geometry must be positive and fit usize"))?;
    let pixels = fs::read(&args[0])?;
    if pixels.is_empty() || pixels.len() % frame_size != 0 {
        return Err(invalid("luma input must contain complete, nonempty compact frames").into());
    }
    let expected = read_index(&fs::read_to_string(&args[1])?)?;
    let mut observations = Vec::new();
    for (ordinal, luma) in pixels.chunks_exact(frame_size).enumerate() {
        observations.push(Observation {
            time: Duration::from_nanos(clock_ticks(ordinal as u64, 1_000_000_000)?),
            identity: read_identity(luma, width, height, width)?,
        });
    }
    let cutoff = observations.last().unwrap().time;
    let report = compare_frames(source_id, &expected, &observations, cutoff, None)?;
    println!("virtual 60 Hz observation times, offline decode order is not latency");
    println!(
        "first recognized source index: {:?}",
        observations.iter().find_map(|observation| {
            observation
                .identity
                .filter(|identity| identity.source_id == source_id)
                .map(|identity| identity.source_index)
        })
    );
    println!("{:?}", report);
    if report.wrong_source != 0 {
        return Err(invalid("decoded identities do not match the expected source ID").into());
    }
    Ok(())
}

fn read_index(index: &str) -> Result<Vec<EncodeOutcome>, Box<dyn Error>> {
    let mut lines = index.lines();
    if lines.next() != Some(HEADER) {
        return Err(invalid("unexpected fixture index header").into());
    }
    let mut expected = Vec::new();
    let mut previous: Option<FrameMetadata> = None;
    let mut next_offset = 0u64;
    for (line, row) in lines.enumerate() {
        let fields: Vec<_> = row.split('\t').collect();
        let malformed = || invalid(&format!("invalid fixture index row {}", line + 2));
        if fields.len() != 10 || fields[6..].iter().any(|value| !matches!(*value, "0" | "1")) {
            return Err(malformed().into());
        }
        let source_index: u64 = fields[0].parse().map_err(|_| malformed())?;
        let nanos: u128 = fields[1].parse().map_err(|_| malformed())?;
        let seconds = nanos / 1_000_000_000;
        if seconds > u128::from(u64::MAX) {
            return Err(malformed().into());
        }
        let metadata = FrameMetadata {
            source_index,
            timestamp: Duration::new(seconds as u64, (nanos % 1_000_000_000) as u32),
        };
        if let Some(previous) = previous {
            if previous.source_index >= source_index || previous.timestamp >= metadata.timestamp {
                return Err(invalid(
                    "fixture source indices and timestamps must increase strictly",
                )
                .into());
            }
        }
        let offset: u64 = fields[3].parse().map_err(|_| malformed())?;
        let length: u64 = fields[4].parse().map_err(|_| malformed())?;
        if offset != next_offset {
            return Err(invalid("fixture byte offsets must be contiguous").into());
        }
        next_offset = offset.checked_add(length).ok_or_else(malformed)?;
        let outcome = match fields[2] {
            "emitted" if length > 0 => EncodeOutcome::Emitted {
                metadata,
                bytes: Vec::new(),
                kind: match fields[5] {
                    "Idr" => FrameKind::Idr,
                    "I" => FrameKind::I,
                    "P" => FrameKind::P,
                    "IpMixed" => FrameKind::IpMixed,
                    _ => return Err(malformed().into()),
                },
            },
            "skipped" if length == 0 && fields[5] == "-" && fields[6..9] == ["0", "0", "0"] => {
                EncodeOutcome::Skipped { metadata }
            }
            _ => return Err(malformed().into()),
        };
        expected.push(outcome);
        previous = Some(metadata);
    }
    if expected.is_empty() {
        return Err(invalid("fixture index has no source steps").into());
    }
    Ok(expected)
}
