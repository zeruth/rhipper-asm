//! The R3000 itself, for the code of a `PlayStation` program the lift has no function for.
//!
//! The lift covers the code the analysis finds in the image. A console's program has more: code
//! it copies into RAM and runs there (a kernel's vectors and tables, a shell a ROM unpacks), code
//! it runs through another segment of the address space, and the few things the processor does
//! that no function of IR says (a branch in a delay slot, whose slot is the first branch's
//! target). For all of that the run has the processor: this module runs the instructions at an
//! address straight from the program's memory, over the same `%State` the lifted functions take,
//! as a call of a function is run: from its entry to the return that leaves it.
//!
//! The two kinds of code call each other. A jump or a call that lands on the entry of a lifted
//! function runs that function and comes back where it returns to; a lifted function's call to
//! an address the lift has nothing for comes here. A program whose kernel is answered reaches it
//! through the `A0`, `B0` and `C0` entries here as it does in lifted code.
//!
//! A ROM is the kernel itself, and its run takes the processor's exceptions as the processor
//! does ([`Runtime::psx_exception`]): the cause and the return address in coprocessor 0, the
//! status register's mode pushed, and the code at the vector run until it jumps back to where
//! the exception was taken. Its interrupts are the level of the console's interrupt line, looked
//! at between two instructions.

use ra_asm_ir::Type;
use ra_asm_ir::interp::memory::UNMAPPED;
use ra_asm_ir::interp::{Stop, Symbol, Value, Vm};

use super::host::Runtime;
use crate::raise::prototypes::{PSX_SETJMP, psx_bios, psx_syscall};
use crate::state::{MIPS_HI, MIPS_LO, N64_COP0, PSX_GP, PSX_GTE};

/// What ends the run of an exception's handler from inside a lifted function it called: the
/// jump back to where the exception was taken, as a fault the exception's own frame reads.
pub(super) const RETURNED: &str = "the exception's handler returned";
/// What ends every frame of the run down to its first when a handler leaves an exception for
/// somewhere else than the code it was taken in (another thread, a handler of the program's
/// own): the processor goes on there, as a fault the first frame reads.
pub(super) const ELSEWHERE: &str = "the processor goes on elsewhere";
/// The instructions between two looks at the machine.
const POLL: u64 = 64;
/// Where a kernel enters the shell it copied into RAM.
const SHELL: u32 = 0x8003_0000;
/// Coprocessor 0's registers the processor writes: the status, the cause, the return address.
const SR: u64 = 12;
const CAUSE: u64 = 13;
const EPC: u64 = 14;
/// The status register's bit that cuts the data cache off from the memory.
const ISOLATED: u32 = 1 << 16;
/// Coprocessor 0's register of the address an address error was taken for.
const BAD_ADDRESS: u64 = 8;
/// The exception codes the processor raises here.
const INTERRUPT: u32 = 0;
const ADDRESS_LOAD: u32 = 4;
const ADDRESS_STORE: u32 = 5;
const BUS_CODE: u32 = 6;
const BUS_DATA: u32 = 7;
const SYSCALL: u32 = 8;
const BREAK: u32 = 9;
const RESERVED: u32 = 10;
const OVERFLOW: u32 = 12;

/// What a run keeps of the processor beside the `%State`.
pub(super) struct Processor {
    /// The program is the console's own kernel: exceptions are taken, nothing is answered.
    pub(super) kernel: bool,
    /// The exceptions being handled, innermost last: the addresses each one's handler returns
    /// to when the code the exception was taken in goes on (the instruction it was taken at,
    /// the one after it, or both, by what that code can go on from).
    exceptions: Vec<[Option<u32>; 2]>,
    /// Where the handler that last returned through a lifted function went.
    returned: u32,
    /// The program a ROM is beside was entered in the shell's stead.
    booted: bool,
    /// For each run of code the program copies into RAM ([`crate::Resident`]): the watch of its
    /// range, and the functions whose bytes the RAM was found to hold since the last write there.
    resident: Vec<(usize, std::collections::HashSet<u64>)>,
    /// The calls and jumps of lifted code through the run that are open, innermost last: each
    /// one's target and the block it was made from.
    nest: Vec<(u32, u32)>,
    /// Whether the last leave for elsewhere was a return from an exception to another address
    /// (a switch of threads), and not a jump buffer's.
    left_by_rfe: bool,
    /// The RAM as it was when the data cache was cut off, while it is: the stores made then
    /// flush cache lines and reach no memory, so the RAM comes back when the cache does.
    isolated: Option<Vec<u8>>,
    /// Where coprocessor 0's registers, the global pointer and the GTE's registers are.
    cop0: u64,
    gp: Option<u64>,
    gte: u64,
}

impl Processor {
    pub(super) fn new(vm: &mut Vm<'_>, kernel: bool) -> Option<Self> {
        let cop0 = vm.symbol(N64_COP0)?;
        // The processor's identity, as a program reads it.
        vm.memory.write_u64(cop0 + 15 * 8, 2).ok()?;
        let gte = vm.symbol(PSX_GTE)?;
        Some(Self {
            kernel,
            exceptions: Vec::new(),
            returned: 0,
            booted: false,
            resident: Vec::new(),
            nest: Vec::new(),
            left_by_rfe: false,
            isolated: None,
            cop0,
            gp: vm.symbol(PSX_GP),
            gte,
        })
    }
}

/// Where the processor's instructions go, when `RA_RUN_STATS` is set: the instructions it ran
/// from each address a run of it started at, which are the functions the lift has none for.
pub mod stats {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static RUNS: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("RA_RUN_STATS").is_some())
    }

    pub(super) fn add(entry: u32, instructions: u64) {
        if on()
            && let Ok(mut runs) = RUNS.lock()
        {
            *runs.get_or_insert_with(HashMap::new).entry(entry).or_default() += instructions;
        }
    }

    static TRAIL: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// With `RA_PROCESSOR_TRAIL` set, keeps the last crossings between lifted code and the
    /// processor and the exceptions taken, which a reserved instruction's exception prints
    /// while the platform is traced: how a run came to code that is none.
    pub(super) fn trail(what: impl FnOnce() -> String) {
        static ON: OnceLock<bool> = OnceLock::new();
        if *ON.get_or_init(|| std::env::var_os("RA_PROCESSOR_TRAIL").is_some())
            && let Ok(mut trail) = TRAIL.lock()
        {
            if trail.len() >= 4000 {
                trail.drain(..2000);
            }
            trail.push(what());
        }
    }

    pub(super) fn dump_trail(last: usize) {
        if let Ok(trail) = TRAIL.lock() {
            for line in trail.iter().skip(trail.len().saturating_sub(last)) {
                eprintln!("trail: {line}");
            }
        }
    }

    static SAMPLES: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

    /// The processor stood at `pc` when the run looked at its machine: a sample of where its
    /// instructions go. The count starts over at each million, so the report is of the last
    /// stretch of the run.
    pub(super) fn sample(pc: u32) {
        if on()
            && let Ok(mut samples) = SAMPLES.lock()
        {
            let samples = samples.get_or_insert_with(HashMap::new);
            if samples.values().sum::<u64>() >= 200_000 {
                samples.clear();
            }
            *samples.entry(pc).or_default() += 1;
        }
    }

    /// Where the processor stood the most in the last stretch of the run, most first.
    pub fn samples(most: usize) -> Vec<(u32, u64)> {
        let mut entries: Vec<(u32, u64)> = SAMPLES.lock().ok().and_then(|e| e.clone()).unwrap_or_default().into_iter().collect();
        entries.sort_by_key(|&(pc, count)| (std::cmp::Reverse(count), pc));
        entries.truncate(most);
        entries
    }

    static CALLED: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

    /// A call came to `target`, which no lifted function is: the entry of a function the lift
    /// has still to find.
    pub(super) fn call(target: u32) {
        if on()
            && let Ok(mut called) = CALLED.lock()
        {
            *called.get_or_insert_with(HashMap::new).entry(target).or_default() += 1;
        }
    }

    /// Every address a call came to that no lifted function was, with how often, by address.
    pub fn called() -> Vec<(u32, u64)> {
        let mut entries: Vec<(u32, u64)> = CALLED.lock().ok().and_then(|e| e.clone()).unwrap_or_default().into_iter().collect();
        entries.sort_unstable();
        entries
    }

    static CONTINUED: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

    /// A jump through a register came into the middle of a lifted function at `target`: where
    /// a context the program switched away from goes on, which the next lift makes an entry.
    pub(super) fn continuation(target: u32) {
        if on()
            && let Ok(mut continued) = CONTINUED.lock()
        {
            *continued.get_or_insert_with(HashMap::new).entry(target).or_default() += 1;
        }
    }

    /// Every address such a jump came to, with how often, by address.
    pub fn continued() -> Vec<(u32, u64)> {
        let mut entries: Vec<(u32, u64)> = CONTINUED.lock().ok().and_then(|e| e.clone()).unwrap_or_default().into_iter().collect();
        entries.sort_unstable();
        entries
    }

    static ENTERED: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

    /// A call or a jump of the processor's came to `target`, which no lifted function is.
    pub(super) fn enter(target: u32) {
        if on()
            && let Ok(mut entered) = ENTERED.lock()
        {
            *entered.get_or_insert_with(HashMap::new).entry(target).or_default() += 1;
        }
    }

    /// The addresses the processor's calls and jumps came to the most, most first.
    pub fn entered(most: usize) -> Vec<(u32, u64)> {
        let mut entries: Vec<(u32, u64)> = ENTERED.lock().ok().and_then(|e| e.clone()).unwrap_or_default().into_iter().collect();
        entries.sort_by_key(|&(entry, count)| (std::cmp::Reverse(count), entry));
        entries.truncate(most);
        entries
    }

    /// The processor's instructions in all, and the entries it ran the most from, most first.
    pub fn report(most: usize) -> (u64, Vec<(u32, u64)>) {
        let runs = RUNS.lock().ok().and_then(|runs| runs.clone()).unwrap_or_default();
        let total = runs.values().sum();
        let mut entries: Vec<(u32, u64)> = runs.into_iter().collect();
        entries.sort_by_key(|&(entry, count)| (std::cmp::Reverse(count), entry));
        entries.truncate(most);
        (total, entries)
    }
}

