use crate::frame::{YuvFrame, BLACK};

const CELL_SIZE: usize = 8;
const WHITE: u8 = 235;

pub(crate) fn render_checkerboard(frame: &mut YuvFrame) {
    let width = frame.view().width() as usize;
    for (y, row) in frame.y_mut().chunks_mut(width).enumerate() {
        for (x, cell) in row.chunks_mut(CELL_SIZE).enumerate() {
            cell.fill(if (x + y / CELL_SIZE) % 2 == 0 {
                BLACK
            } else {
                WHITE
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Pattern};

    #[test]
    fn renders_four_cells_at_minimum_size() {
        let config = Config::new(16, 16, 1_000_000, Pattern::Checkerboard).unwrap();
        let mut frame = YuvFrame::new(config);
        let dark_row = [
            16, 16, 16, 16, 16, 16, 16, 16, 235, 235, 235, 235, 235, 235, 235, 235,
        ];
        let light_row = [
            235, 235, 235, 235, 235, 235, 235, 235, 16, 16, 16, 16, 16, 16, 16, 16,
        ];
        let mut expected = dark_row.repeat(8);
        expected.extend_from_slice(&light_row.repeat(8));
        render_checkerboard(&mut frame);
        assert_eq!(frame.view().y(), expected.as_slice());
    }

    #[test]
    fn clips_partial_cells() {
        let config = Config::new(18, 18, 1_000_000, Pattern::Checkerboard).unwrap();
        let mut frame = YuvFrame::new(config);
        let dark_row = [
            16, 16, 16, 16, 16, 16, 16, 16, 235, 235, 235, 235, 235, 235, 235, 235, 16, 16,
        ];
        let light_row = [
            235, 235, 235, 235, 235, 235, 235, 235, 16, 16, 16, 16, 16, 16, 16, 16, 235, 235,
        ];
        let mut expected = dark_row.repeat(8);
        expected.extend_from_slice(&light_row.repeat(8));
        expected.extend_from_slice(&dark_row.repeat(2));
        frame.y_mut().fill(89);
        render_checkerboard(&mut frame);
        assert_eq!(frame.view().y(), expected.as_slice());
    }

    #[test]
    fn fully_redraws_default_frame() {
        let mut frame = YuvFrame::new(Config::default());
        frame.y_mut().fill(89);
        render_checkerboard(&mut frame);
        let raw = frame.view();
        let expected = raw.y().to_vec();
        let addresses = (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr());
        assert!(expected.iter().all(|&value| value == 16 || value == 235));
        for &(x, y, value) in &[
            (0, 0, 16),
            (7, 7, 16),
            (8, 0, 235),
            (0, 8, 235),
            (8, 8, 16),
            (319, 0, 235),
            (0, 239, 235),
            (319, 239, 16),
        ] {
            assert_eq!(expected[y * 320 + x], value);
        }
        for &previous_luma in &[0, 255] {
            frame.y_mut().fill(previous_luma);
            render_checkerboard(&mut frame);
            let raw = frame.view();
            assert_eq!(raw.y(), expected.as_slice());
            assert!(raw.u().iter().all(|&value| value == 128));
            assert!(raw.v().iter().all(|&value| value == 128));
            assert_eq!(
                (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr()),
                addresses
            );
        }
    }
}
