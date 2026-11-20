//! The PlayStation's sound processor: half a megabyte of sound memory, twenty-four voices that
//! each play ADPCM from it at a pitch, under an envelope, to a left and a right volume, and the
//! mix of them at 44100 samples a second, with the reverb unit's reflections of the voices that
//! are sent to it, which it keeps in the end of the sound memory.

use crate::adpcm::{self, History};

pub const RATE: u32 = 44_100;
const RAM_SIZE: usize = 512 * 1024;
const VOICES: usize = 24;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Default)]
struct Voice {
    /// The address of the block being played, in bytes, and the loop's start.
    at: usize,
    repeat: usize,
    /// The block's samples, and the three before them for the interpolation.
    samples: [i16; 28],
    last: i16,
    history: History,
    /// How far into the block the voice is, in 4096ths of a sample.
    counter: u32,
    phase: Phase,
    /// The envelope's level, `0..=0x7fff`, and the cycles until its next step.
    level: i32,
    wait: u32,
    /// The program set the loop's start itself: a block's own flag does not move it.
    repeat_set: bool,
}

pub struct Spu {
    ram: Vec<u8>,
    /// The registers as written, a sixteen-bit word each, from `0x1f801c00`.
    regs: [u16; 0x200],
    voices: Vec<Voice>,
    /// Where the next byte written through the transfer port goes.
    transfer: usize,
    /// The voices that reached their end since they were last keyed on.
    endx: u32,
    /// The fraction of a sample the clock has run past the last one, in cycles.
    cycles: u64,
    /// The samples mixed and not yet handed to the host.
    pub out: Vec<[i16; 2]>,
    /// The drive's sound still to mix in, a sample of it to each of the unit's own.
    pub cd: std::collections::VecDeque<[i16; 2]>,
    /// The reverb unit: whether this sample is one it steps on, its last output, and where it
    /// stands in its work area.
    reverb_tick: bool,
    reverb_out: [i32; 2],
    reverb_at: usize,
}

impl Default for Spu {
    fn default() -> Self {
        Self::new()
    }
}

/// The processor's cycles to a sample.
const CYCLES_PER_SAMPLE: u64 = 33_868_800 / RATE as u64;

const MAIN_VOLUME: usize = 0x180;
/// The reverb's output volume, the voices it hears, its work area's base, and its registers.
const REVERB_VOLUME: usize = 0x184;
const REVERB_ON: usize = 0x198;
const REVERB_BASE: usize = 0x1a2;
const REVERB: usize = 0x1c0;
const KEY_ON: usize = 0x188;
const KEY_OFF: usize = 0x18c;
const ENDX: usize = 0x19c;
const TRANSFER_ADDRESS: usize = 0x1a6;
const TRANSFER_FIFO: usize = 0x1a8;
const CONTROL: usize = 0x1aa;
/// The volume of the drive's sound, a side each.
const CD_VOLUME: usize = 0x1b0;
const STATUS: usize = 0x1ae;

impl Spu {
    pub fn new() -> Self {
        Self {
            ram: vec![0; RAM_SIZE],
            regs: [0; 0x200],
            voices: vec![Voice::default(); VOICES],
            transfer: 0,
            endx: 0,
            cycles: 0,
            out: Vec::new(),
            cd: std::collections::VecDeque::new(),
            reverb_tick: false,
            reverb_out: [0; 2],
            reverb_at: 0,
        }
    }

    /// The register at `offset` from the processor's first, sixteen bits wide.
    pub fn read(&self, offset: usize) -> u16 {
        let offset = offset & 0x3fe;
        match offset {
            ENDX => self.endx as u16,
            o if o == ENDX + 2 => (self.endx >> 16) as u16,
            // The status mirrors the control's mode bits, which a program waits to see.
            STATUS => self.regs[CONTROL / 2] & 0x3f,
            o if o < VOICES * 0x10 && o & 0xf == 0xc => self.voices[o >> 4].level as u16,
            o if o < VOICES * 0x10 && o & 0xf == 0xe => (self.voices[o >> 4].repeat / 8) as u16,
            _ => self.regs[offset / 2],
        }
    }