/// What a run of the processor is the run of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A function, from its entry to its return.
    Function,
    /// The handler of the exception of this index, to its return to the exception's address.
    Handler(usize),
    /// The run's first frame, which the program never returns from: where it starts, and
    /// where the processor goes on in the middle of it.
    Bottom,
}

/// How control came to an address: what the instruction before the delay slot was.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrival {
    /// `j`.
    Jump,
    /// `jal`, `jalr` and the linking branches.
    Call,
    /// `jr` through this register.
    Register(usize),
    /// `jr` with `rfe` in its slot: the return of an exception's handler.
    Exception,
}

struct Cpu {
    r: [u32; 32],
    hi: u32,
    lo: u32,
    /// The instruction to run next, and the one after it: a branch sets the second, so its
    /// delay slot runs first.
    pc: u32,
    next: u32,
    /// The load the last instruction made, which lands after the next one.
    load: Option<(usize, u32)>,
    /// The load in flight while an instruction runs: a write of its register cancels it.
    pending: Option<(usize, u32)>,
    /// The instruction that ran was a branch or a jump: the next is in its delay slot.
    branch: bool,
    /// An exception was taken and came back: `pc` is where it returned to.
    resumed: bool,
    /// The instruction that ran was `rfe`.
    rfe: bool,
}

impl Cpu {
    fn set(&mut self, n: usize, value: u32) {
        if n != 0 {
            self.r[n] = value;
        }
        if self.pending.is_some_and(|(register, _)| register == n) {
            self.pending = None;
        }
    }

    /// A load's result, which the register holds one instruction later.
    fn delay(&mut self, n: usize, value: u32) {
        if self.pending.is_some_and(|(register, _)| register == n) {
            self.pending = None;
        }
        if n != 0 {
            self.load = Some((n, value));
        }
    }

    /// The register as `lwl` and `lwr` read it: the load in flight when it is this register's.
    fn merged(&self, n: usize) -> u32 {
        match self.pending {
            Some((register, value)) if register == n => value,
            _ => self.r[n],
        }
    }

    fn branch_to(&mut self, taken: bool, word: u32) {
        self.branch = true;
        if taken {
            self.next = self.pc.wrapping_add((i32::from(word as u16 as i16) << 2).cast_unsigned());
        }
    }
}

/// Whether `word` is a branch or a jump: an instruction with a delay slot.
fn is_branch(word: u32) -> bool {
    match word >> 26 {
        0 => matches!(word & 0x3f, 8 | 9),
        1..=7 => true,
        _ => false,
    }
}

fn sext(word: u32) -> u64 {
    i64::from(word.cast_signed()).cast_unsigned()
}

/// The instruction word at `pc`: from the RAM where the run mapped it, whatever segment the
/// address names it through, or from the memory as a load reads it.
fn fetch(vm: &mut Vm<'_>, pc: u32) -> Result<u32, Stop> {
    let physical = pc & 0x1fff_ffff;
    if pc & 3 == 0 && physical < 0x80_0000 && matches!(pc >> 29, 0 | 4 | 5) {
        let at = super::PSX_RAM + u64::from(physical & 0x1f_ffff);
        // SAFETY: the run mapped and committed the console's RAM at `PSX_RAM`, and the address
        // is a word inside it.
        return Ok(unsafe { std::ptr::read_unaligned(at as usize as *const u32) });
    }
    vm.memory.read_u32(u64::from(pc))
}

