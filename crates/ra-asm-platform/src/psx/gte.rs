//! The PlayStation's geometry transformation engine, coprocessor 2 of its processor: sixty-four
//! registers (thirty-two of data, thirty-two of control) and the commands that work on them, the
//! perspective transformation, the normal clip, the lighting and colour commands and the depth
//! cue. A command is a pure function of the registers, done here in the fixed-point arithmetic
//! the hardware does it in, with its saturations and the flags they leave.
//!
//! The registers are a slice of sixty-four words as a `mfc2` and a `cfc2` read them: the data
//! registers first, then the control registers.

/// The data registers.
const VXY0: usize = 0;
const RGBC: usize = 6;
const OTZ: usize = 7;
const IR0: usize = 8;
const IR1: usize = 9;
const SXY0: usize = 12;
const SXY2: usize = 14;
const SZ0: usize = 16;
const SZ3: usize = 19;
const RGB0: usize = 20;
const RGB2: usize = 22;
const MAC0: usize = 24;
const MAC1: usize = 25;
/// The control registers: the three matrices with the vector each is added to, the far colour,
/// the screen offset, the projection distance, the depth-cue coefficients, the average scales,
/// and the flags.
const ROTATION: usize = 32;
const TRANSLATION: usize = 37;
const LIGHT: usize = 40;
const BACKGROUND: usize = 45;
const COLOUR: usize = 48;
const FAR: usize = 53;
const OFX: usize = 56;
const OFY: usize = 57;
const H: usize = 58;
const DQA: usize = 59;
const DQB: usize = 60;
const ZSF3: usize = 61;
const ZSF4: usize = 62;
const FLAG: usize = 63;

/// The reciprocals the division starts from, for a divisor's top nine bits.
fn reciprocal(index: usize) -> i64 {
    ((0x40000 / (index as i64 + 0x100) + 1) / 2 - 0x101).max(0)
}

struct Gte<'a> {
    r: &'a mut [i32; 64],
    flag: u32,
    /// The shift of the command's results: its `sf` bit, twelve or none.
    shift: u32,
    /// The command's `lm` bit: the three `IR` saturate at zero below.
    floor: bool,
}

