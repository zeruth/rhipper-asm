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
use ra_asm_ir::interp::{Stop, Symbol, Value, Vm};

use super::host::Runtime;
use crate::raise::prototypes::{PSX_SETJMP, psx_bios, psx_syscall};
use crate::state::{Isa, MIPS_GTE, MIPS_HI, MIPS_LO, N64_COP0, PSX_GP};

/// What ends the run of an exception's handler from inside a lifted function it called: the
/// jump back to where the exception was taken, as a fault the exception's own frame reads.
pub(super) const RETURNED: &str = "the exception's handler returned";
/// The instructions between two looks at the machine.
const POLL: u64 = 64;
/// Coprocessor 0's registers the processor writes: the status, the cause, the return address.
const SR: u64 = 12;
const CAUSE: u64 = 13;
const EPC: u64 = 14;
/// The status register's bit that cuts the data cache off from the memory.
const ISOLATED: u32 = 1 << 16;
/// The exception codes the processor raises here.
const INTERRUPT: u32 = 0;
const SYSCALL: u32 = 8;
const BREAK: u32 = 9;
const RESERVED: u32 = 10;
const OVERFLOW: u32 = 12;

/// What a run keeps of the processor beside the `%State`.
pub(super) struct Processor {
    /// The program is the console's own kernel: exceptions are taken, nothing is answered.
    pub(super) kernel: bool,
    /// The exceptions being handled, innermost last: the two addresses each one's handler
    /// returns to (the instruction it was taken at, and the one after a `syscall`).
    exceptions: Vec<[u32; 2]>,
    /// Where the handler that last returned through a lifted function went.
    returned: u32,
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
        let gte = Isa::Mips.spec().offsets()[MIPS_GTE as usize];
        Some(Self { kernel, exceptions: Vec::new(), returned: 0, isolated: None, cop0, gp: vm.symbol(PSX_GP), gte })
    }
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
            .is_some_and(|returns| returns.contains(&target))
    }

    /// A lifted function's call or jump to `target`, which no lifted function is: the code there
    /// run as the function it is.
    pub(super) fn r3000_call(&mut self, vm: &mut Vm<'_>, state: u64, target: u64) -> Result<(), Stop> {
        let target = target as u32;
        if vm.trace {
            eprintln!("> the processor at {target:#x}");
        }
        self.r3000_run(vm, state, target, None).map(|_| ())
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
    /// to. The handler of exception `own` also ends where it jumps back to the exception's
    /// address. The result is the address control went to.
    fn r3000_run(&mut self, vm: &mut Vm<'_>, state: u64, entry: u32, own: Option<usize>) -> Result<u32, Stop> {
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
        };
        self.r3000_load(vm, state, &mut cpu)?;
        // The calls open; how control came to `pc`, once the slot of the jump that brought it has
        // run; the jump whose slot runs now.
        let mut depth = 0u32;
        let mut arrived: Option<Arrival> = None;
        let mut jumped: Option<Arrival> = None;
        let mut slot = false;
        let mut count = 0u64;
        loop {
            if !slot {
                if let Some(arrival) = arrived.take()
                    && cpu.next == cpu.pc.wrapping_add(4)
                    && let Some(target) = self.r3000_arrive(vm, state, &mut cpu, arrival, &mut depth, own)?
                {
                    vm.instructions += count;
                    vm.steps += count;
                    self.r3000_flush(vm, state, &mut cpu)?;
                    return Ok(target);
                }
                if count >= POLL || vm.memory.take_device_write() {
                    vm.instructions += count;
                    vm.steps += count;
                    count = 0;
                    self.r3000_flush(vm, state, &mut cpu)?;
                    vm.site = Some(u64::from(cpu.pc));
                    vm.poll_argument = Some(state);
                    self.console_poll(vm)?;
                    self.r3000_load(vm, state, &mut cpu)?;
                }
            }
            let current = cpu.pc;
            let word = fetch(vm, current)?;
            cpu.pc = cpu.next;
            cpu.next = cpu.next.wrapping_add(4);
            cpu.pending = cpu.load.take();
            cpu.branch = false;
            cpu.resumed = false;
            count += 1;
            let jump = self.r3000_exec(vm, state, &mut cpu, word, current, slot).map_err(|stop| match stop {
                Stop::Fault(message) if !message.starts_with(RETURNED) && !message.ends_with(')') => {
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
                arrived = jumped.take();
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
        own: Option<usize>,
    ) -> Result<Option<u32>, Stop> {
        let target = cpu.pc;
        match arrival {
            Arrival::Register(31) => {
                if *depth == 0 {
                    return Ok(Some(target));
                }
                *depth -= 1;
                return Ok(None);
            }
            // A handler returns through a register of the kernel's, never through `ra`.
            Arrival::Register(_) if self.returns_exception(target) => {
                let processor = self.processor()?;
                if own == Some(processor.exceptions.len() - 1) {
                    return Ok(Some(target));
                }
                processor.returned = target;
                self.r3000_flush(vm, state, cpu)?;
                return Err(Stop::fault(RETURNED));
            }
            Arrival::Call => *depth += 1,
            Arrival::Register(_) | Arrival::Jump => {}
        }
        // Where the callee returns to: its caller's link, which a tail jump leaves as it was.
        let link = cpu.r[31];
        let entry = target & 0x1fff_ffff;
        let answered = !self.processor()?.kernel && matches!(entry, 0xa0 | 0xb0 | 0xc0);
        if answered {
            self.r3000_bios(vm, state, cpu, entry)?;
        } else if matches!(vm.symbol_at(u64::from(target)), Some(Symbol::Function(_))) {
            let name = vm.name_at(u64::from(target)).unwrap_or_default().to_owned();
            if !vm.function(&name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void) {
                return Ok(None);
            }
            self.r3000_flush(vm, state, cpu)?;
            vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
            self.r3000_load(vm, state, cpu)?;
        } else {
            return Ok(None);
        }
        // The callee ran to its end. A call's goes on after it; a jump to it was the end of the
        // function that jumped.
        if arrival != Arrival::Call && *depth == 0 {
            return Ok(Some(link));
        }
        *depth -= 1;
        cpu.pc = link;
        cpu.next = link.wrapping_add(4);
        Ok(None)
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
        let target = self.psx_exception(vm, state, code, address, in_slot)?;
        self.r3000_load(vm, state, cpu)?;
        cpu.pc = target;
        cpu.next = target.wrapping_add(4);
        cpu.resumed = true;
        Ok(())
    }

    /// The processor takes the exception `code` at `address` (`slot` when the instruction is in
    /// a branch's delay slot and `address` is the branch): the cause and the return address into
    /// coprocessor 0, the status register's mode pushed, and the code at the vector run until it
    /// jumps back. The result is where it jumped back to.
    pub(super) fn psx_exception(&mut self, vm: &mut Vm<'_>, state: u64, code: u32, address: u32, slot: bool) -> Result<u32, Stop> {
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
        processor.exceptions.push([address, address.wrapping_add(4)]);
        let result = self.r3000_run(vm, state, vector, Some(index));
        let processor = self.processor()?;
        processor.exceptions.truncate(index);
        match result {
            Err(Stop::Fault(message)) if message.starts_with(RETURNED) => Ok(processor.returned),
            other => other,
        }
    }

    /// Between two instructions of a kernel's run: the interrupt line's level into the cause
    /// register, and the interrupt taken when the status register lets it through. `state` is
    /// the state of the code the look interrupts, when the run knows it.
    pub(super) fn psx_interrupt(&mut self, vm: &mut Vm<'_>, state: Option<u64>) -> Result<(), Stop> {
        let Some(line) = self.machine.as_ref().map(super::console::Machine::interrupt) else { return Ok(()) };
        let cop0 = self.processor()?.cop0;
        let cause = vm.memory.read_u64(cop0 + 8 * CAUSE)? as u32;
        let level = if line { cause | 0x400 } else { cause & !0x400 };
        if level != cause {
            vm.memory.write_u64(cop0 + 8 * CAUSE, sext(level))?;
        }
        let status = vm.memory.read_u64(cop0 + 8 * SR)? as u32;
        let (Some(state), Some(site)) = (state, vm.site) else { return Ok(()) };
        // A handler's return to the entry of a lifted function reads as a call of it: such an
        // instruction is not interrupted, the next look is soon enough.
        if !line || status & 0x401 != 0x401 || vm.symbol_at(site).is_some() {
            return Ok(());
        }
        self.psx_exception(vm, state, INTERRUPT, site as u32, false).map(|_| ())
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
        Ok(())
    }

    fn gte_slot(&mut self, vm: &mut Vm<'_>, state: u64, slot: u32) -> Result<u32, Stop> {
        let gte = self.processor()?.gte;
        vm.memory.read_u32(state + gte + 4 * u64::from(slot))
    }

    fn set_gte_slot(&mut self, vm: &mut Vm<'_>, state: u64, slot: u32, value: u32) -> Result<(), Stop> {
        let gte = self.processor()?.gte;
        vm.memory.write_u32(state + gte + 4 * u64::from(slot), value)
    }

    /// Data register `n` of the GTE as `mfc2` reads it: as the lift's `gte_data`.
    fn gte_data(&mut self, vm: &mut Vm<'_>, state: u64, n: u32) -> Result<u32, Stop> {
        match n {
            15 => self.gte_slot(vm, state, 14),
            28 | 29 => {
                let mut packed = 0;
                for k in 0..3 {
                    let ir = self.gte_slot(vm, state, 9 + k)?.cast_signed();
                    packed |= ((ir >> 7).clamp(0, 0x1f) as u32) << (5 * k);
                }
                Ok(packed)
            }
            31 => {
                let value = self.gte_slot(vm, state, 30)?.cast_signed();
                Ok((value ^ (value >> 31)).leading_zeros())
            }
            _ => self.gte_slot(vm, state, n),
        }
    }

    /// Writes data register `n` of the GTE as `mtc2` does: as the lift's `set_gte_data`.
    fn set_gte_data(&mut self, vm: &mut Vm<'_>, state: u64, n: u32, value: u32) -> Result<(), Stop> {
        match n {
            1 | 3 | 5 | 8..=11 => self.set_gte_slot(vm, state, n, i32::from(value as u16 as i16).cast_unsigned()),
            7 | 16..=19 => self.set_gte_slot(vm, state, n, value & 0xffff),
            15 => {
                let (s1, s2) = (self.gte_slot(vm, state, 13)?, self.gte_slot(vm, state, 14)?);
                self.set_gte_slot(vm, state, 12, s1)?;
                self.set_gte_slot(vm, state, 13, s2)?;
                self.set_gte_slot(vm, state, 14, value)
            }
            28 => {
                for k in 0..3 {
                    self.set_gte_slot(vm, state, 9 + k, ((value >> (5 * k)) & 0x1f) << 7)?;
                }
                self.set_gte_slot(vm, state, 28, value)
            }
            29 | 31 => Ok(()),
            _ => self.set_gte_slot(vm, state, n, value),
        }
    }

    /// Writes control register `n` of the GTE as `ctc2` does: as the lift's `set_gte_control`.
    fn set_gte_control(&mut self, vm: &mut Vm<'_>, state: u64, n: u32, value: u32) -> Result<(), Stop> {
        let value = match n {
            4 | 12 | 20 | 26 | 27 | 29 | 30 => i32::from(value as u16 as i16).cast_unsigned(),
            31 => (value & 0x7fff_f000) | if value & 0x7f87_e000 != 0 { 1 << 31 } else { 0 },
            _ => value,
        };
        self.set_gte_slot(vm, state, 32 + n, value)
    }

    /// One instruction: `word`, read at `current`, in a delay slot when `in_slot`. The result is
    /// the jump it made, when it is one whose target may be a callee's entry or a return.
    #[allow(clippy::too_many_lines)]
    fn r3000_exec(&mut self, vm: &mut Vm<'_>, state: u64, cpu: &mut Cpu, word: u32, current: u32, in_slot: bool) -> Result<Option<Arrival>, Stop> {
        let (op, rs, rt, rd) = (word >> 26, (word >> 21 & 31) as usize, (word >> 16 & 31) as usize, (word >> 11 & 31) as usize);
        let (shift, funct) = (word >> 6 & 31, word & 0x3f);
        let (s, t) = (cpu.r[rs], cpu.r[rt]);
        let simm = i32::from(word as u16 as i16).cast_unsigned();
        let address = s.wrapping_add(simm);
        let at = u64::from(address);
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
                        if rd as u64 == SR {
                            self.psx_status(vm)?;
                        }
                    }
                    // `rfe`: the status register's mode pops.
                    16..=31 if funct == 0x10 => {
                        let status = vm.memory.read_u64(cop0 + 8 * SR)? as u32;
                        vm.memory.write_u64(cop0 + 8 * SR, sext((status & !0xf) | ((status >> 2) & 0xf)))?;
                    }
                    _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
                }
            }
            18 => match rs {
                0 => {
                    let value = self.gte_data(vm, state, rd as u32)?;
                    cpu.delay(rt, value);
                }
                2 => {
                    let value = self.gte_slot(vm, state, 32 + rd as u32)?;
                    cpu.delay(rt, value);
                }
                4 => self.set_gte_data(vm, state, rd as u32, t)?,
                6 => self.set_gte_control(vm, state, rd as u32, t)?,
                16..=31 => {
                    let gte = self.processor()?.gte;
                    super::console::gte(vm, &[Value::Int(u128::from(state + gte)), Value::Int(u128::from(word))])?;
                }
                _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
            },
            32 => {
                let value = i32::from(vm.memory.read_u8(at)?.cast_signed()).cast_unsigned();
                cpu.delay(rt, value);
            }
            33 => {
                let value = i32::from(vm.memory.read_u16(at)?.cast_signed()).cast_unsigned();
                cpu.delay(rt, value);
            }
            34 | 38 => {
                let held = cpu.merged(rt);
                let memory = vm.memory.read_u32(at & !3)?;
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
                let value = vm.memory.read_u32(at)?;
                cpu.delay(rt, value);
            }
            36 => {
                let value = u32::from(vm.memory.read_u8(at)?);
                cpu.delay(rt, value);
            }
            37 => {
                let value = u32::from(vm.memory.read_u16(at)?);
                cpu.delay(rt, value);
            }
            40 => vm.memory.write_u8(at, t as u8)?,
            41 => vm.memory.write_u16(at, t as u16)?,
            42 | 46 => {
                let memory = vm.memory.read_u32(at & !3)?;
                let value = match (op, address & 3) {
                    (42, 0) => (memory & 0xffff_ff00) | (t >> 24),
                    (42, 1) => (memory & 0xffff_0000) | (t >> 16),
                    (42, 2) => (memory & 0xff00_0000) | (t >> 8),
                    (46, 1) => (memory & 0x0000_00ff) | (t << 8),
                    (46, 2) => (memory & 0x0000_ffff) | (t << 16),
                    (46, 3) => (memory & 0x00ff_ffff) | (t << 24),
                    _ => t,
                };
                vm.memory.write_u32(at & !3, value)?;
            }
            43 => vm.memory.write_u32(at, t)?,
            50 => {
                let value = vm.memory.read_u32(at)?;
                self.set_gte_data(vm, state, rt as u32, value)?;
            }
            58 => {
                let value = self.gte_data(vm, state, rt as u32)?;
                vm.memory.write_u32(at, value)?;
            }
            _ => self.r3000_raise(vm, state, cpu, RESERVED, current, in_slot)?,
        }
        Ok(None)
    }
}
