//! The EE kernel as a program calls it: the functions of its system-call table by their names.
//! The threads, the semaphores, the interrupt and DMA handlers and the alarms are kept here, and
//! the calls into the program they come to (a handler, a thread's entry) are the machine's
//! deliveries.
//!
//! A thread started runs as a call nested in the one that started it, to its end or to the wait
//! it never leaves: the threads of a program take turns only where one waits for what an
//! interrupt gives. A thread that waits for the thread it was started from waits for ever.

use std::collections::{BTreeMap, VecDeque};

use super::{CLOCK, Event, Ps2};
use crate::{Answer, Delivery, Memory, MemoryExt, Stop};

/// A thread's states, as `ReferThreadStatus` gives them.
const RUNNING: u32 = 0x01;
const READY: u32 = 0x02;
const WAITING: u32 = 0x04;
const DORMANT: u32 = 0x10;
/// The thread `SetupThread` makes of the program's start.
const MAIN: usize = 1;
/// The cycles of a scanline, which an alarm's time counts.
const SCANLINE: u64 = CLOCK / 15_734;

#[derive(Debug, Clone, Copy, Default)]
struct Thread {
    entry: u32,
    stack: u32,
    stack_size: u32,
    gp: u32,
    priority: u32,
    status: u32,
    wakeups: u32,
}

#[derive(Debug, Clone, Copy)]
struct Handler {
    id: u32,
    /// A handler of a DMA channel, not of an interrupt line.
    dma: bool,
    cause: u32,
    entry: u32,
    argument: u32,
}

/// A call into the program the kernel has due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Handler { entry: u32, cause: u32, argument: u32 },
    Thread { id: usize, argument: u32, from: usize },
    Alarm { entry: u32, id: u32, time: u32, argument: u32 },
}

/// What the innermost thread waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    Semaphore(usize),
    Wakeup(usize),
}

pub(super) struct Kernel {
    /// The mode `SetGsCrt` set: 2 is NTSC, 3 PAL.
    pub(super) video_mode: u32,
    threads: Vec<Thread>,
    current: usize,
    /// Each semaphore's count and most; `None` for a deleted one.
    semaphores: Vec<Option<(u32, u32)>>,
    handlers: Vec<Handler>,
    next_handler: u32,
    /// The alarms set, by id: the handler, the time and the argument.
    alarms: BTreeMap<u32, (u32, u32, u32)>,
    next_alarm: u32,
    heap_end: u32,
    /// Where `SetVSyncFlag` asked for the flag and the status register's copy at each blank.
    vsync_flag: (u32, u32),
    sif_registers: BTreeMap<u32, u32>,
    waits: Vec<Wait>,
    steps: VecDeque<Step>,
    running: Option<Step>,
    token: u64,
}

impl Default for Kernel {
    fn default() -> Self {
        let main = Thread { status: RUNNING, priority: 1, ..Thread::default() };
        Self {
            video_mode: 2,
            threads: vec![Thread { status: DORMANT, ..Thread::default() }, main],
            current: MAIN,
            semaphores: Vec::new(),
            handlers: Vec::new(),
            next_handler: 1,
            alarms: BTreeMap::new(),
            next_alarm: 1,
            heap_end: 0,
            vsync_flag: (0, 0),
            sif_registers: BTreeMap::new(),
            waits: Vec::new(),
            steps: VecDeque::new(),
            running: None,
            token: 0,
        }
    }
}

impl Ps2 {
    /// The kernel's part of a vertical blank: the flags `SetVSyncFlag` asked for.
    pub(super) fn kernel_vblank(&mut self, memory: &mut dyn Memory) {
        let (flag, status) = self.kernel.vsync_flag;
        if flag != 0 {
            memory.set_u32(u64::from(flag), 1);
        }
        if status != 0 {
            memory.set_u64(u64::from(status), self.gs.csr);
        }
    }

