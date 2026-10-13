use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.left as f64
            && p.x < self.right as f64
            && p.y >= self.top as f64
            && p.y < self.bottom as f64
    }
}
pub fn zoom(z: f64) -> f64 {
    if z.is_finite() {
        z.clamp(1., 12.)
    } else {
        4.
    }
}
pub fn damp(current: f64, target: f64, dt: f64, time_constant: f64) -> f64 {
    if time_constant <= 0. {
        target
    } else {
        current + (target - current) * (1. - (-dt.max(0.) / time_constant).exp())
    }
}
pub fn source_rect(center: Point, width: i32, height: i32, factor: f64, bounds: Rect) -> Rect {
    let w = ((width as f64 / zoom(factor)).ceil() as i32).clamp(1, bounds.width().max(1));
    let h = ((height as f64 / zoom(factor)).ceil() as i32).clamp(1, bounds.height().max(1));
    let left = ((center.x - w as f64 / 2.).round() as i32).clamp(bounds.left, bounds.right - w);
    let top = ((center.y - h as f64 / 2.).round() as i32).clamp(bounds.top, bounds.bottom - h);
    Rect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}
pub fn placement(cursor: Point, w: i32, _h: i32, offset: Point, work: Rect) -> Point {
    let x = if cursor.x + offset.x + w as f64 > work.right as f64 {
        cursor.x - offset.x.abs() - w as f64
    } else {
        cursor.x + offset.x
    };
    // Keep the cursor offset independent of lens height. Growing the lens
    // must not flip it above the cursor; its bottom may extend past the display.
    let y = cursor.y + offset.y;
    Point {
        x: x.clamp(work.left as f64, (work.right - w).max(work.left) as f64),
        y: y.clamp(work.top as f64, (work.bottom - 1).max(work.top) as f64),
    }
}
pub fn logical_to_physical(value: f64, dpi: u32) -> i32 {
    (value * dpi as f64 / 96.).round() as i32
}
pub fn pixel_index(display_pixel: usize, factor: f64, source_size: usize) -> usize {
    ((display_pixel as f64 / zoom(factor)).floor() as usize).min(source_size.saturating_sub(1))
}
pub fn measurement(a: Point, b: Point) -> (i32, i32, f64) {
    let w = (b.x - a.x).round() as i32;
    let h = (b.y - a.y).round() as i32;
    (w.abs(), h.abs(), (w as f64).hypot(h as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    const B: Rect = Rect {
        left: -1920,
        top: -1080,
        right: 0,
        bottom: 0,
    };
    #[test]
    fn negative_origin_fractional_source() {
        let r = source_rect(
            Point {
                x: -1000.,
                y: -500.,
            },
            360,
            240,
            2.5,
            B,
        );
        assert_eq!((r.width(), r.height()), (144, 96));
        assert_eq!(r.left, -1072);
    }
    #[test]
    fn display_edges() {
        let r = source_rect(Point { x: -1., y: -1. }, 360, 240, 4., B);
        assert_eq!((r.right, r.bottom), (0, 0));
    }
    #[test]
    fn zoom_is_safe() {
        assert_eq!(zoom(f64::NAN), 4.);
        assert_eq!(zoom(100.), 12.);
        assert_eq!(zoom(-1.), 1.);
    }
    #[test]
    fn damping_frame_rate_independent() {
        let a = damp(0., 10., 0.1, 0.08);
        let b = damp(damp(0., 10., 0.05, 0.08), 10., 0.05, 0.08);
        assert!((a - b).abs() < 1e-9);
        assert!(a < 10.);
    }
    #[test]
    fn dpi_conversion() {
        assert_eq!(logical_to_physical(360., 144), 540);
    }
    #[test]
    fn placement_anchor_stays_in_work_area() {
        let p = placement(
            Point { x: -2., y: -2. },
            360,
            240,
            Point { x: 32., y: 32. },
            B,
        );
        assert!(p.x + 360. <= 0. && p.y >= B.top as f64 && p.y < B.bottom as f64);
    }
    #[test]
    fn growing_lens_keeps_vertical_cursor_offset() {
        let cursor = Point {
            x: -1000.,
            y: -400.,
        };
        let offset = Point { x: 32., y: 32. };
        for h in [120, 240, 400, 800, 120] {
            assert_eq!(placement(cursor, 360, h, offset, B).y, -368.);
        }
        // Explicit negative offsets still work; scaling does not alter them.
        assert_eq!(
            placement(cursor, 360, 800, Point { x: 32., y: -32. }, B).y,
            -432.
        );
    }
    #[test]
    fn original_pixel_measurement() {
        assert_eq!(
            measurement(Point { x: -100., y: 0. }, Point { x: -97., y: 4. }),
            (3, 4, 5.)
        );
    }
    #[test]
    fn fractional_pixel_mapping() {
        assert_eq!(pixel_index(4, 2.5, 10), 1);
        assert_eq!(pixel_index(5, 2.5, 10), 2);
        assert_eq!(pixel_index(200, 8., 5), 4);
    }
}
