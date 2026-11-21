//! The core of a Cortex-M microcontroller, as far as firmware with no board under it needs one:
//! the system timer of the System Control Space and the exception it raises.
//!
//! `SysTick` is three registers at `0xe000e010`: control and status (the enable, whether reaching
//! zero raises the exception, and the flag that says it reached zero since the last read), the
//! reload value, and the current value, which counts down by the instructions the program runs
//! and takes the reload value after zero. When it raises its exception, the run calls the handler
//! the vector table names for it (the fifteenth entry) on the registers of the code it
//! interrupts, between two instructions, and puts those registers back when the handler returns:
//! the processor stacks what a function may change and unstacks it at the handler's return, and
//! a handler is a function that keeps the rest.

use std::cell::RefCell;
use std::rc::Rc;

use ra_asm_ir::interp::memory::Device;
use ra_asm_ir::interp::{Stop, Value, Vm};
use ra_asm_pe::Image;

/// The System Control Space: the core's own registers.
const SCS: (u64, u64) = (0xe000_e000, 0x1000);
const CONTROL: u64 = 0xe000_e010;
const RELOAD: u64 = 0xe000_e014;
const CURRENT: u64 = 0xe000_e018;
/// How many instructions run between two looks at the timer.
const POLL: u64 = 32;
/// The entry of the vector table that is the timer's exception.
const SYSTICK: u64 = 15;

const ENABLE: u32 = 1;
const TICKINT: u32 = 2;
const COUNTFLAG: u32 = 1 << 16;

#[derive(Debug, Default)]
struct SysTick {
    control: u32,
    reload: u32,
    current: u32,
    /// The exception is raised and its handler has not run yet.
    pending: bool,
}

impl SysTick {
    /// Counts down by `instructions`, through zero as often as that takes.
    fn advance(&mut self, instructions: u64) {
        if self.control & ENABLE == 0 {
            return;
        }
        let current = u64::from(self.current);
        if instructions <= current && current != 0 {
            self.current = (current - instructions) as u32;
            if self.current != 0 {
                return;
            }
        } else {
            // Past zero: the reload value is taken, and the count goes on from it.
            let period = u64::from(self.reload) + 1;
            let past = instructions.saturating_sub(current) % period;
            self.current = (u64::from(self.reload) - past.min(u64::from(self.reload))) as u32;
        }
        self.control |= COUNTFLAG;
        if self.control & TICKINT != 0 {
            self.pending = true;
        }
    }
}

struct Registers(Rc<RefCell<SysTick>>);

impl Device for Registers {
    fn read(&mut self, address: u64, buf: &mut [u8]) {
        let mut timer = self.0.borrow_mut();
        let word = match address & !3 {
            CONTROL => {
                // Reading the register clears the flag it shows.
                let control = timer.control;
                timer.control &= !COUNTFLAG;
                control
            }
            RELOAD => timer.reload,
            CURRENT => timer.current,
            _ => 0,
        };
        let bytes = word.to_le_bytes();
        for (n, byte) in buf.iter_mut().enumerate() {
            *byte = bytes.get((address as usize & 3) + n).copied().unwrap_or(0);
        }
    }

    fn write(&mut self, address: u64, bytes: &[u8]) {
        let mut word = [0u8; 4];
        for (n, &byte) in bytes.iter().take(4).enumerate() {
            word[n] = byte;
        }
        let word = u32::from_le_bytes(word);
        let mut timer = self.0.borrow_mut();
        match address & !3 {
            CONTROL => timer.control = (timer.control & COUNTFLAG) | (word & 7),
            RELOAD => timer.reload = word & 0x00ff_ffff,
            // Any write clears the count and the flag.
            CURRENT => {
                timer.current = 0;
                timer.control &= !COUNTFLAG;
            }
            _ => {}
        }
    }
}

/// The core a firmware's run has.
pub(super) struct Core {
    timer: Rc<RefCell<SysTick>>,
    /// The timer's handler, as the vector table names it.
    handler: Option<u64>,
    /// The instructions the timer has counted.
    counted: u64,
    /// A handler is running: the exception is not taken again inside itself.
    handling: bool,
}

impl Core {
    /// The core of an image that is a Cortex-M firmware: its registers answer in the System
    /// Control Space, and the run looks at the timer every few instructions.
    pub(super) fn attach(image: &Image, vm: &mut Vm<'_>) -> Option<Self> {
        let vectors = image.cortex_m_vectors()?;
        let timer = Rc::new(RefCell::new(SysTick::default()));
        vm.memory.add_device(SCS.0, SCS.1, Box::new(Registers(timer.clone())));
        vm.poll = POLL;
        let entry = image.image_base() + 4 * SYSTICK;
        let handler = vectors.iter().find(|(at, _)| *at == entry).map(|&(_, handler)| handler);
        Some(Self { timer, handler, counted: vm.instructions, handling: false })
    }
}

impl super::host::Runtime<'_> {
    /// Between two instructions of a firmware: the timer counts the instructions run since, and
    /// its exception, when it is raised, is taken.
    pub(super) fn cortexm_poll(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let Some(core) = &mut self.cortexm else { return Ok(()) };
        let state = vm.poll_argument.take();
        let ran = vm.instructions - core.counted;
        core.counted = vm.instructions;
        let mut timer = core.timer.borrow_mut();
        timer.advance(ran);
        let (Some(state), Some(handler), true, false) = (state, core.handler, timer.pending, core.handling) else { return Ok(()) };
        timer.pending = false;
        drop(timer);
        let Some(name) = vm.name_at(handler).map(str::to_owned) else {
            return Err(Stop::fault(format!("the timer's vector names {handler:#x}, which is no lifted function")));
        };
        // The registers of the interrupted code are kept whole and put back: what the processor
        // stacks, and what the handler keeps as any function does.
        let size = crate::state::Isa::Arm32.spec().size();
        let kept = vm.memory.read_vec(state, size)?;
        core.handling = true;
        let result = vm.call_named(self, &name, vec![Value::Int(u128::from(state))]);
        if let Some(core) = &mut self.cortexm {
            core.handling = false;
            // The instructions of the handler are not the timer's to count twice.
            core.counted = vm.instructions;
        }
        vm.memory.write(state, &kept)?;
        result.map(|_| ())
    }
}