impl Gte<'_> {
    /// The row `row` of the matrix at `at`: nine sixteen-bit entries packed two to a register.
    fn matrix(&self, at: usize, row: usize) -> [i64; 3] {
        [0, 1, 2].map(|column| {
            let n = row * 3 + column;
            i64::from((self.r[at + n / 2] >> (16 * (n % 2))) as i16)
        })
    }

    /// The vector `n`: one of the three `V`, or the three `IR`.
    fn vector(&self, n: usize) -> [i64; 3] {
        if n == 3 {
            return [0, 1, 2].map(|i| i64::from(self.r[IR1 + i] as i16));
        }
        let xy = self.r[VXY0 + 2 * n];
        [i64::from(xy as i16), i64::from((xy >> 16) as i16), i64::from(self.r[VXY0 + 2 * n + 1] as i16)]
    }

    fn words(&self, at: usize) -> [i64; 3] {
        [0, 1, 2].map(|i| i64::from(self.r[at + i]))
    }

    /// A sum on its way to `MAC1..3`, which is 44 bits wide: past either end sets its flag.
    fn wide(&mut self, n: usize, value: i64) -> i64 {
        if value > 0x7ff_ffff_ffff {
            self.flag |= 1 << (30 - n);
        }
        if value < -0x800_0000_0000 {
            self.flag |= 1 << (27 - n);
        }
        (value << 20) >> 20
    }

    /// `translation * 4096 + matrix * vector`, each row checked as its terms add up.
    fn transform(&mut self, translation: [i64; 3], matrix: usize, vector: [i64; 3]) -> [i64; 3] {
        [0, 1, 2].map(|row| {
            let m = self.matrix(matrix, row);
            let mut sum = self.wide(row, (translation[row] << 12) + m[0] * vector[0]);
            sum = self.wide(row, sum + m[1] * vector[1]);
            self.wide(row, sum + m[2] * vector[2])
        })
    }

    /// Stores `MAC1..3` from the sums, shifted, and `IR1..3` from them, saturated.
    fn set_mac_ir(&mut self, sums: [i64; 3], floor: bool) {
        for (n, sum) in sums.into_iter().enumerate() {
            let sum = self.wide(n, sum);
            let mac = (sum >> self.shift) as i32;
            self.r[MAC1 + n] = mac;
            self.r[IR1 + n] = self.saturate_ir(n, mac, floor);
        }
    }

    fn saturate_ir(&mut self, n: usize, value: i32, floor: bool) -> i32 {
        let low = if floor { 0 } else { -0x8000 };
        if value < low || value > 0x7fff {
            self.flag |= 1 << (24 - n);
        }
        value.clamp(low, 0x7fff)
    }

    fn set_mac0(&mut self, value: i64) {
        if value > 0x7fff_ffff {
            self.flag |= 1 << 16;
        }
        if value < -0x8000_0000 {
            self.flag |= 1 << 15;
        }
        self.r[MAC0] = value as i32;
    }

    fn push_sz(&mut self, value: i64) {
        if !(0..=0xffff).contains(&value) {
            self.flag |= 1 << 18;
        }
        self.r.copy_within(SZ0 + 1..=SZ3, SZ0);
        self.r[SZ3] = value.clamp(0, 0xffff) as i32;
    }

    fn push_colour(&mut self) {
        let mut packed = self.r[RGBC].cast_unsigned() & 0xff00_0000;
        for n in 0..3 {
            let channel = self.r[MAC1 + n] >> 4;
            if !(0..=0xff).contains(&channel) {
                self.flag |= 1 << (21 - n);
            }
            packed |= (channel.clamp(0, 0xff) as u32) << (8 * n);
        }
        self.r.copy_within(RGB0 + 1..=RGB2, RGB0);
        self.r[RGB2] = packed.cast_signed();
    }

    /// The perspective transformation of the vector `v`: into the screen's coordinates and
    /// depth FIFOs, with the depth cue's `IR0` when `last`.
    fn perspective(&mut self, v: usize, last: bool) {
        let vector = self.vector(v);
        let translation = self.words(TRANSLATION);
        let sums = self.transform(translation, ROTATION, vector);
        // The third `IR` saturates as the others do, but its flag is for the depth before the
        // command's shift.
        for (n, sum) in sums.into_iter().enumerate() {
            self.r[MAC1 + n] = (sum >> self.shift) as i32;
        }
        for n in 0..2 {
            self.r[IR1 + n] = self.saturate_ir(n, self.r[MAC1 + n], self.floor);
        }
        let depth = sums[2] >> 12;
        if !(-0x8000..=0x7fff).contains(&depth) {
            self.flag |= 1 << 22;
        }
        let low = if self.floor { 0 } else { -0x8000 };
        self.r[IR1 + 2] = self.r[MAC1 + 2].clamp(low, 0x7fff);
        self.push_sz(depth);

        // The projection: `H / SZ3`, by a table's reciprocal refined once, at most 0x1ffff.
        let (h, sz) = (i64::from(self.r[H] as u16), i64::from(self.r[SZ3]));
        let quotient = if h < sz * 2 {
            let zeros = (sz as u16).leading_zeros();
            let (n, d) = (h << zeros, sz << zeros);
            let u = reciprocal(((d - 0x7fc0) >> 7) as usize) + 0x101;
            let d = (0x200_0080 - d * u) >> 8;
            let d = (0x80 + d * u) >> 8;
            ((n * d + 0x8000) >> 16).min(0x1_ffff)
        } else {
            self.flag |= 1 << 17;
            0x1_ffff
        };
        let mut screen = [0i32; 2];
        for (axis, offset) in [OFX, OFY].into_iter().enumerate() {
            let value = quotient * i64::from(self.r[IR1 + axis]) + i64::from(self.r[offset]);
            self.set_mac0(value);
            let coordinate = value >> 16;
            if !(-0x400..=0x3ff).contains(&coordinate) {
                self.flag |= 1 << (14 - axis);
            }
            screen[axis] = coordinate.clamp(-0x400, 0x3ff) as i32;
        }
        self.r.copy_within(SXY0 + 1..=SXY2, SXY0);
        self.r[SXY2] = (screen[0] & 0xffff) | (screen[1] << 16);
        if last {
            let cue = quotient * i64::from(self.r[DQA] as i16) + i64::from(self.r[DQB]);
            self.set_mac0(cue);
            let ir0 = cue >> 12;
            if !(0..=0x1000).contains(&ir0) {
                self.flag |= 1 << 12;
            }
            self.r[IR0] = ir0.clamp(0, 0x1000) as i32;
        }
    }

    /// The depth cue: the sums move toward the far colour by `IR0`, then are stored.
    fn cue(&mut self, sums: [i64; 3]) {
        let far = self.words(FAR);
        let ir0 = i64::from(self.r[IR0] as i16);
        let mut moved = [0i64; 3];
        for n in 0..3 {
            let step = self.wide(n, (far[n] << 12) - sums[n]) >> self.shift;
            let step = i64::from(self.saturate_ir(n, step as i32, false));
            moved[n] = step * ir0 + sums[n];
        }
        self.set_mac_ir(moved, self.floor);
    }

    /// The light of a normal: the light matrix by the vector, then the colour matrix by that
    /// with the background colour.
    fn light(&mut self, v: usize) {
        let vector = self.vector(v);
        let lit = self.transform([0; 3], LIGHT, vector);
        self.set_mac_ir(lit, self.floor);
        self.background();
    }

    fn background(&mut self) {
        let (background, ir) = (self.words(BACKGROUND), self.vector(3));
        let coloured = self.transform(background, COLOUR, ir);
        self.set_mac_ir(coloured, self.floor);
    }

    /// The primary colour's channels times the three `IR`, as sums.
    fn shaded(&self) -> [i64; 3] {
        let rgb = self.r[RGBC].cast_unsigned();
        [0, 1, 2].map(|n| (i64::from((rgb >> (8 * n)) & 0xff) * i64::from(self.r[IR1 + n] as i16)) << 4)
    }

    fn run(&mut self, command: u32) {
        let screen = |gte: &Self, n: usize| (i64::from(gte.r[SXY0 + n] as i16), i64::from((gte.r[SXY0 + n] >> 16) as i16));
        match command & 0x3f {
            // RTPS, RTPT.
            0x01 => self.perspective(0, true),
            0x30 => (0..3).for_each(|v| self.perspective(v, v == 2)),
            // NCLIP: twice the signed area of the screen triangle.
            0x06 => {
                let ((x0, y0), (x1, y1), (x2, y2)) = (screen(self, 0), screen(self, 1), screen(self, 2));
                self.set_mac0(x0 * y1 + x1 * y2 + x2 * y0 - x0 * y2 - x1 * y0 - x2 * y1);
            }
            // OP: the cross product of the rotation's diagonal and the `IR`.
            0x0c => {
                let d = [0, 1, 2].map(|n| self.matrix(ROTATION, n)[n]);
                let ir = self.vector(3);
                self.set_mac_ir([ir[2] * d[1] - ir[1] * d[2], ir[0] * d[2] - ir[2] * d[0], ir[1] * d[0] - ir[0] * d[1]], self.floor);
            }
            // DPCS, DPCT: the primary colour, or each of the FIFO's three, depth-cued.
            0x10 | 0x2a => {
                for _ in 0..if command & 0x3f == 0x10 { 1 } else { 3 } {
                    let rgb = if command & 0x3f == 0x10 { self.r[RGBC] } else { self.r[RGB0] }.cast_unsigned();
                    self.cue([0, 1, 2].map(|n| i64::from((rgb >> (8 * n)) & 0xff) << 16));
                    self.push_colour();
                }
            }
            // INTPL: the `IR`, depth-cued.
            0x11 => {
                let ir = self.vector(3);
                self.cue(ir.map(|v| v << 12));
                self.push_colour();
            }
            // MVMVA: a matrix by a vector, with a vector added, as the command's fields pick.
            0x12 => {
                let matrix = [ROTATION, LIGHT, COLOUR][((command >> 17) & 3).min(2) as usize];
                let vector = self.vector(((command >> 15) & 3) as usize);
                let translation = match (command >> 13) & 3 {
                    0 => self.words(TRANSLATION),
                    1 => self.words(BACKGROUND),
                    2 => self.words(FAR),
                    _ => [0; 3],
                };
                let sums = self.transform(translation, matrix, vector);
                self.set_mac_ir(sums, self.floor);
            }
            // NCDS, NCDT: the light of a normal on the primary colour, depth-cued.
            0x13 | 0x16 => {
                for v in 0..if command & 0x3f == 0x13 { 1 } else { 3 } {
                    self.light(v);
                    self.cue(self.shaded());
                    self.push_colour();
                }
            }
            // CDP, CC: the `IR` as the light's colour on the primary colour, depth-cued or not.
            0x14 | 0x1c => {
                self.background();
                if command & 0x3f == 0x14 {
                    self.cue(self.shaded());
                } else {
                    self.set_mac_ir(self.shaded(), self.floor);
                }
                self.push_colour();
            }
            // NCCS, NCCT: the light of a normal on the primary colour.
            0x1b | 0x3f => {
                for v in 0..if command & 0x3f == 0x1b { 1 } else { 3 } {
                    self.light(v);
                    self.set_mac_ir(self.shaded(), self.floor);
                    self.push_colour();
                }
            }
            // NCS, NCT: the light of a normal.
            0x1e | 0x20 => {
                for v in 0..if command & 0x3f == 0x1e { 1 } else { 3 } {
                    self.light(v);
                    self.push_colour();
                }
            }
            // SQR: the `IR` squared.
            0x28 => {
                let ir = self.vector(3);
                self.set_mac_ir(ir.map(|v| v * v), self.floor);
            }
            // DCPL: the primary colour by the `IR`, depth-cued.
            0x29 => {
                self.cue(self.shaded());
                self.push_colour();
            }
            // AVSZ3, AVSZ4: the average of the depth FIFO, scaled, as an ordering table's index.
            0x2d | 0x2e => {
                let (scale, first) = if command & 0x3f == 0x2d { (ZSF3, SZ0 + 1) } else { (ZSF4, SZ0) };
                let sum: i64 = (first..=SZ3).map(|n| i64::from(self.r[n])).sum();
                let average = i64::from(self.r[scale] as i16) * sum;
                self.set_mac0(average);
                if !(0..=0xffff).contains(&(average >> 12)) {
                    self.flag |= 1 << 18;
                }
                self.r[OTZ] = (average >> 12).clamp(0, 0xffff) as i32;
            }
            // GPF, GPL: the `IR` scaled by `IR0`, alone or added to the `MAC`.
            0x3d | 0x3e => {
                let (ir, ir0) = (self.vector(3), i64::from(self.r[IR0] as i16));
                let base = if command & 0x3f == 0x3e { [0, 1, 2].map(|n| i64::from(self.r[MAC1 + n]) << self.shift) } else { [0; 3] };
                self.set_mac_ir([0, 1, 2].map(|n| base[n] + ir[n] * ir0), self.floor);
                self.push_colour();
            }
            _ => {}
        }
    }
}

