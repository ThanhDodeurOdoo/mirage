use mirage::read_identity;
use std::env;
use std::error::Error;
use std::fs;
use std::io;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const HEADER: &str = "first_sample_index\tsample_count\ttimestamp_ns\toffset\tlength\tlookahead";

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn cue_luma(luma: &[u8]) -> u32 {
    (18..22)
        .flat_map(|y| luma[y * WIDTH + 266..y * WIDTH + 270].iter())
        .map(|&value| u32::from(value))
        .sum()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(invalid("usage: av_sync <video-luma> <audio-directory>").into());
    }
    let audio_directory = std::path::Path::new(&args[1]);
    let index = fs::read_to_string(audio_directory.join("packets.tsv"))?;
    let mut rows = index.lines();
    if rows.next() != Some(HEADER) {
        return Err(invalid("unexpected audio index header").into());
    }
    let mut samples = 0u64;
    let mut next_offset = 8u64;
    let mut lookahead = None;
    for row in rows {
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() != 6 {
            return Err(invalid("invalid audio index row").into());
        }
        let first: u64 = fields[0].parse()?;
        let count: u64 = fields[1].parse()?;
        let timestamp: u128 = fields[2].parse()?;
        let offset: u64 = fields[3].parse()?;
        let length: u64 = fields[4].parse()?;
        let delay: u64 = fields[5].parse()?;
        if first != samples
            || count != 960
            || timestamp != u128::from(first) * 1_000_000_000 / 48_000
        {
            return Err(invalid(
                "audio index must describe contiguous 960-sample blocks at 48000 Hz",
            )
            .into());
        }
        if offset != next_offset
            || length == 0
            || lookahead.map_or(false, |previous| previous != delay)
        {
            return Err(
                invalid("audio packet offsets, lengths or lookahead are inconsistent").into(),
            );
        }
        samples = samples
            .checked_add(count)
            .ok_or_else(|| invalid("audio sample count overflows"))?;
        next_offset = offset
            .checked_add(length)
            .and_then(|end| end.checked_add(8))
            .ok_or_else(|| invalid("audio packet offset overflows"))?;
        lookahead = Some(delay);
    }
    let lookahead = lookahead.ok_or_else(|| invalid("audio index is empty"))?;
    let pcm = fs::read(audio_directory.join("decoded.s16le"))?;
    if samples <= lookahead || u128::from(samples) * 2 != pcm.len() as u128 {
        return Err(invalid("PCM must contain all untrimmed indexed mono samples as s16le").into());
    }
    let onset = pcm
        .chunks_exact(48 * 2)
        .position(|window| {
            let energy: i64 = window
                .chunks_exact(2)
                .map(|bytes| {
                    let sample = i64::from(i16::from_le_bytes([bytes[0], bytes[1]]));
                    sample * sample
                })
                .sum();
            energy >= 48 * 2_000 * 2_000
        })
        .ok_or_else(|| invalid("decoded audio has no window with RMS at least 2000"))?;
    let decoded_sample = onset as u128 * 48;
    let adjusted_sample = decoded_sample as i128 - i128::from(lookahead);
    let luma = fs::read(&args[0])?;
    let frame_size = WIDTH * HEIGHT;
    if luma.is_empty() || luma.len() % frame_size != 0 {
        return Err(invalid("video must contain complete 320x240 compact luma frames").into());
    }
    if cue_luma(&luma[..frame_size]) < 180 * 16 {
        return Err(
            invalid("decoded video does not begin with the white checkerboard cue region").into(),
        );
    }
    let transition = luma
        .chunks_exact(frame_size)
        .find(|frame| cue_luma(frame) < 128 * 16)
        .ok_or_else(|| invalid("decoded video has no dark cue transition"))?;
    let identity = read_identity(transition, WIDTH, HEIGHT, WIDTH)?
        .filter(|identity| identity.source_id == 7)
        .ok_or_else(|| invalid("visual transition has no recognized identity for source 7"))?;
    let visual_sample = i128::from(identity.source_index) * 800;
    println!("declared source epoch: 0, scheduled visual and audio cue: 1.000 s");
    println!(
        "decoded visual transition: source index {}, source time {:.6} s",
        identity.source_index,
        identity.source_index as f64 / 60.0
    );
    println!(
        "decoded audio onset: sample {}, first 48-sample window with RMS >= 2000",
        decoded_sample
    );
    println!("decoder leading trim: 0 samples, codec lookahead: {} samples, adjusted audio source sample: {}, source time {:.6} s", lookahead, adjusted_sample, adjusted_sample as f64 / 48_000.0);
    println!(
        "audio minus visual cue: {:.3} ms, audio detector granularity: 1 ms",
        (adjusted_sample - visual_sample) as f64 / 48.0
    );
    Ok(())
}