impl Runtime<'_> {
    /// Whether the run has the processor for the code the lift has no function for.
    pub(super) fn r3000(&self) -> bool {
        self.machine.as_ref().is_some_and(|machine| machine.r3000.is_some())
    }

    fn processor(&mut self) -> Result<&mut Processor, Stop> {
        self.machine.as_mut().and_then(|machine| machine.r3000.as_mut()).ok_or_else(|| Stop::fault("the run has no R3000"))
    }

    /// Whether a jump to `target` is the return of the exception being handled.
    fn returns_exception(&self, target: u32) -> bool {
        self.machine
            .as_ref()
            .and_then(|machine| machine.r3000.as_ref())
            .and_then(|processor| processor.exceptions.last())
            .is_some_and(|returns| returns.contains(&Some(target)))
    }

    /// The lifted function the code at `target` is, by its address in the module: the address
    /// folded to the one the code was lifted at (RAM at `kseg0`, the ROM at `kseg1`, whatever
    /// segment or mirror `target` names it through). Code the program copied into RAM is its
    /// lifted function only while the RAM holds the bytes it was lifted from; other code there
    /// is the processor's.
    pub(super) fn psx_lifted(&mut self, vm: &mut Vm<'_>, target: u64) -> Result<Option<u64>, Stop> {
        let physical = target & 0x1fff_ffff;
        let at = match target >> 29 {
            0 | 4 | 5 if physical < 0x80_0000 => 0x8000_0000 | (physical & 0x1f_ffff),
            0 | 4 | 5 if (0x1fc0_0000..0x1fc8_0000).contains(&physical) => 0xa000_0000 | physical,
            _ => target,
        };
        if !matches!(vm.symbol_at(at), Some(Symbol::Function(_))) {
            return Ok(None);
        }
        let lifted = self.lifted;
        let Some(index) = lifted.resident.iter().position(|part| at.wrapping_sub(part.start) < part.bytes.len() as u64) else {
            return Ok(Some(at));
        };
        let Ok(processor) = self.processor() else { return Ok(Some(at)) };
        while processor.resident.len() < lifted.resident.len() {
            let (start, len) = lifted.resident[processor.resident.len()].code();
            processor.resident.push((vm.memory.watch(start, len), std::collections::HashSet::new()));
        }
        let (watch, held) = &mut processor.resident[index];
        if vm.memory.take_written(*watch) {
            held.clear();
        }
        if held.contains(&at) {
            return Ok(Some(at));
        }
        let part = &lifted.resident[index];
        for &(start, end) in part.functions.get(&at).map_or(&[][..], Vec::as_slice) {
            let from = (start - part.start) as usize;
            let expected = part.bytes.get(from..from + (end - start) as usize);
            if expected.is_none_or(|bytes| vm.memory.read_vec(start, end - start).is_ok_and(|now| now != bytes)) {
                return Ok(None);
            }
        }
        self.processor()?.resident[index].1.insert(at);
        Ok(Some(at))
    }

    /// The run of a kernel from its first frame: the lifted function `name`, or the code at
    /// `entry`, and then the processor wherever a handler left an exception for.
    pub(super) fn psx_boot(&mut self, vm: &mut Vm<'_>, state: u64, name: Option<&str>, entry: u32) -> Result<(), Stop> {
        let mut result = match name {
            Some(name) => vm.call_named(self, name, vec![Value::Int(u128::from(state))]).map(|_| ()),
            None => self.r3000_run(vm, state, entry, Frame::Bottom).map(|_| ()),
        };
        loop {
            match result {
                Err(Stop::Fault(message)) if message.starts_with(ELSEWHERE) => {
                    let processor = self.processor()?;
                    processor.exceptions.clear();
                    let target = processor.returned;
                    stats::trail(|| format!("elsewhere: the processor goes on at {target:#x}"));
                    result = match self.psx_lifted(vm, u64::from(target))? {
                        // The run has a function for where it goes on: that function, and
                        // when it returns, the processor from where it returned to.
                        Some(at) => {
                            let name = vm.name_at(at).unwrap_or_default().to_owned();
                            vm.call_named(self, &name, vec![Value::Int(u128::from(state))]).and_then(|_| {
                                let link = vm.memory.read_u64(state + 8 * 31)? as u32;
                                self.r3000_run(vm, state, link, Frame::Bottom).map(|_| ())
                            })
                        }
                        None => self.r3000_run(vm, state, target, Frame::Bottom).map(|_| ()),
                    };
                }
                // Lifted code reached memory nothing answers at: the processor's bus error at
                // the instruction, whose handler says where the processor goes on.
                Err(Stop::Fault(message)) if message.contains(UNMAPPED) && !message.ends_with(')') && vm.site.is_some() => {
                    let site = vm.site.unwrap_or(0) as u32;
                    let slot = fetch(vm, site.wrapping_sub(4)).is_ok_and(is_branch);
                    let address = if slot { site.wrapping_sub(4) } else { site };
                    result = self.psx_exception(vm, state, BUS_DATA, address, slot, [None, None]).map(|_| ());
                }
                other => return other,
            }
        }
    }

    /// A lifted function's call or jump to `target`, which no lifted function is: the code there
    /// run as the function it is.
    pub(super) fn r3000_call(&mut self, vm: &mut Vm<'_>, state: u64, target: u64) -> Result<(), Stop> {
        let target = target as u32;
        if vm.trace {
            eprintln!("> the processor at {target:#x}");
        }
        let link = vm.memory.read_u64(state + 8 * 31)? as u32;
        let returned = self.r3000_run(vm, state, target, Frame::Function)?;
        // A function of a kernel returns where it was called from, or leaves through a jump
        // buffer for where the buffer was filled: the lifted frames up to there are left as
        // they stand, and the processor goes on from the run's first frame. (Code of the image
        // itself is the rest of a lifted function, which returns where that function does.)
        let physical = u64::from(target) & 0x1fff_ffff;
        let image = (self.image.image_base() & 0x1fff_ffff, u64::from(self.image.size_of_image()));
        let resident = self.lifted.resident.iter().any(|part| physical.wrapping_sub(part.start & 0x1fff_ffff) < part.bytes.len() as u64);
        let outside = physical.wrapping_sub(image.0) >= image.1 && !resident;
        stats::trail(|| format!("lifted -> processor {target:#x} link {link:#x} returned {returned:#x} outside {outside}"));
        let processor = self.processor()?;
        if processor.kernel && outside && returned != link {
            processor.returned = returned;
            processor.left_by_rfe = false;
            return Err(Stop::fault(ELSEWHERE));
        }
        Ok(())
    }

    /// A lifted function's jump through a register to `target` (`ra_psx_jump`): into the middle
    /// of a function the run has lifted from RAM it is no call, and the processor goes on there
    /// from the run's first frame; anywhere else the jump is made as a call is.
    pub(super) fn psx_jump(&mut self, target: u64) -> Result<(), Stop> {
        let physical = target & 0x1fff_ffff;
        if !matches!(target >> 29, 0 | 4 | 5) || physical >= 0x80_0000 {
            return Ok(());
        }
        let at = 0x8000_0000 | (physical & 0x1f_ffff);
        let inside = self.lifted.resident.iter().any(|part| {
            !part.functions.contains_key(&at) && part.functions.values().flatten().any(|&(start, end)| (start..end).contains(&at))
        });
        // Where an earlier run saw such a jump land, the lift has an entry: the jump is still
        // no call, and the function there is entered from the run's first frame.
        let continues = self.lifted.resident.iter().any(|part| part.continuations.contains(&at));
        if !inside && !continues {
            return Ok(());
        }
        if inside {
            stats::continuation(at as u32);
        }
        stats::trail(|| format!("a lifted jump into the middle of a function, at {target:#x}"));
        let processor = self.processor()?;
        (processor.returned, processor.left_by_rfe) = (target as u32, false);
        Err(Stop::fault(ELSEWHERE))
    }

    /// A lifted function's call or jump through the run to `target` opens: how many were open
    /// before it. A program does not nest its calls without end, so past a depth no program
    /// reaches the run stops, with the targets and the sites of the last of them: where
    /// lifted code that leaves by a jump is taken for code that calls.
    pub(super) fn psx_nest(&mut self, vm: &Vm<'_>, target: u32) -> Result<usize, Stop> {
        const DEEPEST: usize = 3000;
        let site = vm.site.unwrap_or(0) as u32;
        let Ok(processor) = self.processor() else { return Ok(0) };
        let depth = processor.nest.len();
        processor.nest.push((target, site));
        if depth >= DEEPEST {
            let last: Vec<String> =
                processor.nest.iter().rev().take(12).map(|(target, site)| format!("{target:#x} from {site:#x}")).collect();
            return Err(Stop::fault(format!("lifted code is {depth} jumps through the run deep, the last to {}", last.join(", "))));
        }
        Ok(depth)
    }

    /// The call or jump that opened at `depth` is over, with every one it opened.
    pub(super) fn psx_unnest(&mut self, depth: usize) {
        if let Ok(processor) = self.processor() {
            processor.nest.truncate(depth);
        }
    }

    /// A lifted function of a kernel's run came back from a call whose return address was
    /// `link`. A function returns through `ra`, so `ra` is where it returned to: any other
    /// address than `link` is a jump buffer's, where the buffer was filled, and the processor
    /// goes on there from the run's first frame, the lifted frames left as they stand.
    pub(super) fn psx_returned(&mut self, vm: &mut Vm<'_>, state: u64, link: u32) -> Result<(), Stop> {
        let Ok(processor) = self.processor() else { return Ok(()) };
        if !processor.kernel {
            return Ok(());
        }
        let returned = vm.memory.read_u64(state + 8 * 31)? as u32;
        if returned == link {
            return Ok(());
        }
        stats::trail(|| format!("a lifted function called for {link:#x} returned to {returned:#x}"));
        let processor = self.processor()?;
        (processor.returned, processor.left_by_rfe) = (returned, false);
        Err(Stop::fault(ELSEWHERE))
    }

    /// A lifted handler's return from its exception to `target` (`ra_psx_return`): to the code
    /// the exception was taken in, whose frame reads the address, or to somewhere else, where
    /// the processor goes on from the run's first frame.
    pub(super) fn psx_returns(&mut self, target: u32) -> Result<(), Stop> {
        let within = self.returns_exception(target);
        let processor = self.processor()?;
        (processor.returned, processor.left_by_rfe) = (target, true);
        stats::trail(|| format!("lifted rfe to {target:#x}, within {within}"));
        Err(Stop::fault(if within { RETURNED } else { ELSEWHERE }))
    }

    /// Whether a lifted function's jump to `target` leaves the handler of the exception being
    /// handled; the address is kept for the exception's frame.
    pub(super) fn r3000_leaves(&mut self, target: u32) -> Result<bool, Stop> {
        if !self.returns_exception(target) {
            return Ok(false);
        }
        self.processor()?.returned = target;
        Ok(true)
    }

    fn r3000_load(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu) -> Result<(), Stop> {
        for n in 1..32 {
            cpu.r[n] = vm.memory.read_u64(state + 8 * n as u64)? as u32;
        }
        if let Some(gp) = self.processor()?.gp {
            cpu.r[28] = vm.memory.read_u64(gp)? as u32;
        }
        cpu.hi = vm.memory.read_u64(state + 8 * u64::from(MIPS_HI))? as u32;
        cpu.lo = vm.memory.read_u64(state + 8 * u64::from(MIPS_LO))? as u32;
        Ok(())
    }

    /// The registers back into the state, as the lifted functions keep them: a word sign-extended.
    /// A lifted function knows no load in flight, so the one there is lands first.
    fn r3000_flush(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu) -> Result<(), Stop> {
        if let Some((n, value)) = cpu.load.take() {
            cpu.r[n] = value;
        }
        for n in 1..32 {
            vm.memory.write_u64(state + 8 * n as u64, sext(cpu.r[n]))?;
        }
        if let Some(gp) = self.processor()?.gp {
            vm.memory.write_u64(gp, sext(cpu.r[28]))?;
        }
        vm.memory.write_u64(state + 8 * u64::from(MIPS_HI), sext(cpu.hi))?;
        vm.memory.write_u64(state + 8 * u64::from(MIPS_LO), sext(cpu.lo))
    }

    /// Runs the code at `entry` over the state at `state` until the function it starts returns:
    /// its `jr $ra` with every call it made returned from, or the end of a function it jumped
    /// to. An exception's handler also ends where it jumps back to the exception's address,
    /// and the run's first frame has no end. The result is the address control went to.
    fn r3000_run(&mut self, vm: &mut Vm<'_>, state: u64, entry: u32, frame: Frame) -> Result<u32, Stop> {
        let before = ra_asm_platform::profile::phase(2);
        let result = self.r3000_steps(vm, state, entry, frame);
        ra_asm_platform::profile::phase(before);
        result
    }

    fn r3000_steps(&mut self, vm: &mut Vm<'_>, state: u64, entry: u32, frame: Frame) -> Result<u32, Stop> {
        let mut cpu = Cpu {
            r: [0; 32],
            hi: 0,
            lo: 0,
            pc: entry,
            next: entry.wrapping_add(4),
            load: None,
            pending: None,
            branch: false,
            resumed: false,
            rfe: false,
        };
        self.r3000_load(vm, state, &mut cpu)?;
        // The calls open; how control came to `pc`, once the slot of the jump that brought it has
        // run; the jump whose slot runs now.
        let mut depth = 0u32;
        let mut arrived: Option<Arrival> = None;
        let mut jumped: Option<Arrival> = None;
        let mut slot = false;
        let mut count = 0u64;
        let kernel = self.processor()?.kernel;
        loop {
            if !slot {
                if let Some(arrival) = arrived.take()
                    && cpu.next == cpu.pc.wrapping_add(4)
                    && let Some(target) = self.r3000_arrive(vm, state, &mut cpu, arrival, &mut depth, frame)?
                {
                    vm.instructions += count;
                    vm.steps += count;
                    stats::add(entry, count);
                    self.r3000_flush(vm, state, &mut cpu)?;
                    return Ok(target);
                }
                let wrote = vm.memory.take_device_write();
                if count >= POLL || wrote {
                    if wrote && ra_asm_platform::tracing() {
                        eprintln!("psx: the processor before {:#x}, called from {:#x}, writes", cpu.pc, cpu.r[31].wrapping_sub(8));
                    }
                    vm.instructions += count;
                    vm.steps += count;
                    stats::add(entry, count);
                    stats::sample(cpu.pc);
                    count = 0;
                    // The look runs no code of the program: a load in flight stays in flight,
                    // and the next instruction reads its register as it was.
                    let in_flight = cpu.load.take();
                    self.r3000_flush(vm, state, &mut cpu)?;
                    vm.site = Some(u64::from(cpu.pc));
                    vm.poll_argument = Some(state);
                    self.console_poll(vm)?;
                    self.r3000_load(vm, state, &mut cpu)?;
                    cpu.load = in_flight;
                }
                // The processor looks at its interrupts before each instruction.
                if vm.boundary && self.psx_interrupt_due(vm)? {
                    self.r3000_flush(vm, state, &mut cpu)?;
                    let target = self.psx_exception(vm, state, INTERRUPT, cpu.pc, false, [Some(cpu.pc), Some(cpu.pc)])?;
                    self.r3000_load(vm, state, &mut cpu)?;
                    (cpu.pc, cpu.next) = (target, target.wrapping_add(4));
                }
            }
            let current = cpu.pc;
            if kernel && matches!(current & 0x1fff_ffff, 0xa0 | 0xb0) {
                self.psx_tty(vm, &cpu, current & 0xff)?;
            }
            let word = match fetch(vm, current) {
                Ok(word) if current & 3 == 0 || !kernel => word,
                // Over a kernel, an address that holds no instruction is the processor's
                // exception: an address error for one that is no word's, a bus error else.
                Ok(_) | Err(Stop::Fault(_)) if kernel => {
                    let code = if current & 3 == 0 { BUS_CODE } else { ADDRESS_LOAD };
                    self.r3000_bad(vm, state, &mut cpu, code, current, current, false)?;
                    (slot, jumped, arrived) = (false, None, None);
                    continue;
                }
                other => other?,
            };
            cpu.pc = cpu.next;
            cpu.next = cpu.next.wrapping_add(4);
            cpu.pending = cpu.load.take();
            cpu.branch = false;
            cpu.resumed = false;
            cpu.rfe = false;
            count += 1;
            let jump = self.r3000_exec(vm, state, &mut cpu, word, current, slot).map_err(|stop| match stop {
                Stop::Fault(message) if !message.starts_with(RETURNED) && !message.starts_with(ELSEWHERE) && !message.ends_with(')') => {
                    Stop::Fault(format!("{message} at {current:#x} (the processor)"))
                }
                other => other,
            })?;
            if let Some((n, value)) = cpu.pending.take() {
                cpu.r[n] = value;
            }
            if cpu.resumed {
                // The exception's handler put control where it is: no slot, no jump in flight.
                (slot, jumped, arrived) = (false, None, None);
                continue;
            }
            if slot {
                arrived = match jumped.take() {
                    Some(Arrival::Register(_)) if cpu.rfe => Some(Arrival::Exception),
                    other => other,
                };
            }
            slot = cpu.branch;
            if jump.is_some() {
                jumped = jump;
            }
        }
    }

    /// Control came to `cpu.pc` by a jump: the return of a function, of an exception's handler,
    /// or the entry of a callee the run has otherwise (a lifted function, a function of the
    /// answered kernel), which runs here to its end. `Some` is where this run's function
    /// returned to.
    fn r3000_arrive(
        &mut self,
        vm: &mut Vm<'_>,
        state: u64,
        cpu: &mut Cpu,
        arrival: Arrival,
        depth: &mut u32,
        frame: Frame,
    ) -> Result<Option<u32>, Stop> {
        let target = cpu.pc;
        match arrival {
            // A function ends where it returns with every call it made returned from. A handler
            // does not end so: a kernel's may leave through a jump buffer whose `ra` is the
            // code that returns from the exception.
            Arrival::Register(31) => {
                if *depth == 0 && frame == Frame::Function {
                    return Ok(Some(target));
                }
                *depth = depth.saturating_sub(1);
                return Ok(None);
            }
            // A handler returns through a register of the kernel's, never through `ra`: to the
            // code its exception was taken in, which goes on, in this frame or in one below.
            Arrival::Register(_) | Arrival::Exception if self.returns_exception(target) => {
                let processor = self.processor()?;
                if frame == Frame::Handler(processor.exceptions.len() - 1) {
                    return Ok(Some(target));
                }
                processor.returned = target;
                self.r3000_flush(vm, state, cpu)?;
                return Err(Stop::fault(RETURNED));
            }
            // Or to somewhere else, where the processor goes on from the run's first frame:
            // the code the exception was taken in is left as it stood.
            Arrival::Exception if !self.processor()?.exceptions.is_empty() => {
                let processor = self.processor()?;
                (processor.returned, processor.left_by_rfe) = (target, true);
                self.r3000_flush(vm, state, cpu)?;
                return Err(Stop::fault(ELSEWHERE));
            }
            Arrival::Call => *depth += 1,
            Arrival::Register(_) | Arrival::Exception | Arrival::Jump => {}
        }
        if target == SHELL && self.image.rom_boots && !std::mem::replace(&mut self.processor()?.booted, true) {
            return self.r3000_program(vm, state, cpu);
        }
        // Where the callee returns to: its caller's link, which a tail jump leaves as it was.
        let link = cpu.r[31];
        let arrival_name = match arrival {
            Arrival::Call => "call",
            Arrival::Jump => "j",
            Arrival::Register(_) => "jr",
            Arrival::Exception => "rfe",
        };
        let entry = target & 0x1fff_ffff;
        let answered = !self.processor()?.kernel && matches!(entry, 0xa0 | 0xb0 | 0xc0);
        if answered {
            self.r3000_bios(vm, state, cpu, entry)?;
        } else if let Some(at) = self.psx_lifted(vm, u64::from(target))? {
            stats::trail(|| format!("processor -> lifted {at:#x} ({arrival_name}) ra {link:#x} sp {:#x} t1 {:#x}", cpu.r[29], cpu.r[9]));
            let name = vm.name_at(at).unwrap_or_default().to_owned();
            if !vm.function(&name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void) {
                return Ok(None);
            }
            self.r3000_flush(vm, state, cpu)?;
            let called = vm.call_named(self, &name, vec![Value::Int(u128::from(state))]);
            ra_asm_platform::profile::phase(2);
            called?;
            self.psx_returned(vm, state, link)?;
            self.r3000_load(vm, state, cpu)?;
        } else {
            stats::enter(target);
            if arrival == Arrival::Call {
                stats::call(target);
            }
            return Ok(None);
        }
        // The callee ran to its end. A call's goes on after it; a jump to it was the end of the
        // function that jumped.
        if arrival != Arrival::Call && *depth == 0 && frame == Frame::Function {
            return Ok(Some(link));
        }
        *depth = depth.saturating_sub(1);
        cpu.pc = link;
        cpu.next = link.wrapping_add(4);
        Ok(None)
    }

    /// A kernel's TTY is a serial port of a development board, which no console has: what a
    /// program prints is taken where it enters the kernel, at the table `entry` (`0xa0` or
    /// `0xb0`) with the number of `putchar`, `puts` or a `write` to the standard output in `t1`.
    /// The kernel's own function runs after as it would.
    fn psx_tty(&mut self, vm: &mut Vm<'_>, cpu: &Cpu, entry: u32) -> Result<(), Stop> {
        let [a0, a1, a2] = [cpu.r[4], cpu.r[5], cpu.r[6]].map(u64::from);
        let text = match (entry, cpu.r[9] & 0xff) {
            (0xa0, 0x3c) | (0xb0, 0x3d) => vec![a0 as u8],
            (0xa0, 0x3e) | (0xb0, 0x3f) => {
                let mut text = Vec::new();
                while text.len() < 1 << 16 {
                    match vm.memory.read_u8(a0 + text.len() as u64) {
                        Ok(0) | Err(_) => break,
                        Ok(byte) => text.push(byte),
                    }
                }
                text
            }
            (0xa0, 0x03) | (0xb0, 0x35) if a0 == 1 => vm.memory.read_vec(a1, a2.min(1 << 16)).unwrap_or_default(),
            _ => return Ok(()),
        };
        if let Some(machine) = &self.machine {
            machine.tty(&text);
        }
        Ok(())
    }

    /// The kernel is about to enter its shell, and the ROM is beside a program: the program is
    /// loaded and entered in the shell's stead, as the kernel's `Exec` starts one (its text in
    /// RAM, its BSS cleared, `gp` and the stack its header's), and its end is the run's.
    fn r3000_program(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu) -> Result<Option<u32>, Stop> {
        let image = self.image;
        let exe = image.psx.as_ref().ok_or_else(|| Stop::fault("the ROM is beside no PlayStation program"))?;
        vm.memory.write(image.image_base(), image.mapped())?;
        if let Some((at, len)) = exe.bss_range() {
            vm.memory.write(at, &vec![0; len as usize])?;
        }
        if exe.gp != 0 {
            cpu.r[28] = exe.gp;
        }
        if let Some(sp) = exe.initial_sp() {
            (cpu.r[29], cpu.r[30]) = (sp, sp);
        }
        (cpu.r[4], cpu.r[5], cpu.r[31]) = (0, 0, 0);
        let entry = exe.entry();
        self.r3000_flush(vm, state, cpu)?;
        match vm.name_at(entry).map(str::to_owned) {
            Some(name) if matches!(vm.symbol_at(entry), Some(Symbol::Function(_))) => {
                vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
            }
            _ => {
                self.r3000_run(vm, state, entry as u32, Frame::Function)?;
            }
        }
        Err(Stop::Exit(vm.memory.read_u32(state + 2 * 8)?.cast_signed()))
    }

    /// A function of the answered kernel, by the table at `entry` and the number in `t1`.
    fn r3000_bios(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu, entry: u32) -> Result<(), Stop> {
        let number = cpu.r[9];
        let Some(call) = psx_bios(u64::from(entry), u64::from(number)) else {
            return Err(Stop::fault(format!("a BIOS call through {entry:#x} with {number:#x} in t1, which the table does not name")));
        };
        self.r3000_flush(vm, state, cpu)?;
        let mut args: Vec<Value> = (0..usize::from(call.params)).map(|i| Value::Int(u128::from(sext(cpu.r[4 + i])))).collect();
        if call.name == PSX_SETJMP {
            args[1] = Value::Int(u128::from(state));
        }
        let result = self.console_call(vm, call.name, &args)?;
        self.r3000_load(vm, state, cpu)?;
        if call.returns {
            cpu.r[2] = result.as_u64() as u32;
        }
        Ok(())
    }

    /// The exception `code` at the instruction at `current`, from the code this module runs: a
    /// kernel takes it at its vector and control goes where its handler returns; a program whose
    /// kernel is answered has only the `syscall` the kernel answers.
    fn r3000_raise(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu, code: u32, current: u32, in_slot: bool) -> Result<(), Stop> {
        if !self.processor()?.kernel {
            if code == SYSCALL {
                let call = psx_syscall();
                self.r3000_flush(vm, state, cpu)?;
                let result = self.console_call(vm, call.name, &[Value::Int(u128::from(sext(cpu.r[4])))])?;
                self.r3000_load(vm, state, cpu)?;
                cpu.r[2] = result.as_u64() as u32;
                return Ok(());
            }
            return Err(Stop::fault(match code {
                BREAK => "int3",
                OVERFLOW => "an arithmetic overflow",
                _ => "bytes that are no instruction",
            }));
        }
        self.r3000_flush(vm, state, cpu)?;
        // An exception in a delay slot is the branch's: the handler returns to the branch.
        let address = if in_slot { current.wrapping_sub(4) } else { current };
        let target = self.psx_exception(vm, state, code, address, in_slot, [Some(address), Some(address.wrapping_add(4))])?;
        self.r3000_load(vm, state, cpu)?;
        cpu.pc = target;
        cpu.next = target.wrapping_add(4);
        cpu.resumed = true;
        Ok(())
    }

    /// An access to `bad` the processor of a kernel's run does not make: the exception `code`,
    /// with the address in coprocessor 0's register for it when it is an address error.
    fn r3000_bad(
        &mut self,
        vm: &mut Vm<'_>,
        state: u64,
        cpu: &mut Cpu,
        code: u32,
        bad: u32,
        current: u32,
        in_slot: bool,
    ) -> Result<(), Stop> {
        let processor = self.processor()?;
        if code != BUS_DATA && code != BUS_CODE {
            vm.memory.write_u64(processor.cop0 + 8 * BAD_ADDRESS, sext(bad))?;
        }
        self.r3000_raise(vm, state, cpu, code, current, in_slot)
    }

    /// The processor takes the exception `code` at `address` (`slot` when the instruction is in
    /// a branch's delay slot and `address` is the branch): the cause and the return address into
    /// coprocessor 0, the status register's mode pushed, and the code at the vector run until it
    /// jumps back to one of `returns`, the addresses the code the exception was taken in goes on
    /// from. The result is the one it jumped back to; a handler that leaves for another address
    /// ends the frames of the run down to its first ([`ELSEWHERE`]).
    pub(super) fn psx_exception(
        &mut self,
        vm: &mut Vm<'_>,
        state: u64,
        code: u32,
        address: u32,
        slot: bool,
        returns: [Option<u32>; 2],
    ) -> Result<u32, Stop> {
        stats::trail(|| format!("exception {code} at {address:#x} slot {slot} returns {returns:x?}"));
        if ra_asm_platform::tracing() && !matches!(code, INTERRUPT | SYSCALL) {
            eprintln!("psx: the exception {code} at {address:#x}{}", if slot { ", in a delay slot" } else { "" });
            if matches!(code, RESERVED | BUS_CODE | BUS_DATA | ADDRESS_LOAD | ADDRESS_STORE) {
                stats::dump_trail(60);
            }
        }
        let cop0 = self.processor()?.cop0;
        let status = vm.memory.read_u64(cop0 + 8 * SR)? as u32;
        let cause = vm.memory.read_u64(cop0 + 8 * CAUSE)? as u32;
        vm.memory.write_u64(cop0 + 8 * CAUSE, sext((cause & 0xff00) | (code << 2) | (u32::from(slot) << 31)))?;
        vm.memory.write_u64(cop0 + 8 * EPC, sext(address))?;
        vm.memory.write_u64(cop0 + 8 * SR, sext((status & !0x3f) | ((status << 2) & 0x3f)))?;
        // The vectors of a kernel that has not set up the RAM's yet are the ROM's.
        let vector = if status & (1 << 22) == 0 { 0x8000_0080 } else { 0xbfc0_0180 };
        if ra_asm_platform::tracing() && code != INTERRUPT {
            eprintln!("psx: exception {code} at {address:#x}");
        }
        let processor = self.processor()?;
        let index = processor.exceptions.len();
        processor.exceptions.push(returns);
        let mut result = self.r3000_run(vm, state, vector, Frame::Handler(index));
        // A handler that leaves through a jump buffer (its kernel's way out of an interrupt) is
        // still this exception's handler where the buffer was filled: it goes on there, in this
        // frame, to its return, so the code the exception was taken in goes on as it stood.
        // One that returns from the exception to another address has left it for good.
        loop {
            let left = matches!(&result, Err(Stop::Fault(message)) if message.starts_with(ELSEWHERE));
            let processor = self.processor()?;
            if !left || processor.left_by_rfe || processor.exceptions.len() != index + 1 {
                break;
            }
            let target = processor.returned;
            result = self.r3000_run(vm, state, target, Frame::Handler(index));
        }
        let processor = self.processor()?;
        processor.exceptions.truncate(index);
        match result {
            Err(Stop::Fault(message)) if message.starts_with(RETURNED) => Ok(processor.returned),
            other => other,
        }
    }

    /// The exception `code` lifted code raised at `address`: `code`'s ninth bit says the
    /// instruction is in the delay slot of the branch at `address`. Lifted code goes on after
    /// the instruction alone, and after one in a slot not at all: a handler that returns to
    /// the instruction itself, or into a slot's branch, has the processor run it.
    pub(super) fn psx_raised(&mut self, vm: &mut Vm<'_>, state: u64, code: u32, address: u32) -> Result<(), Stop> {
        let slot = code & 0x100 != 0;
        let returns = if slot { [None, None] } else { [Some(address.wrapping_add(4)), None] };
        self.psx_exception(vm, state, code & 0x1f, address, slot, returns).map(|_| ())
    }

    /// The interrupt line's level into the cause register of a kernel's run, and whether the
    /// processor has an interrupt to look at before its instructions: the line's, or one of
    /// the two the program raises itself.
    pub(super) fn psx_line(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let Some(line) = self.machine.as_ref().map(super::console::Machine::interrupt) else { return Ok(()) };
        let cop0 = self.processor()?.cop0;
        let cause = vm.memory.read_u64(cop0 + 8 * CAUSE)? as u32;
        let level = if line { cause | 0x400 } else { cause & !0x400 };
        if level != cause {
            vm.memory.write_u64(cop0 + 8 * CAUSE, sext(level))?;
        }
        vm.boundary = level & 0xff00 != 0;
        Ok(())
    }

    /// Whether the program stands in a loop of nothing: at `vm.site`, or just before it, a
    /// branch or a jump to itself with an empty delay slot, which only an interrupt ends. The
    /// clock may move on to what the machine has next.
    pub(super) fn psx_idles(&self, vm: &mut Vm<'_>) -> bool {
        let Some(site) = vm.site.map(|site| site as u32) else { return false };
        // The RAM alone, which is read here without the read counting as the program's.
        if !self.r3000() || site & 0x1fff_ffff >= 0x80_0000 {
            return false;
        }
        [site, site.wrapping_sub(4)].into_iter().any(|at| {
            let (Ok(word), Ok(slot)) = (fetch(vm, at), fetch(vm, at.wrapping_add(4))) else { return false };
            let to_itself = match word >> 26 {
                2 => (at.wrapping_add(4) & 0xf000_0000) | ((word & 0x03ff_ffff) << 2) == at,
                4 => word & 0xffff == 0xffff && (word >> 21) & 31 == (word >> 16) & 31,
                _ => false,
            };
            to_itself && slot == 0
        })
    }

    /// Whether the status register lets an interrupt the cause register holds through.
    fn psx_interrupt_due(&mut self, vm: &mut Vm<'_>) -> Result<bool, Stop> {
        let cop0 = self.processor()?.cop0;
        let status = vm.memory.read_u64(cop0 + 8 * SR)? as u32;
        let cause = vm.memory.read_u64(cop0 + 8 * CAUSE)? as u32;
        Ok(status & 1 != 0 && status & cause & 0xff00 != 0)
    }

    /// Before the instruction at `to` of lifted code, after the one at `from`: the processor
    /// takes the interrupt it has, where the code can be left and come back to. Not between a
    /// branch and its slot, nor between the slot and what the branch does after it (its call,
    /// which the lift places there, and what follows the call's return), nor at a function's
    /// entry, which a handler's return to would read as a call of it.
    pub(super) fn psx_boundary(
        &mut self,
        vm: &mut Vm<'_>,
        from: Option<u64>,
        to: u64,
        start: bool,
        state: Option<u64>,
    ) -> Result<(), Stop> {
        let Some(state) = state else { return Ok(()) };
        if !self.r3000() || !self.psx_interrupt_due(vm)? || vm.symbol_at(to).is_some() {
            return Ok(());
        }
        let branch = |vm: &mut Vm<'_>, at: u64| fetch(vm, at as u32).map(is_branch);
        if let Some(from) = from
            && (to == from.wrapping_sub(4) || (to == from.wrapping_add(4) && branch(vm, from)?))
        {
            return Ok(());
        }
        if !start && from != Some(to.wrapping_sub(4)) && branch(vm, to)? {
            return Ok(());
        }
        self.psx_exception(vm, state, INTERRUPT, to as u32, false, [Some(to as u32), None]).map(|_| ())
    }

    /// The status register was written: when the write cuts the data cache off from the memory
    /// the RAM is kept as it is, and comes back as it was when the cache does.
    pub(super) fn psx_status(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let processor = self.processor()?;
        let status = vm.memory.read_u64(processor.cop0 + 8 * SR)? as u32;
        let ram = (super::PSX_RAM as usize as *mut u8, super::PSX_RAM_SIZE as usize);
        if status & ISOLATED != 0 {
            if processor.isolated.is_none() {
                // SAFETY: the run mapped and committed the console's RAM there, whole.
                processor.isolated = Some(unsafe { std::slice::from_raw_parts(ram.0, ram.1) }.to_vec());
            }
        } else if let Some(kept) = processor.isolated.take() {
            // SAFETY: as above; the kept bytes are the RAM's length.
            unsafe { std::ptr::copy_nonoverlapping(kept.as_ptr(), ram.0, ram.1) };
        }
        self.psx_line(vm)
    }

    fn gte_slot(&mut self, vm: &mut Vm<'_>, slot: u32) -> Result<u32, Stop> {
        let gte = self.processor()?.gte;
        vm.memory.read_u32(gte + 4 * u64::from(slot))
    }

    fn set_gte_slot(&mut self, vm: &mut Vm<'_>, slot: u32, value: u32) -> Result<(), Stop> {
        let gte = self.processor()?.gte;
        vm.memory.write_u32(gte + 4 * u64::from(slot), value)
    }

    /// Data register `n` of the GTE as `mfc2` reads it: as the lift's `gte_data`.
    fn gte_data(&mut self, vm: &mut Vm<'_>, n: u32) -> Result<u32, Stop> {
        match n {
            15 => self.gte_slot(vm, 14),
            28 | 29 => {
                let mut packed = 0;
                for k in 0..3 {
                    let ir = self.gte_slot(vm, 9 + k)?.cast_signed();
                    packed |= ((ir >> 7).clamp(0, 0x1f) as u32) << (5 * k);
                }
                Ok(packed)
            }
            31 => {
                let value = self.gte_slot(vm, 30)?.cast_signed();
                Ok((value ^ (value >> 31)).leading_zeros())
            }
            _ => self.gte_slot(vm, n),
        }
    }

    /// Writes data register `n` of the GTE as `mtc2` does: as the lift's `set_gte_data`.
    fn set_gte_data(&mut self, vm: &mut Vm<'_>, n: u32, value: u32) -> Result<(), Stop> {
        match n {
            1 | 3 | 5 | 8..=11 => self.set_gte_slot(vm, n, i32::from(value as u16 as i16).cast_unsigned()),
            7 | 16..=19 => self.set_gte_slot(vm, n, value & 0xffff),
            15 => {
                let (s1, s2) = (self.gte_slot(vm, 13)?, self.gte_slot(vm, 14)?);
                self.set_gte_slot(vm, 12, s1)?;
                self.set_gte_slot(vm, 13, s2)?;
                self.set_gte_slot(vm, 14, value)
            }
            28 => {
                for k in 0..3 {
                    self.set_gte_slot(vm, 9 + k, ((value >> (5 * k)) & 0x1f) << 7)?;
                }
                self.set_gte_slot(vm, 28, value)
            }
            29 | 31 => Ok(()),
            _ => self.set_gte_slot(vm, n, value),
        }
    }

    /// Writes control register `n` of the GTE as `ctc2` does: as the lift's `set_gte_control`.
    fn set_gte_control(&mut self, vm: &mut Vm<'_>, n: u32, value: u32) -> Result<(), Stop> {
        let value = match n {
            4 | 12 | 20 | 26 | 27 | 29 | 30 => i32::from(value as u16 as i16).cast_unsigned(),
            31 => (value & 0x7fff_f000) | if value & 0x7f87_e000 != 0 { 1 << 31 } else { 0 },
            _ => value,
        };
        self.set_gte_slot(vm, 32 + n, value)
    }

    /// One instruction: `word`, read at `current`, in a delay slot when `in_slot`. The result is
    /// the jump it made, when it is one whose target may be a callee's entry or a return.
    #[allow(clippy::too_many_lines)]
    fn r3000_exec(
        &mut self,
        vm: &mut Vm<'_>,
        state: u64,
        cpu: &mut Cpu,
        word: u32,
        current: u32,
        in_slot: bool,
    ) -> Result<Option<Arrival>, Stop> {
        let (op, rs, rt, rd) = (word >> 26, (word >> 21 & 31) as usize, (word >> 16 & 31) as usize, (word >> 11 & 31) as usize);
        let (shift, funct) = (word >> 6 & 31, word & 0x3f);
        let (s, t) = (cpu.r[rs], cpu.r[rt]);
        let simm = i32::from(word as u16 as i16).cast_unsigned();
        let address = s.wrapping_add(simm);
        let at = u64::from(address);
        // An access of the instruction to memory: over a kernel, one nothing answers is the
        // processor's bus error.
        macro_rules! bus {
            ($access:expr) => {
                match $access {
                    Ok(value) => value,
                    Err(Stop::Fault(message)) if message.contains(UNMAPPED) && self.processor()?.kernel => {
                        self.r3000_bad(vm, state, cpu, BUS_DATA, address, current, in_slot)?;
                        return Ok(None);
                    }
                    Err(stop) => return Err(stop),
                }
            };
        }
        match op {
            0 => match funct {
                0 => cpu.set(rd, t << shift),
                2 => cpu.set(rd, t >> shift),
                3 => cpu.set(rd, (t.cast_signed() >> shift).cast_unsigned()),
                4 => cpu.set(rd, t << (s & 31)),
                6 => cpu.set(rd, t >> (s & 31)),
                7 => cpu.set(rd, (t.cast_signed() >> (s & 31)).cast_unsigned()),
                8 => {
                    cpu.branch = true;
                    cpu.next = s;
                    return Ok(Some(Arrival::Register(rs)));
                }
                9 => {
                    cpu.branch = true;
                    let link = cpu.next;
                    cpu.next = s;
                    cpu.set(rd, link);
                    return Ok(Some(Arrival::Call));
                }
                12 => self.r3000_raise(vm, state, cpu, SYSCALL, current, in_slot)?,
                13 => self.r3000_raise(vm, state, cpu, BREAK, current, in_slot)?,
                16 => cpu.set(rd, cpu.hi),
                17 => cpu.hi = s,
                18 => cpu.set(rd, cpu.lo),
                19 => cpu.lo = s,
                24 => {
                    let product = i64::from(s.cast_signed()) * i64::from(t.cast_signed());
                    (cpu.hi, cpu.lo) = ((product >> 32) as u32, product as u32);
                }
                25 => {
                    let product = u64::from(s) * u64::from(t);
                    (cpu.hi, cpu.lo) = ((product >> 32) as u32, product as u32);
                }
                26 => {
                    let (n, d) = (s.cast_signed(), t.cast_signed());
                    (cpu.hi, cpu.lo) = if d == 0 {
                        (s, if n >= 0 { u32::MAX } else { 1 })
                    } else if n == i32::MIN && d == -1 {
                        (0, s)
                    } else {
                        ((n % d).cast_unsigned(), (n / d).cast_unsigned())
                    };
                }
                27 => (cpu.hi, cpu.lo) = if t == 0 { (s, u32::MAX) } else { (s % t, s / t) },
                32 => match s.cast_signed().checked_add(t.cast_signed()) {
                    Some(sum) => cpu.set(rd, sum.cast_unsigned()),
                    None => self.r3000_raise(vm, state, cpu, OVERFLOW, current, in_slot)?,
                },
                33 => cpu.set(rd, s.wrapping_add(t)),
                34 => match s.cast_signed().checked_sub(t.cast_signed()) {
                    Some(difference) => cpu.set(rd, difference.cast_unsigned()),
                    None => self.r3000_raise(vm, state, cpu, OVERFLOW, current, in_slot)?,
                },
                35 => cpu.set(rd, s.wrapping_sub(t)),
                36 => cpu.set(rd, s & t),
                37 => cpu.set(rd, s | t),
                38 => cpu.set(rd, s ^ t),
                39 => cpu.set(rd, !(s | t)),
                42 => cpu.set(rd, u32::from(s.cast_signed() < t.cast_signed())),
                43 => cpu.set(rd, u32::from(s < t)),
                _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
            },
            1 => {
                // Every `rt` is one of the four: bit 0 picks the test, `0b1000x` the link.
                let taken = (s.cast_signed() < 0) != (rt & 1 == 1);
                let links = rt & 0x1e == 0x10;
                if links {
                    cpu.set(31, cpu.next);
                }
                cpu.branch_to(taken, word);
                if links && taken {
                    return Ok(Some(Arrival::Call));
                }
            }
            2 | 3 => {
                cpu.branch = true;
                let target = (cpu.pc & 0xf000_0000) | ((word & 0x03ff_ffff) << 2);
                if op == 3 {
                    cpu.set(31, cpu.next);
                }
                cpu.next = target;
                return Ok(Some(if op == 3 { Arrival::Call } else { Arrival::Jump }));
            }
            4 => cpu.branch_to(s == t, word),
            5 => cpu.branch_to(s != t, word),
            6 => cpu.branch_to(s.cast_signed() <= 0, word),
            7 => cpu.branch_to(s.cast_signed() > 0, word),
            8 => match s.cast_signed().checked_add(simm.cast_signed()) {
                Some(sum) => cpu.set(rt, sum.cast_unsigned()),
                None => self.r3000_raise(vm, state, cpu, OVERFLOW, current, in_slot)?,
            },
            9 => cpu.set(rt, address),
            10 => cpu.set(rt, u32::from(s.cast_signed() < simm.cast_signed())),
            11 => cpu.set(rt, u32::from(s < simm)),
            12 => cpu.set(rt, s & (word & 0xffff)),
            13 => cpu.set(rt, s | (word & 0xffff)),
            14 => cpu.set(rt, s ^ (word & 0xffff)),
            15 => cpu.set(rt, word << 16),
            16 => {
                let cop0 = self.processor()?.cop0;
                let register = cop0 + 8 * rd as u64;
                match rs {
                    0 => {
                        let value = vm.memory.read_u64(register)? as u32;
                        cpu.delay(rt, value);
                    }
                    4 => {
                        // Of the cause, a program writes the two software interrupts alone.
                        let value = if rd as u64 == CAUSE { (vm.memory.read_u64(register)? as u32 & !0x300) | (t & 0x300) } else { t };
                        vm.memory.write_u64(register, sext(value))?;
                        match rd as u64 {
                            SR => self.psx_status(vm)?,
                            CAUSE => self.psx_line(vm)?,
                            _ => {}
                        }
                    }
                    // `rfe`: the status register's mode pops.
                    16..=31 if funct == 0x10 => {
                        let status = vm.memory.read_u64(cop0 + 8 * SR)? as u32;
                        vm.memory.write_u64(cop0 + 8 * SR, sext((status & !0xf) | ((status >> 2) & 0xf)))?;
                        cpu.rfe = true;
                    }
                    _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
                }
            }
            18 => match rs {
                0 => {
                    let value = self.gte_data(vm, rd as u32)?;
                    cpu.delay(rt, value);
                }
                2 => {
                    let value = self.gte_slot(vm, 32 + rd as u32)?;
                    cpu.delay(rt, value);
                }
                4 => self.set_gte_data(vm, rd as u32, t)?,
                6 => self.set_gte_control(vm, rd as u32, t)?,
                16..=31 => {
                    let gte = self.processor()?.gte;
                    super::console::gte(vm, &[Value::Int(u128::from(gte)), Value::Int(u128::from(word))])?;
                }
                _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
            },
            // A halfword or a word at an address that is no multiple of its size.
            33 | 35 | 37 | 50 | 41 | 43 | 58
                if address & (if matches!(op, 33 | 37 | 41) { 1 } else { 3 }) != 0 && self.processor()?.kernel =>
            {
                let code = if matches!(op, 41 | 43 | 58) { ADDRESS_STORE } else { ADDRESS_LOAD };
                self.r3000_bad(vm, state, cpu, code, address, current, in_slot)?;
            }
            32 => {
                let value = i32::from(bus!(vm.memory.read_u8(at)).cast_signed()).cast_unsigned();
                cpu.delay(rt, value);
            }
            33 => {
                let value = i32::from(bus!(vm.memory.read_u16(at)).cast_signed()).cast_unsigned();
                cpu.delay(rt, value);
            }
            34 | 38 => {
                let held = cpu.merged(rt);
                let memory = bus!(vm.memory.read_u32(at & !3));
                let value = match (op, address & 3) {
                    (34, 0) => (held & 0x00ff_ffff) | (memory << 24),
                    (34, 1) => (held & 0x0000_ffff) | (memory << 16),
                    (34, 2) => (held & 0x0000_00ff) | (memory << 8),
                    (38, 1) => (held & 0xff00_0000) | (memory >> 8),
                    (38, 2) => (held & 0xffff_0000) | (memory >> 16),
                    (38, 3) => (held & 0xffff_ff00) | (memory >> 24),
                    _ => memory,
                };
                cpu.delay(rt, value);
            }
            35 => {
                let value = bus!(vm.memory.read_u32(at));
                cpu.delay(rt, value);
            }
            36 => {
                let value = u32::from(bus!(vm.memory.read_u8(at)));
                cpu.delay(rt, value);
            }
            37 => {
                let value = u32::from(bus!(vm.memory.read_u16(at)));
                cpu.delay(rt, value);
            }
            40 => bus!(vm.memory.write_u8(at, t as u8)),
            41 => bus!(vm.memory.write_u16(at, t as u16)),
            42 | 46 => {
                let memory = bus!(vm.memory.read_u32(at & !3));
                let value = match (op, address & 3) {
                    (42, 0) => (memory & 0xffff_ff00) | (t >> 24),
                    (42, 1) => (memory & 0xffff_0000) | (t >> 16),
                    (42, 2) => (memory & 0xff00_0000) | (t >> 8),
                    (46, 1) => (memory & 0x0000_00ff) | (t << 8),
                    (46, 2) => (memory & 0x0000_ffff) | (t << 16),
                    (46, 3) => (memory & 0x00ff_ffff) | (t << 24),
                    _ => t,
                };
                bus!(vm.memory.write_u32(at & !3, value));
            }
            43 => bus!(vm.memory.write_u32(at, t)),
            50 => {
                let value = bus!(vm.memory.read_u32(at));
                self.set_gte_data(vm, rt as u32, value)?;
            }
            58 => {
                let value = self.gte_data(vm, rt as u32)?;
                bus!(vm.memory.write_u32(at, value));
            }
            _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
        }
        Ok(None)
    }
}
