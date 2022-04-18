#[cfg(any(feature = "h264", test))]
use crate::frame::{YuvFrame, BLACK, WHITE};
use crate::Error;

const ORIGIN: usize = 16;
const CELL: usize = 8;
const COLUMNS: usize = 16;
const ROWS: usize = 8;
pub(crate) const MIN_WIDTH: usize = ORIGIN + (COLUMNS + 1) * CELL;
pub(crate) const MIN_HEIGHT: usize = ORIGIN + (ROWS + 1) * CELL;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIdentity {
    pub source_id: u32,
    pub source_index: u64,
}

/// Unscaled luma only. `None` means too small, low contrast or failed card checks.
///
/// # Errors
///
/// [`Error::InvalidLumaLayout`] for invalid dimensions, stride, buffer length or overflow.
pub fn read_identity(
    luma: &[u8],
    width: usize,
    height: usize,
    stride: usize,
) -> Result<Option<FrameIdentity>, Error> {
    let length = height
        .checked_sub(1)
        .and_then(|rows| rows.checked_mul(stride))
        .and_then(|length| length.checked_add(width))
        .ok_or(Error::InvalidLumaLayout)?;
    if width == 0 || stride < width || luma.len() < length {
        return Err(Error::InvalidLumaLayout);
    }
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return Ok(None);
    }
    let mut bytes = [0u8; 16];
    for bit in 0..bytes.len() * 8 {
        let x = ORIGIN + (bit % COLUMNS) * CELL;
        let y = ORIGIN + (bit / COLUMNS) * CELL;
        let mut sum = 0u32;
        for row in y + 2..y + 6 {
            for &pixel in &luma[row * stride + x + 2..row * stride + x + 6] {
                sum += u32::from(pixel);
            }
        }
        match sum / 16 {
            0..=80 => {}
            180..=255 => bytes[bit / 8] |= 1 << (bit % 8),
            _ => return Ok(None),
        }
    }
    if bytes[..2] != [0x55, 0xd3]
        || checksum(&bytes[..14]) != u16::from_le_bytes([bytes[14], bytes[15]])
    {
        return Ok(None);
    }
    Ok(Some(FrameIdentity {
        source_id: u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]),
        source_index: u64::from_le_bytes([
            bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13],
        ]),
    }))
}

#[cfg(any(feature = "h264", test))]
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

    #[test]
    fn reads_compact_and_padded_cards_and_rejects_damage() -> Result<(), Error> {
        let mut frame = YuvFrame::new(Config::default());
        for &index in &[255, 256, u64::MAX] {
            render_identity(&mut frame, 0x8000_0001, index);
            let expected = Some(FrameIdentity {
                source_id: 0x8000_0001,
                source_index: index,
            });
            assert_eq!(read_identity(frame.view().y(), 320, 240, 320)?, expected);
            let mut padded = vec![0; 239 * 336 + 320];
            for (source, dest) in frame.view().y().chunks(320).zip(padded.chunks_mut(336)) {
                dest[..320].copy_from_slice(source);
            }
            assert_eq!(read_identity(&padded, 320, 240, 336)?, expected);
        }
        for row in frame.y_mut().chunks_mut(320).skip(24).take(8) {
            row[16..24].fill(BLACK);
        }
        assert_eq!(read_identity(frame.view().y(), 320, 240, 320)?, None);
        frame.y_mut().fill(128);
        assert_eq!(read_identity(frame.view().y(), 320, 240, 320)?, None);
        Ok(())
    }

    #[test]
    fn rejects_invalid_luma_layouts_before_sampling() {
        for &(width, height, stride, length) in &[
            (0, 1, 1, 1),
            (320, 0, 320, 0),
            (320, 240, 319, 0),
            (320, 240, 320, 100),
            (320, usize::MAX, usize::MAX, 0),
        ] {
            assert!(matches!(
                read_identity(&vec![0; length], width, height, stride),
                Err(Error::InvalidLumaLayout)
            ));
        }
        assert_eq!(read_identity(&[0; 16 * 16], 16, 16, 16).unwrap(), None);
    }
}