    pub fn write(&mut self, offset: usize, value: u16) {
        let offset = offset & 0x3fe;
        self.regs[offset / 2] = value;
        match offset {
            KEY_ON => self.key(u32::from(value), true),
            o if o == KEY_ON + 2 => self.key(u32::from(value) << 16, true),
            KEY_OFF => self.key(u32::from(value), false),
            o if o == KEY_OFF + 2 => self.key(u32::from(value) << 16, false),
            TRANSFER_ADDRESS => self.transfer = usize::from(value) * 8,
            TRANSFER_FIFO => self.push(&value.to_le_bytes()),
            o if o < VOICES * 0x10 && o & 0xf == 0xe => {
                let voice = &mut self.voices[o >> 4];
                voice.repeat = usize::from(value) * 8;
                voice.repeat_set = true;
            }
            _ => {}
        }
    }

    /// Bytes for the sound memory, as the transfer port or a DMA block writes them.
    pub fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.ram[self.transfer & (RAM_SIZE - 1)] = byte;
            self.transfer += 1;
        }
    }

    /// Bytes of the sound memory from the transfer address on, as a DMA block reads them.
    pub fn pull(&mut self, len: usize) -> Vec<u8> {
        let out = (0..len).map(|i| self.ram[(self.transfer + i) & (RAM_SIZE - 1)]).collect();
        self.transfer += len;
        out
    }

    fn key(&mut self, mask: u32, on: bool) {
        for n in (0..VOICES).filter(|n| mask >> n & 1 != 0) {
            let start = usize::from(self.regs[(n * 0x10 + 6) / 2]) * 8;
            let voice = &mut self.voices[n];
            if on {
                *voice = Voice { at: start, repeat: start, phase: Phase::Attack, counter: 28 << 12, ..Voice::default() };
                self.endx &= !(1 << n);
            } else if voice.phase != Phase::Off {
                voice.phase = Phase::Release;
                voice.wait = 0;
            }
        }
    }

    /// Lets `cycles` of the clock pass: the samples that fall in them are mixed into `out`.
    pub fn advance(&mut self, cycles: u64) {
        self.cycles += cycles;
        while self.cycles >= CYCLES_PER_SAMPLE {
            self.cycles -= CYCLES_PER_SAMPLE;
            let sample = self.mix();
            self.out.push(sample);
        }
    }

    fn mix(&mut self) -> [i16; 2] {
        let enabled = self.regs[CONTROL / 2] & 0x8000 != 0;
        let muted = self.regs[CONTROL / 2] & 0x4000 == 0;
        let mut sum = [0i32; 2];
        let mut echoed = [0i32; 2];
        let echo = u32::from(self.regs[REVERB_ON / 2]) | u32::from(self.regs[REVERB_ON / 2 + 1]) << 16;
        for n in 0..VOICES {
            if self.voices[n].phase == Phase::Off {
                continue;
            }
            let sample = self.voice_sample(n);
            self.envelope(n);
            let amplitude = sample * self.voices[n].level / 0x8000;
            for side in 0..2 {
                let heard = amplitude * volume(self.regs[(n * 0x10 + side * 2) / 2]) / 0x4000;
                sum[side] += heard;
                if echo >> n & 1 != 0 {
                    echoed[side] += heard;
                }
            }
        }
        // The reverb runs at half the rate: a step every other sample, its output held between.
        self.reverb_tick = !self.reverb_tick;
        if self.reverb_tick {
            self.reverb_out = self.reverb(echoed.map(|v| v.clamp(-0x8000, 0x7fff)));
        }
        // The drive's sound, at its own volume, when the control register lets it in.
        let cd = self.cd.pop_front().unwrap_or([0, 0]);
        if self.regs[CONTROL / 2] & 1 != 0 {
            for side in 0..2 {
                sum[side] += i32::from(cd[side]) * i32::from(self.regs[(CD_VOLUME + side * 2) / 2].cast_signed()) / 0x8000;
            }
        }
        if !enabled || muted {
            return [0, 0];
        }
        let side = |i: usize| {
            let dry = sum[i] * volume(self.regs[(MAIN_VOLUME + i * 2) / 2]) / 0x4000;
            let wet = self.reverb_out[i] * i32::from(self.regs[(REVERB_VOLUME + i * 2) / 2].cast_signed()) / 0x8000;
            (dry + wet).clamp(-0x8000, 0x7fff) as i16
        };
        [side(0), side(1)]
    }

    /// One step of the reverb unit on the two inputs: the reflections through the sound memory's
    /// work area, by the coefficients and the offsets of its thirty-two registers, as the
    /// hardware's formula has them. The work area runs from its base to the memory's end, and
    /// every address is taken from where the unit stands in it, which moves a sample a step.
    fn reverb(&mut self, input: [i32; 2]) -> [i32; 2] {
        let base = usize::from(self.regs[REVERB_BASE / 2]) * 8;
        if self.regs[CONTROL / 2] & 0x80 == 0 || base >= RAM_SIZE - 16 {
            return [0, 0];
        }
        let span = RAM_SIZE - base;
        let at = self.reverb_at.clamp(base, RAM_SIZE - 2);
        let register = |spu: &Self, n: usize| spu.regs[(REVERB + 2 * n) / 2];
        // A coefficient, in 0x8000ths, and an offset into the work area, in bytes.
        let volume = |spu: &Self, n: usize| i32::from(register(spu, n).cast_signed());
        let place = |spu: &Self, n: usize, back: usize| base + (at - base + usize::from(register(spu, n)) * 8 + span - back) % span;
        let read = |spu: &Self, address: usize| {
            i32::from(i16::from_le_bytes([spu.ram[address & (RAM_SIZE - 1)], spu.ram[(address + 1) & (RAM_SIZE - 1)]]))
        };
        let clamp = |v: i32| v.clamp(-0x8000, 0x7fff);
        let (wall, iir) = (volume(self, 3 + 4), volume(self, 2));
        let mut writes = Vec::with_capacity(8);
        // The same-side and the cross-side reflections: each a low-pass of the input and of the
        // wall's reflection of the other tap.
        let inputs = [input[0] * volume(self, 0x1e) / 0x8000, input[1] * volume(self, 0x1f) / 0x8000];
        for (side, (store, same_from, diff_store, diff_from)) in
            [(0x0a, 0x10, 0x12, 0x19), (0x0b, 0x11, 0x13, 0x18)].into_iter().enumerate()
        {
            for (to, from) in [(store, same_from), (diff_store, diff_from)] {
                let before = read(self, place(self, to, 2));
                let reflected = inputs[side] + read(self, place(self, from, 0)) * wall / 0x8000;
                writes.push((place(self, to, 0), clamp((reflected - before) * iir / 0x8000 + before)));
            }
        }
        // The four combs summed, then through the two all-pass filters.
        let mut out = [0i32; 2];
        for side in 0..2 {
            let comb = |n: usize, coefficient: usize| read(self, place(self, n + side, 0)) * volume(self, coefficient) / 0x8000;
            let mut sound = clamp(comb(0x0c, 3) + comb(0x0e, 4) + comb(0x14, 5) + comb(0x16, 6));
            for (filter, coefficient, delay) in [(0x1a, 8, 0), (0x1c, 9, 1)] {
                let delayed = read(self, place(self, filter + side, usize::from(register(self, delay)) * 8));
                sound = clamp(sound - volume(self, coefficient) * delayed / 0x8000);
                writes.push((place(self, filter + side, 0), sound));
                sound = clamp(sound * volume(self, coefficient) / 0x8000 + delayed);
            }
            out[side] = sound;
        }
        for (address, value) in writes {
            let [low, high] = (value as i16).to_le_bytes();
            self.ram[address & (RAM_SIZE - 1)] = low;
            self.ram[(address + 1) & (RAM_SIZE - 1)] = high;
        }
        self.reverb_at = if at + 2 >= RAM_SIZE { base } else { at + 2 };
        out
    }

    /// The voice's sample now, between the two decoded samples its counter lies between, and the
    /// counter moved on by the pitch.
    fn voice_sample(&mut self, n: usize) -> i32 {
        let pitch = u32::from(self.regs[(n * 0x10 + 4) / 2]).min(0x4000);
        let voice = &mut self.voices[n];
        while voice.counter >= 28 << 12 {
            voice.counter -= 28 << 12;
            voice.last = voice.samples[27];
            let mut block = [0u8; 16];
            for (i, byte) in block.iter_mut().enumerate() {
                *byte = self.ram[(voice.at + i) & (RAM_SIZE - 1)];
            }
            let (samples, flags) = adpcm::decode(&block, &mut voice.history);
            voice.samples = samples;
            if flags & adpcm::LOOP_START != 0 && !voice.repeat_set {
                voice.repeat = voice.at;
            }
            voice.at += 16;
            if flags & adpcm::LOOP_END != 0 {
                self.endx |= 1 << n;
                voice.at = voice.repeat;
                if flags & adpcm::LOOP_REPEAT == 0 {
                    voice.phase = Phase::Off;
                    voice.level = 0;
                    return 0;
                }
            }
        }
        let index = (voice.counter >> 12) as usize;
        let fraction = (voice.counter & 0xfff) as i32;
        let before = i32::from(if index == 0 { voice.last } else { voice.samples[index - 1] });
        let now = i32::from(voice.samples[index]);
        voice.counter += pitch;
        before + (now - before) * fraction / 0x1000
    }

    /// One sample's step of the voice's envelope, by the two ADSR registers.
    fn envelope(&mut self, n: usize) {
        let (low, high) = (u32::from(self.regs[(n * 0x10 + 8) / 2]), u32::from(self.regs[(n * 0x10 + 10) / 2]));
        let voice = &mut self.voices[n];
        let sustain_level = (((low & 0xf) + 1) * 0x800) as i32;
        // Each phase's rate: exponential or linear, rising or falling, a shift and a step.
        let (exponential, rising, shift, step) = match voice.phase {
            Phase::Off => return,
            Phase::Attack => (low & 0x8000 != 0, true, (low >> 10) & 0x1f, 7 - ((low >> 8) & 3) as i32),
            Phase::Decay => (true, false, (low >> 4) & 0xf, -8),
            Phase::Sustain => {
                let rising = high & 0x4000 == 0;
                let step = ((high >> 6) & 3) as i32;
                (high & 0x8000 != 0, rising, (high >> 8) & 0x1f, if rising { 7 - step } else { -8 + step })
            }
            Phase::Release => (high & 0x20 != 0, false, high & 0x1f, -8),
        };
        if voice.wait > 0 {
            voice.wait -= 1;
            return;
        }
        let mut wait = 1u32 << shift.saturating_sub(11);
        let mut step = step << 11u32.saturating_sub(shift);
        if exponential && rising && voice.level > 0x6000 {
            wait *= 4;
        }
        if exponential && !rising {
            step = step * voice.level / 0x8000;
        }
        voice.wait = wait - 1;
        voice.level = (voice.level + step).clamp(0, 0x7fff);
        match voice.phase {
            Phase::Attack if voice.level >= 0x7fff => voice.phase = Phase::Decay,
            Phase::Decay if voice.level <= sustain_level => voice.phase = Phase::Sustain,
            Phase::Release if voice.level == 0 => voice.phase = Phase::Off,
            _ => {}
        }
    }
}