    pub(super) fn kernel_alarm(&mut self, id: u32) {
        if let Some((entry, time, argument)) = self.kernel.alarms.remove(&id) {
            self.kernel.steps.push_back(Step::Alarm { entry, id, time, argument });
        }
    }

    /// The next call into the program: a step still due, else the handlers of the interrupts and
    /// DMA channels pending, which the kernel acknowledges as it takes them.
    pub(super) fn kernel_deliver(&mut self) -> Option<Delivery> {
        if self.kernel.running.is_some() {
            return None;
        }
        if self.kernel.steps.is_empty() {
            let (lines, channels) = (self.i_stat & self.i_mask, self.d_stat & (self.d_stat >> 16));
            for line in (0..15).filter(|line| lines & (1 << line) != 0) {
                let handlers: Vec<Step> = self.kernel.handlers.iter().filter(|h| !h.dma && h.cause == line).map(Self::step_of).collect();
                if !handlers.is_empty() {
                    self.i_stat &= !(1 << line);
                    self.kernel.steps.extend(handlers);
                }
            }
            for channel in (0..10).filter(|channel| channels & (1 << channel) != 0) {
                let handlers: Vec<Step> = self.kernel.handlers.iter().filter(|h| h.dma && h.cause == channel).map(Self::step_of).collect();
                if !handlers.is_empty() {
                    self.d_stat &= !(1 << channel);
                    self.kernel.steps.extend(handlers);
                }
            }
        }
        let step = self.kernel.steps.pop_front()?;
        self.kernel.running = Some(step);
        self.kernel.token += 1;
        let token = self.kernel.token;
        Some(match step {
            Step::Handler { entry, cause, argument } => {
                Delivery { entry: entry.into(), args: [cause.into(), argument.into(), 0, 0], stack: None, gp: None, token }
            }
            Step::Alarm { entry, id, time, argument } => {
                Delivery { entry: entry.into(), args: [id.into(), time.into(), argument.into(), 0], stack: None, gp: None, token }
            }
            Step::Thread { id, argument, .. } => {
                let thread = &mut self.kernel.threads[id];
                thread.status = RUNNING;
                self.kernel.current = id;
                let thread = self.kernel.threads[id];
                Delivery {
                    entry: thread.entry.into(),
                    args: [argument.into(), 0, 0, 0],
                    stack: Some(u64::from(thread.stack + thread.stack_size) & !0xf),
                    gp: (thread.gp != 0).then_some(thread.gp.into()),
                    token,
                }
            }
        })
    }

    fn step_of(handler: &Handler) -> Step {
        Step::Handler { entry: handler.entry, cause: handler.cause, argument: handler.argument }
    }

    /// The end of a delivered call: a thread that returned, or left through `ExitThread`, is over.
    pub(super) fn kernel_delivered(&mut self, _delivery: &Delivery, _result: u64) {
        if let Some(Step::Thread { id, from, .. }) = self.kernel.running.take() {
            self.kernel.threads[id].status = DORMANT;
            self.kernel.current = from;
        }
    }

    /// The result of the wait in flight, once what the innermost thread waits for came.
    pub(super) fn kernel_resume(&mut self) -> Option<u64> {
        match *self.kernel.waits.last()? {
            Wait::Semaphore(id) => {
                let (count, _) = self.kernel.semaphores.get_mut(id)?.as_mut()?;
                if *count == 0 {
                    return None;
                }
                *count -= 1;
                self.kernel.waits.pop();
                Some(id as u64)
            }
            Wait::Wakeup(id) => {
                let thread = &mut self.kernel.threads[id];
                if thread.wakeups == 0 {
                    return None;
                }
                thread.wakeups -= 1;
                thread.status = RUNNING;
                self.kernel.waits.pop();
                Some(id as u64)
            }
        }
    }

