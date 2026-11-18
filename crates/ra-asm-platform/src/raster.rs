//! The software rasterizer the graphics processors share: which pixels a triangle, a line or a
//! rectangle covers, and where each lies between the primitive's vertices. What a pixel is (its
//! format, its blending, its tests) is the console's: a console hands a closure that takes each
//! covered pixel with its weights and does the rest.
//!
//! It is the reference renderer, exact and slow: one edge function per pixel, no spans, no
//! threads. Positions carry `sub` fractional bits (none on the first PlayStation, four on the
//! second), and a pixel is covered when its sample point is inside by the top-left rule, so two
//! triangles that share an edge draw each pixel of it once.

/// A rectangle of pixels, its right and bottom edges outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    pub fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    pub fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    pub fn holds(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Where a pixel lies in a triangle: the weight of each vertex, which sum to `area`.
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub w: [i64; 3],
    pub area: i64,
}

impl Weights {
    /// The value at the pixel of an attribute the vertices hold as `a`, rounded toward zero.
    pub fn int(&self, a: [i32; 3]) -> i32 {
        ((self.w[0] * i64::from(a[0]) + self.w[1] * i64::from(a[1]) + self.w[2] * i64::from(a[2])) / self.area) as i32
    }

    /// The value at the pixel of an attribute the vertices hold as `a`.
    pub fn float(&self, a: [f32; 3]) -> f32 {
        let area = self.area as f64;
        ((self.w[0] as f64 * f64::from(a[0]) + self.w[1] as f64 * f64::from(a[1]) + self.w[2] as f64 * f64::from(a[2])) / area) as f32
    }
}

fn edge(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> i64 {
    i64::from(b.0 - a.0) * i64::from(p.1 - a.1) - i64::from(b.1 - a.1) * i64::from(p.0 - a.0)
}

/// Calls `plot` for every pixel of `clip` the triangle `p` covers, with the pixel's weights.
/// The vertices are in units of `1 << sub` to a pixel; either winding draws.
pub fn triangle(mut p: [(i32, i32); 3], sub: u32, clip: Rect, mut plot: impl FnMut(i32, i32, Weights)) {
    let mut area = edge(p[0], p[1], p[2]);
    if area == 0 {
        return;
    }
    // The weights are taken with the vertices wound one way: a triangle wound the other has its
    // last two swapped, and its weights swapped back.
    let flipped = area < 0;
    if flipped {
        p.swap(1, 2);
        area = -area;
    }
    let one = 1i32 << sub;
    let floor = |v: i32| v >> sub;
    let ceil = |v: i32| (v + one - 1) >> sub;
    let bounds = Rect::new(
        floor(p[0].0.min(p[1].0).min(p[2].0)),
        floor(p[0].1.min(p[1].1).min(p[2].1)),
        ceil(p[0].0.max(p[1].0).max(p[2].0)) + 1,
        ceil(p[0].1.max(p[1].1).max(p[2].1)) + 1,
    )
    .intersect(clip);
    if bounds.is_empty() {
        return;
    }
    // An edge owns the pixels exactly on it when it is a top edge or a left one: with y down and
    // this winding, an edge going up, or a level edge going right.
    let owns = |a: (i32, i32), b: (i32, i32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        dy < 0 || (dy == 0 && dx > 0)
    };
    let bias = [i64::from(!owns(p[1], p[2])), i64::from(!owns(p[2], p[0])), i64::from(!owns(p[0], p[1]))];
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            let at = (x << sub, y << sub);
            let w = [edge(p[1], p[2], at), edge(p[2], p[0], at), edge(p[0], p[1], at)];
            if w[0] >= bias[0] && w[1] >= bias[1] && w[2] >= bias[2] {
                let w = if flipped { [w[0], w[2], w[1]] } else { w };
                plot(x, y, Weights { w, area });
            }
        }
    }
}