/// A volume register as a factor in 0x4000ths: a fixed volume is its fifteen bits, signed; a
/// sweep is played at its full level, which is where it ends.
fn volume(register: u16) -> i32 {
    if register & 0x8000 == 0 { i32::from((register << 1).cast_signed()) / 2 } else { 0x3fff }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound of two blocks, the second the end without a loop: a ramp of the largest nibble.
    fn loaded() -> Spu {
        let mut spu = Spu::new();
        spu.write(TRANSFER_ADDRESS, 0x200);
        let mut sound = [0u8; 32];
        sound[0] = 0x00;
        sound[2..16].fill(0x77);
        sound[16] = 0x00;
        sound[17] = adpcm::LOOP_END;
        spu.push(&sound);
        spu.write(MAIN_VOLUME, 0x3fff);
        spu.write(MAIN_VOLUME + 2, 0x3fff);
        spu.write(0x00, 0x3fff);
        spu.write(0x02, 0x1000);
        spu.write(0x04, 0x1000);
        spu.write(0x06, 0x200);
        // The fastest attack, the slowest decay, the highest sustain, held.
        spu.write(0x08, 0x00ff);
        spu.write(0x0a, 0x0000);
        spu.write(CONTROL, 0xc000);
        spu
    }

    #[test]
    fn the_status_mirrors_the_control_and_the_port_fills_the_memory() {
        let mut spu = loaded();
        spu.write(CONTROL, 0xc023);
        assert_eq!(spu.read(STATUS), 0x23);
        assert_eq!(&spu.ram[0x1000..0x1003], &[0x00, 0x00, 0x77]);
        spu.write(TRANSFER_ADDRESS, 0x200);
        assert_eq!(spu.pull(3), [0x00, 0x00, 0x77]);
    }

    #[test]
    fn a_keyed_voice_plays_its_blocks_and_ends() {
        let mut spu = loaded();
        spu.write(KEY_ON, 1);
        spu.advance(CYCLES_PER_SAMPLE * 60);
        assert_eq!(spu.out.len(), 60);
        // The nibble 7 at shift 0 is 0x7000: the left side near it at its volume, the right a
        // quarter of that, once the attack is up.
        let [left, right] = spu.out[20];
        assert!((0x6000..=0x7000).contains(&left), "{left:#x}");
        assert!((i32::from(left) / 4 - i32::from(right)).abs() <= 2, "{left:#x} {right:#x}");
        assert_eq!(spu.out[59], [0, 0], "the sound ended");
        assert_eq!(spu.read(ENDX) & 1, 1);
        assert_eq!(spu.voices[0].phase, Phase::Off);
    }

    #[test]
    fn the_reverb_gives_its_input_back_after_the_delay_its_offsets_make() {
        let mut spu = Spu::new();
        spu.write(CONTROL, 0xc080);
        spu.write(REVERB_BASE, 0xf000);
        let reg = |n: usize| REVERB + 2 * n;
        // The input at full volume into the left same-side tap, 32 bytes ahead of where the
        // first comb reads: sixteen steps later it is there. The two all-pass filters, with no
        // feedback, each hold it four steps more.
        let registers = [(0x1e, 0x7fff), (2, 0x7fff), (0x0a, 4), (3, 0x7fff), (0x0c, 0), (0, 1), (1, 1)];
        let apart = [(0x1a, 0x40), (0x1c, 0x80), (0x12, 0x100), (0x0b, 0x200), (0x13, 0x300)];
        for (n, value) in registers.into_iter().chain(apart) {
            spu.write(reg(n), value);
        }
        let first = spu.reverb([0x4000, 0]);
        assert_eq!(first, [0, 0]);
        let later: Vec<i32> = (0..30).map(|_| spu.reverb([0, 0])[0]).collect();
        let heard = later.iter().position(|v| *v != 0).expect("the echo");
        assert_eq!(heard, 23, "{later:?}");
        assert!((0x3ff0..=0x4000).contains(&later[heard]), "{:#x}", later[heard]);
        assert!(later[heard + 1..].iter().all(|v| *v == 0), "one echo of one impulse: {later:?}");
        // With the unit off in the control register nothing is reflected.
        spu.write(CONTROL, 0xc000);
        assert_eq!(spu.reverb([0x4000, 0x4000]), [0, 0]);
    }

    #[test]
    fn a_muted_processor_plays_nothing_and_key_off_releases() {
        let mut spu = loaded();
        spu.write(CONTROL, 0x8000);
        spu.write(KEY_ON, 1);
        spu.advance(CYCLES_PER_SAMPLE * 10);
        assert!(spu.out.iter().all(|s| *s == [0, 0]));
        spu.write(KEY_OFF, 1);
        assert_eq!(spu.voices[0].phase, Phase::Release);
    }
}