/// Runs the GTE command `command`, a `cop2` instruction's word, on the registers.
pub fn command(registers: &mut [i32; 64], command: u32) {
    let mut gte = Gte { r: registers, flag: 0, shift: if command & (1 << 19) != 0 { 12 } else { 0 }, floor: command & (1 << 10) != 0 };
    gte.run(command);
    // The top bit is the OR of the flags that are errors.
    let flag = gte.flag;
    gte.r[FLAG] = (flag | if flag & 0x7f87_e000 != 0 { 1 << 31 } else { 0 }).cast_signed();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registers of the GTE fixture (`testdata/src/psx/gte.c`): an identity rotation, the
    /// cube pushed 768 away, the screen's centre at (160, 120), `H` 256 and `ZSF3` a quarter.
    fn registers() -> [i32; 64] {
        let mut r = [0i32; 64];
        r[ROTATION] = 0x1000;
        r[ROTATION + 2] = 0x1000;
        r[ROTATION + 4] = 0x1000;
        r[TRANSLATION + 2] = 768;
        r[OFX] = 160 << 16;
        r[OFY] = 120 << 16;
        r[H] = 256;
        r[ZSF3] = 0x400;
        r
    }

    fn load(r: &mut [i32; 64], n: usize, [x, y, z]: [i16; 3]) {
        r[VXY0 + 2 * n] = i32::from(x as u16) | i32::from(y) << 16;
        r[VXY0 + 2 * n + 1] = i32::from(z);
    }

    fn sxy(r: &[i32; 64], n: usize) -> (i16, i16) {
        (r[SXY0 + n] as i16, (r[SXY0 + n] >> 16) as i16)
    }

    /// RTPT, with `sf` set as the SDK's macro sets it.
    const RTPT: u32 = 0x4a28_0030;
    const RTPS: u32 = 0x4a18_0001;

    #[test]
    fn three_vertices_transform_and_clip_as_the_fixture_expects() {
        let mut r = registers();
        for (n, vertex) in [[-256, -256, -256], [256, -256, -256], [-256, 256, -256]].into_iter().enumerate() {
            load(&mut r, n, vertex);
        }
        command(&mut r, RTPT);
        assert_eq!([sxy(&r, 0), sxy(&r, 1), sxy(&r, 2)], [(32, -8), (288, -8), (32, 248)]);
        assert_eq!((&r[SZ0 + 1..=SZ3], r[FLAG]), (&[512, 512, 512][..], 0));
        command(&mut r, 0x4b40_0006);
        assert_eq!(r[MAC0], 65536, "the normal clip");
        command(&mut r, 0x4b58_002d);
        assert_eq!(r[OTZ], 384, "the average depth");
    }

    #[test]
    fn a_vertex_too_near_saturates_and_flags_the_division() {
        let mut r = registers();
        load(&mut r, 0, [256, 256, -700]);
        command(&mut r, RTPS);
        assert_eq!((sxy(&r, 2), r[SZ3], r[FLAG].cast_unsigned()), ((671, 631), 68, 0x8002_0000));
        load(&mut r, 0, [1000, -1000, -700]);
        command(&mut r, RTPS);
        assert_eq!((sxy(&r, 2), r[SZ3], r[FLAG].cast_unsigned()), ((1023, -1024), 68, 0x8002_6000));
    }

    #[test]
    fn a_colour_is_cued_toward_the_far_colour_by_ir0() {
        let mut r = [0i32; 64];
        r[RGBC] = 0x2a40_8000u32.cast_signed();
        r[IR0] = 0x800;
        r[FAR] = 0xff0;
        // DPCS with `sf`: half way from (0, 0x80, 0x40) to (0xff, 0, 0).
        command(&mut r, 0x4a78_0010);
        assert_eq!(r[RGB2].cast_unsigned(), 0x2a20_407f);
        assert_eq!(r[FLAG], 0);
    }

    #[test]
    fn a_matrix_by_a_vector_keeps_what_the_command_shifts() {
        let mut r = registers();
        r[ROTATION] = 0x0800;
        load(&mut r, 1, [100, 50, 25]);
        // MVMVA: the rotation by V1, the translation added, `sf` set.
        command(&mut r, 0x4a48_8012);
        assert_eq!(&r[MAC1..MAC1 + 3], &[50, 50, 793]);
        assert_eq!(&r[IR1..IR1 + 3], &[50, 50, 793]);
    }
}