/// Calls `plot` for every pixel of `clip` on the line from `a` to `b`, both ends drawn, with how
/// far along the pixel lies: `step` of `steps`. The ends are whole pixels.
pub fn line(a: (i32, i32), b: (i32, i32), clip: Rect, mut plot: impl FnMut(i32, i32, i32, i32)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let steps = dx.abs().max(dy.abs());
    if steps == 0 {
        if clip.holds(a.0, a.1) {
            plot(a.0, a.1, 0, 1);
        }
        return;
    }
    for step in 0..=steps {
        // The minor axis rounds to the nearest pixel, a tie away from the start.
        let along = |d: i32| {
            let scaled = i64::from(d) * i64::from(step) * 2 + i64::from(steps) * i64::from(d.signum());
            (scaled / (i64::from(steps) * 2)) as i32
        };
        let (x, y) = (a.0 + along(dx), a.1 + along(dy));
        if clip.holds(x, y) {
            plot(x, y, step, steps);
        }
    }
}

/// Calls `plot` for every pixel of `rect` within `clip`, with the pixel's offset from the
/// rectangle's own corner (which a sprite's texture coordinates follow, clipped or not).
pub fn rect(rect: Rect, clip: Rect, mut plot: impl FnMut(i32, i32, i32, i32)) {
    let drawn = rect.intersect(clip);
    for y in drawn.top..drawn.bottom {
        for x in drawn.left..drawn.right {
            plot(x, y, x - rect.left, y - rect.top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Rect = Rect::new(0, 0, 16, 16);

    fn covered(p: [(i32, i32); 3], sub: u32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        triangle(p, sub, ALL, |x, y, _| out.push((x, y)));
        out
    }

    #[test]
    fn a_right_triangle_covers_its_top_and_left_edges_only() {
        let got = covered([(0, 0), (4, 0), (0, 4)], 0);
        let expected: Vec<(i32, i32)> = (0..4).flat_map(|y| (0..4 - y).map(move |x| (x, y))).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn two_triangles_of_a_square_draw_each_pixel_once_in_either_winding() {
        let mut seen = std::collections::BTreeMap::new();
        for p in [[(1, 1), (5, 1), (1, 5)], [(5, 5), (5, 1), (1, 5)]] {
            for at in covered(p, 0) {
                *seen.entry(at).or_insert(0) += 1;
            }
        }
        assert_eq!(seen.len(), 16, "{seen:?}");
        assert!(seen.values().all(|n| *n == 1), "{seen:?}");
    }

    #[test]
    fn the_weights_give_each_vertex_its_own_value() {
        let mut at_origin = None;
        triangle([(0, 0), (8, 0), (0, 8)], 0, ALL, |x, y, w| {
            if (x, y) == (0, 0) {
                at_origin = Some((w.int([255, 0, 0]), w.int([0, 255, 0])));
            }
            if (x, y) == (4, 0) {
                assert_eq!(w.int([0, 200, 0]), 100);
                assert!((w.float([0.0, 1.0, 0.0]) - 0.5).abs() < 1e-6);
            }
        });
        assert_eq!(at_origin, Some((255, 0)));
    }

    #[test]
    fn subpixel_positions_cover_the_pixels_whose_corner_is_inside() {
        // Half a pixel in: the first row and column fall outside.
        let got = covered([(8, 8), (72, 8), (8, 72)], 4);
        assert!(!got.contains(&(0, 0)));
        assert!(got.contains(&(1, 1)));
        assert_eq!(got.iter().filter(|(_, y)| *y == 1).count(), 3);
    }

    #[test]
    fn a_line_draws_both_ends_and_clips() {
        let mut got = Vec::new();
        line((0, 0), (3, 1), Rect::new(0, 0, 3, 3), |x, y, step, steps| got.push((x, y, step, steps)));
        assert_eq!(got, [(0, 0, 0, 3), (1, 0, 1, 3), (2, 1, 2, 3)]);
        let mut dot = Vec::new();
        line((2, 2), (2, 2), ALL, |x, y, _, _| dot.push((x, y)));
        assert_eq!(dot, [(2, 2)]);
    }

    #[test]
    fn a_clipped_rectangle_keeps_its_own_offsets() {
        let mut got = Vec::new();
        rect(Rect::new(-1, 0, 2, 1), ALL, |x, y, u, v| got.push((x, y, u, v)));
        assert_eq!(got, [(0, 0, 1, 0), (1, 0, 2, 0)]);
    }
}
