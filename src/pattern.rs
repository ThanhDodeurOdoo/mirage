use crate::frame::{YuvFrame, BLACK};

const CELL_SIZE: usize = 8;
const WHITE: u8 = 235;
const MOTION_PERIOD: u64 = 240;

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

pub(crate) fn render_moving_rectangle(frame: &mut YuvFrame, source_index: u64) {
    let raw = frame.view();
    let width = raw.width() as usize;
    let height = raw.height() as usize;
    let rectangle_width = width / 4;
    let rectangle_height = height / 4;
    let phase = source_index % MOTION_PERIOD;
    let progress = phase.min(MOTION_PERIOD - phase) as usize;
    let half_period = (MOTION_PERIOD / 2) as usize;
    let x = (width - rectangle_width) * progress / half_period;
    let y = (height - rectangle_height) * progress / half_period;
    let luma = frame.y_mut();
    luma.fill(BLACK);
    for row in luma.chunks_mut(width).skip(y).take(rectangle_height) {
        row[x..x + rectangle_width].fill(WHITE);
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

    fn assert_rectangle(frame: &YuvFrame, left: usize, top: usize, width: usize, height: usize) {
        let raw = frame.view();
        for (y, row) in raw.y().chunks(raw.width() as usize).enumerate() {
            for (x, &value) in row.iter().enumerate() {
                let inside = (left..left + width).contains(&x) && (top..top + height).contains(&y);
                assert_eq!(value, if inside { 235 } else { 16 }, "pixel ({}, {})", x, y);
            }
        }
        assert!(raw.u().iter().all(|&value| value == 128));
        assert!(raw.v().iter().all(|&value| value == 128));
    }

    #[test]
    fn moving_rectangle_reaches_corners() {
        for &(width, height, rectangle_width, rectangle_height) in &[
            (16, 16, 4, 4),
            (18, 18, 4, 4),
            (320, 240, 80, 60),
            (1920, 1080, 480, 270),
            (16, 1080, 4, 270),
            (1920, 16, 480, 4),
        ] {
            let config = Config::new(width, height, 1_000_000, Pattern::MovingRectangle).unwrap();
            let mut frame = YuvFrame::new(config);
            let travel_x = width as usize - rectangle_width;
            let travel_y = height as usize - rectangle_height;
            for &(index, x, y) in &[
                (0, 0, 0),
                (60, travel_x / 2, travel_y / 2),
                (120, travel_x, travel_y),
                (240, 0, 0),
            ] {
                render_moving_rectangle(&mut frame, index);
                assert_rectangle(&frame, x, y, rectangle_width, rectangle_height);
            }
        }
    }

    #[test]
    fn moving_rectangle_bounces_without_trails() {
        let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle).unwrap();
        let mut frame = YuvFrame::new(config);
        frame.y_mut().fill(89);
        render_moving_rectangle(&mut frame, 119);
        assert_rectangle(&frame, 238, 178, 80, 60);
        let raw = frame.view();
        let expected = raw.y().to_vec();
        let addresses = (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr());
        render_moving_rectangle(&mut frame, 120);
        assert_rectangle(&frame, 240, 180, 80, 60);
        render_moving_rectangle(&mut frame, 121);
        assert_eq!(frame.view().y(), expected.as_slice());
        for &previous_luma in &[0, 255] {
            frame.y_mut().fill(previous_luma);
            render_moving_rectangle(&mut frame, 121);
            let raw = frame.view();
            assert_eq!(raw.y(), expected.as_slice());
            assert_eq!(
                (raw.y().as_ptr(), raw.u().as_ptr(), raw.v().as_ptr()),
                addresses
            );
            assert!(raw.u().iter().all(|&value| value == 128));
            assert!(raw.v().iter().all(|&value| value == 128));
        }
    }

    #[test]
    fn moving_rectangle_uses_only_source_index() {
        let config = Config::new(18, 18, 1_000_000, Pattern::MovingRectangle).unwrap();
        let mut sequential = YuvFrame::new(config);
        for index in 0..=137 {
            render_moving_rectangle(&mut sequential, index);
        }
        let mut direct = YuvFrame::new(config);
        render_moving_rectangle(&mut direct, 137);
        assert_rectangle(&direct, 12, 12, 4, 4);
        assert_eq!(direct.view().y(), sequential.view().y());
    }

    #[test]
    fn moving_rectangle_handles_large_indices() {
        let config = Config::new(320, 240, 1_000_000, Pattern::MovingRectangle).unwrap();
        let mut frame = YuvFrame::new(config);
        render_moving_rectangle(&mut frame, u64::MAX);
        assert_rectangle(&frame, 30, 22, 80, 60);
        let expected = frame.view().y().to_vec();
        render_moving_rectangle(&mut frame, 15);
        assert_eq!(frame.view().y(), expected.as_slice());
        render_moving_rectangle(&mut frame, u64::MAX - 240);
        assert_eq!(frame.view().y(), expected.as_slice());
    }
}
