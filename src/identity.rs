use crate::frame::{YuvFrame, BLACK, WHITE};

const ORIGIN: usize = 16;
const CELL: usize = 8;
const COLUMNS: usize = 16;
const ROWS: usize = 8;
pub(crate) const MIN_WIDTH: usize = ORIGIN + (COLUMNS + 1) * CELL;
pub(crate) const MIN_HEIGHT: usize = ORIGIN + (ROWS + 1) * CELL;

pub(crate) fn render_identity(frame: &mut YuvFrame, source_id: u32, source_index: u64) {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&[0x55, 0xd3]);
    bytes[2..6].copy_from_slice(&source_id.to_le_bytes());
    bytes[6..14].copy_from_slice(&source_index.to_le_bytes());
    let checksum = checksum(&bytes[..14]);
    bytes[14..].copy_from_slice(&checksum.to_le_bytes());
    let width = frame.view().width() as usize;
    let luma = frame.y_mut();
    for row in luma.chunks_mut(width).take(MIN_HEIGHT).skip(ORIGIN - CELL) {
        row[ORIGIN - CELL..MIN_WIDTH].fill(BLACK);
    }
    for bit in 0..bytes.len() * 8 {
        let x = ORIGIN + (bit % COLUMNS) * CELL;
        let y = ORIGIN + (bit / COLUMNS) * CELL;
        let value = if bytes[bit / 8] & (1 << (bit % 8)) == 0 {
            BLACK
        } else {
            WHITE
        };
        for row in luma.chunks_mut(width).skip(y).take(CELL) {
            row[x..x + CELL].fill(value);
        }
    }
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x1021
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Error, Pattern};

    #[test]
    fn card_keeps_high_identity_bits_and_checks_geometry() -> Result<(), Error> {
        assert_eq!(checksum(b"123456789"), 0x29b1);
        let config =
            Config::new(152, 88, 1_000_000, Pattern::Checkerboard)?.with_identity(0x8000_0001)?;
        let mut frame = YuvFrame::new(config);
        frame.y_mut().fill(99);
        render_identity(&mut frame, config.identity_source().unwrap(), u64::MAX);
        let raw = frame.view();
        for &(x, y, value) in &[
            (0, 0, 99),
            (8, 8, 16),
            (16, 16, 235),
            (24, 16, 16),
            (16, 24, 235),
            (136, 32, 235),
            (136, 64, 235),
        ] {
            assert_eq!(raw.y()[y * 152 + x], value);
        }
        assert!(raw.u().iter().chain(raw.v()).all(|&value| value == 128));
        let small = Config::new(150, 88, 1_000_000, Pattern::Checkerboard)?;
        assert!(matches!(
            small.with_identity(1),
            Err(Error::IdentityCardTooSmall)
        ));
        Ok(())
    }
}