    /// The kernel function `name` on its register arguments, `a0` to `a3` and then `t0` on.
    pub(super) fn kernel_call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop> {
        let a = |i: usize| args.get(i).copied().unwrap_or(0) as u32;
        let value = |v: u64| Ok(Answer::Value(v));
        // An error is a negative word, which the register holds sign-extended.
        let failed = || Ok(Answer::Value(u64::MAX));
        let (a0, a1, a2) = (a(0), a(1), a(2));
        // The handler forms (`iSignalSema`, `_iWakeupThread`) do what the plain ones do.
        let plain = name.trim_start_matches('_').trim_start_matches('i');
        let plain = if name.starts_with('i') || name.starts_with("_i") { plain } else { name.trim_start_matches('_') };
        match plain {
            // The program's end.
            "KExit" => Err(Stop::Exit(a0.cast_signed())),
            "LoadExecPS2" | "ExecPS2" | "ExecOSD" => Err(Stop::Exit(0)),
            "ExitThread" | "ExitDeleteThread" => {
                if self.kernel.current == MAIN {
                    return Err(Stop::Exit(0));
                }
                Ok(Answer::Leave)
            }

            // The start routine's calls: the main thread's stack, at the top of RAM when the
            // program names none, and the heap, up to that stack when it gives no size.
            "SetupThread" => {
                let size = a2;
                let stack = if a1 == u32::MAX { super::RAM_SIZE as u32 - size } else { a1 };
                let main = &mut self.kernel.threads[MAIN];
                (main.gp, main.stack, main.stack_size) = (a0, stack, size);
                self.kernel.heap_end = stack;
                value(u64::from(stack + size))
            }
            "SetupHeap" => {
                if a1 != u32::MAX {
                    self.kernel.heap_end = a0.wrapping_add(a1);
                }
                value(u64::from(self.kernel.heap_end))
            }
            "EndOfHeap" => {
                if self.kernel.heap_end == 0 {
                    // A program entered past its start routine: the heap ends under a stack's room.
                    self.kernel.heap_end = super::RAM_SIZE as u32 - (1 << 20);
                }
                value(u64::from(self.kernel.heap_end))
            }
            "GetMemorySize" => value(super::RAM_SIZE),
            "MachineType"
            | "PSMode"
            | "Deci2Call"
            | "GetCop0"
            | "CpuConfig"
            | "KSeg0"
            | "EnableCache"
            | "DisableCache"
            | "FlushCache"
            | "InitTLB"
            | "SetTLBEntry"
            | "ResetEE"
            | "SetOsdConfigParam"
            | "SetOsdConfigParam2"
            | "GetOsdConfigParam2"
            | "SetGsHParam"
            | "SetGsVParam"
            | "GetGsHParam"
            | "GetGsVParam"
            | "SetPgifHandler"
            | "SetVTLBRefillHandler"
            | "SetVCommonHandler"
            | "SetVInterruptHandler"
            | "SetCPUTimerHandler"
            | "SetCPUTimer"
            | "SetSyscall"
            | "Copy"
            | "GetEntryAddress"
            | "RotateThreadReadyQueue"
            | "ChangeThreadPriority"
            | "EnableDispatchThread"
            | "DisableDispatchThread"
            | "SuspendThread"
            | "ResumeThread"
            | "ReleaseWaitThread"
            | "CancelWakeupThread" => value(0),
            "GetOsdConfigParam" => {
                memory.set_u32(u64::from(a0), 0);
                value(0)
            }

            // The graphics synthesizer's part of the kernel.
            "SetGsCrt" => {
                self.kernel.video_mode = a1;
                // The kernel sets the display's mode itself: interlace, and frame or field.
                self.gs.write(super::gs::SMODE2, u64::from(a0 & 1) | u64::from(a2 & 1) << 1);
                value(0)
            }
            "GsGetIMR" => value(self.gs.imr),
            "GsPutIMR" => {
                let before = self.gs.imr;
                self.gs.write(super::gs::IMR, args.first().copied().unwrap_or(0));
                value(before)
            }
            "SetVSyncFlag" => {
                self.kernel.vsync_flag = (a0, a1);
                value(0)
            }

            // The interrupt and DMA handlers.
            "AddIntcHandler" | "AddDmacHandler" => {
                let id = self.kernel.next_handler;
                self.kernel.next_handler += 1;
                let handler = Handler { id, dma: plain == "AddDmacHandler", cause: a0, entry: a1, argument: a(3) };
                // A handler added first (`next` of zero) is called ahead of the others.
                if a2 == 0 {
                    self.kernel.handlers.insert(0, handler);
                } else {
                    self.kernel.handlers.push(handler);
                }
                value(u64::from(id))
            }
            "RemoveIntcHandler" | "RemoveDmacHandler" => {
                let dma = plain == "RemoveDmacHandler";
                self.kernel.handlers.retain(|h| !(h.dma == dma && h.cause == a0 && h.id == a1));
                value(u64::from(self.kernel.handlers.iter().filter(|h| h.dma == dma && h.cause == a0).count() as u32))
            }
            "EnableIntc" | "DisableIntc" => {
                let was = self.i_mask & (1 << a0) != 0;
                self.i_mask = (self.i_mask & !(1 << a0)) | u32::from(plain == "EnableIntc") << a0;
                value(u64::from(was != (plain == "EnableIntc")))
            }
            "EnableDmac" | "DisableDmac" => {
                let bit = 1 << (16 + a0);
                let was = self.d_stat & bit != 0;
                self.d_stat = (self.d_stat & !bit) | if plain == "EnableDmac" { bit } else { 0 };
                value(u64::from(was != (plain == "EnableDmac")))
            }
            "EnableIntcHandler"
            | "DisableIntcHandler"
            | "EnableDmacHandler"
            | "DisableDmacHandler"
            | "AddSbusIntcHandler"
            | "RemoveSbusIntcHandler"
            | "Interrupt2Iop" => value(0),
            "SetAlarm" => {
                let id = self.kernel.next_alarm;
                self.kernel.next_alarm += 1;
                self.kernel.alarms.insert(id, (a1, a0 & 0xffff, a2));
                self.clock.after(u64::from(a0 & 0xffff) * SCANLINE, Event::Alarm(id));
                value(u64::from(id))
            }
            "ReleaseAlarm" => {
                self.kernel.alarms.remove(&a0);
                self.clock.cancel(|e| *e == Event::Alarm(a0));
                value(0)
            }

            // The threads.
            "CreateThread" => {
                let word = |memory: &mut dyn Memory, at: u64| memory.u32(u64::from(a0) + at);
                let thread = Thread {
                    entry: word(memory, 4),
                    stack: word(memory, 8),
                    stack_size: word(memory, 12),
                    gp: word(memory, 16),
                    priority: word(memory, 20),
                    status: DORMANT,
                    wakeups: 0,
                };
                self.kernel.threads.push(thread);
                value(self.kernel.threads.len() as u64 - 1)
            }
            "DeleteThread" | "TerminateThread" => value(u64::from(a0)),
            "StartThread" => {
                let Some(thread) = self.kernel.threads.get_mut(a0 as usize).filter(|t| t.status == DORMANT && t.entry != 0) else {
                    return failed();
                };
                thread.status = READY;
                self.kernel.steps.push_back(Step::Thread { id: a0 as usize, argument: a1, from: self.kernel.current });
                value(u64::from(a0))
            }
            "GetThreadId" => value(self.kernel.current as u64),
            "ReferThreadStatus" => {
                let id = if a0 == 0 { self.kernel.current } else { a0 as usize };
                let Some(thread) = self.kernel.threads.get(id).copied() else { return failed() };
                if a1 != 0 {
                    let words = [
                        thread.status,
                        thread.entry,
                        thread.stack,
                        thread.stack_size,
                        thread.gp,
                        thread.priority,
                        thread.priority,
                        0,
                        0,
                        0,
                        0,
                        thread.wakeups,
                    ];
                    for (i, word) in words.into_iter().enumerate() {
                        memory.set_u32(u64::from(a1) + 4 * i as u64, word);
                    }
                }
                value(u64::from(thread.status))
            }
            "SleepThread" => {
                let id = self.kernel.current;
                let thread = &mut self.kernel.threads[id];
                if thread.wakeups > 0 {
                    thread.wakeups -= 1;
                    return value(id as u64);
                }
                thread.status = WAITING;
                self.kernel.waits.push(Wait::Wakeup(id));
                Ok(Answer::Wait)
            }
            "WakeupThread" => {
                let Some(thread) = self.kernel.threads.get_mut(a0 as usize) else { return failed() };
                thread.wakeups += 1;
                value(u64::from(a0))
            }

            // The semaphores: a count, its most, and the initial count.
            "CreateSema" => {
                let (most, count) = (memory.u32(u64::from(a0) + 4), memory.u32(u64::from(a0) + 8));
                self.kernel.semaphores.push(Some((count, most)));
                value(self.kernel.semaphores.len() as u64 - 1)
            }
            "DeleteSema" => {
                if let Some(slot) = self.kernel.semaphores.get_mut(a0 as usize) {
                    *slot = None;
                }
                value(u64::from(a0))
            }
            "SignalSema" => {
                let Some(Some((count, most))) = self.kernel.semaphores.get_mut(a0 as usize) else { return failed() };
                *count = (*count + 1).min((*most).max(1));
                value(u64::from(a0))
            }
            "WaitSema" | "PollSema" => {
                let Some(Some((count, _))) = self.kernel.semaphores.get_mut(a0 as usize) else { return failed() };
                if *count > 0 {
                    *count -= 1;
                    return value(u64::from(a0));
                }
                if plain == "PollSema" {
                    return failed();
                }
                self.kernel.waits.push(Wait::Semaphore(a0 as usize));
                Ok(Answer::Wait)
            }
            "ReferSemaStatus" => {
                let Some(Some((count, most))) = self.kernel.semaphores.get(a0 as usize).copied() else { return failed() };
                memory.set_u32(u64::from(a1), count);
                memory.set_u32(u64::from(a1) + 4, most);
                value(u64::from(a0))
            }

            // The SIF, with no IOP at its other end: a transfer is over as it is set, and the
            // registers keep what is written.
            "SifSetDma" => value(1),
            "SifDmaStat" => failed(),
            "SifSetDChain" | "SifStopDma" => value(0),
            "SifSetReg" => {
                self.kernel.sif_registers.insert(a0, a1);
                value(0)
            }
            "SifGetReg" => value(u64::from(self.kernel.sif_registers.get(&a0).copied().unwrap_or(0))),
            _ => Err(Stop::fault(format!("the EE kernel function {name} is not answered"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{I_MASK, I_STAT, NTSC, intc};
    use super::*;
    use crate::host::{Capture, Host};
    use crate::{Platform, Ram};

    fn machine() -> (Ps2, Ram) {
        (Ps2::new(Host::headless(&Capture::new())), Ram::new(0, 0x10_0000))
    }

    fn call(ps2: &mut Ps2, ram: &mut Ram, name: &str, args: &[u64]) -> u64 {
        match ps2.call(ram, name, args) {
            Ok(Answer::Value(v)) => v,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn the_start_routine_gets_its_stack_at_the_top_of_ram_and_its_heap_under_it() {
        let (mut ps2, mut ram) = machine();
        let top = call(&mut ps2, &mut ram, "SetupThread", &[0x12_90f0, u64::from(u32::MAX), 0x2_0000, 0, 0]);
        assert_eq!(top, 0x200_0000);
        assert_eq!(call(&mut ps2, &mut ram, "SetupHeap", &[0x13_0000, u64::from(u32::MAX)]), 0x1fe_0000);
        assert_eq!(call(&mut ps2, &mut ram, "EndOfHeap", &[]), 0x1fe_0000);
        assert_eq!(call(&mut ps2, &mut ram, "GetThreadId", &[]), 1);
        assert_eq!(ps2.call(&mut ram, "ExitThread", &[]), Err(Stop::Exit(0)), "the main thread's end is the program's");
    }

    #[test]
    fn a_handler_is_called_for_its_enabled_interrupt_and_the_kernel_acknowledges() {
        let (mut ps2, mut ram) = machine();
        let id = call(&mut ps2, &mut ram, "AddIntcHandler", &[2, 0x10_2000, 0, 0x55]);
        call(&mut ps2, &mut ram, "_EnableIntc", &[2]);
        assert_ne!(ps2.read(&mut ram, I_MASK, 32) & u64::from(intc::VBLANK_START), 0);
        assert_eq!(ps2.deliver(&mut ram), None);
        ps2.advance(&mut ram, NTSC).unwrap();
        let delivery = ps2.deliver(&mut ram).expect("the handler");
        assert_eq!((delivery.entry, delivery.args[0], delivery.args[1]), (0x10_2000, 2, 0x55));
        assert_eq!(ps2.read(&mut ram, I_STAT, 32) & u64::from(intc::VBLANK_START), 0, "acknowledged");
        assert_eq!(ps2.deliver(&mut ram), None, "one call at a time");
        ps2.delivered(&mut ram, &delivery, 0);
        assert_eq!(ps2.deliver(&mut ram), None);
        assert_eq!(call(&mut ps2, &mut ram, "RemoveIntcHandler", &[2, id]), 0);
    }

    #[test]
    fn a_wait_on_a_semaphore_ends_when_a_handler_signals_it() {
        let (mut ps2, mut ram) = machine();
        ram.set_u32(0x104, 1);
        let semaphore = call(&mut ps2, &mut ram, "CreateSema", &[0x100]);
        assert_eq!(ps2.call(&mut ram, "WaitSema", &[semaphore]), Ok(Answer::Wait));
        assert_eq!(ps2.resume(&mut ram), None);
        call(&mut ps2, &mut ram, "iSignalSema", &[semaphore]);
        assert_eq!(ps2.resume(&mut ram), Some(semaphore));
        assert_eq!(ps2.call(&mut ram, "PollSema", &[semaphore]), Ok(Answer::Value(u64::MAX)), "taken");
    }

    #[test]
    fn a_started_thread_is_a_call_on_its_own_stack_until_it_exits() {
        let (mut ps2, mut ram) = machine();
        for (offset, word) in [(4u64, 0x10_3000u32), (8, 0x8_0000), (12, 0x1000), (16, 0x12_90f0), (20, 5)] {
            ram.set_u32(0x200 + offset, word);
        }
        let thread = call(&mut ps2, &mut ram, "CreateThread", &[0x200]);
        call(&mut ps2, &mut ram, "StartThread", &[thread, 7]);
        let delivery = ps2.deliver(&mut ram).expect("the thread's entry");
        assert_eq!((delivery.entry, delivery.args[0], delivery.stack, delivery.gp), (0x10_3000, 7, Some(0x8_1000), Some(0x12_90f0)));
        assert_eq!(call(&mut ps2, &mut ram, "GetThreadId", &[]), thread);
        assert_eq!(ps2.call(&mut ram, "ExitThread", &[]), Ok(Answer::Leave));
        ps2.delivered(&mut ram, &delivery, 0);
        assert_eq!(call(&mut ps2, &mut ram, "GetThreadId", &[]), 1, "back in the main thread");
    }

    #[test]
    fn an_alarm_calls_its_handler_after_its_scanlines() {
        let (mut ps2, mut ram) = machine();
        let alarm = call(&mut ps2, &mut ram, "_SetAlarm", &[10, 0x10_4000, 0x99]);
        ps2.advance(&mut ram, 10 * SCANLINE - 1).unwrap();
        assert_eq!(ps2.deliver(&mut ram), None);
        ps2.advance(&mut ram, 1).unwrap();
        let delivery = ps2.deliver(&mut ram).expect("the alarm's handler");
        assert_eq!((delivery.entry, delivery.args), (0x10_4000, [alarm, 10, 0x99, 0]));
    }
}
