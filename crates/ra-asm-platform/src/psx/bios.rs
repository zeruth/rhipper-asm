//! The PlayStation's BIOS as a program calls it: the functions of the `A0`, `B0` and `C0` tables
//! by their names, the two critical-section system calls, and the kernel's part of an interrupt
//! (the pad driver, the events, the handler chains and the exit hook). The functions are done
//! here, on the program's memory; none of the BIOS's own code runs.

use std::collections::{BTreeMap, VecDeque};

use super::{Psx, RAM, irq};
use crate::host::File;
use crate::{Answer, Delivery, Memory, MemoryExt, Stop};

/// An event's states, as `TestEvent` and the event's control block hold them.
const EVENT_FREE: u32 = 0;
const EVENT_DISABLED: u32 = 0x1000;
const EVENT_ENABLED: u32 = 0x2000;
const EVENT_READY: u32 = 0x4000;
/// The mode of an event that calls its function when it is delivered.
const MODE_INTERRUPT: u32 = 0x1000;
/// The handles of the events: the index, under this.
const EVENT_HANDLES: u32 = 0xf100_0000;
/// The root counters' event class (the fourth is the vertical blank), and the spec of an
/// interrupt.
const CLASS_COUNTER: u32 = 0xf200_0000;
const SPEC_INTERRUPT: u32 = 0x0002;
/// The memory cards' event class, and the spec of a card that did not answer.
const CLASS_CARD: u32 = 0xf400_0001;
const SPEC_TIMEOUT: u32 = 0x0100;

/// Where `alloc_kernel_memory` hands out of: the kernel's part of RAM, below the program.
const KERNEL_HEAP: u32 = 0x8000_e000;
const KERNEL_HEAP_END: u32 = 0x8001_0000;

#[derive(Debug, Clone, Copy, Default)]
struct Event {
    class: u32,
    spec: u32,
    mode: u32,
    function: u32,
    status: u32,
}

/// What the kernel's interrupt handler still has to call for the interrupt it is handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// An event's function, or a chain element's second one with the first's result.
    Call(u32, u32),
    /// A chain element's first function, whose result decides whether the second is called.
    Verify(u32, u32),
    /// The exit hook: the code the program's `jmp_buf` names, on its stack.
    Hook(u32),
}

pub(super) struct Bios {
    /// The blocks `malloc` handed out, by address, and the heap's bounds.
    blocks: BTreeMap<u32, u32>,
    heap: (u32, u32),
    kernel_next: u32,
    /// The open files by descriptor (the first two are the TTY), each with its position.
    files: Vec<Option<(Box<dyn File>, u64)>>,
    events: Vec<Event>,
    /// The two pad buffers of `InitPad` with their lengths, and whether `StartPad` ran.
    pads: [(u32, u32); 2],
    pads_started: bool,
    /// Where the kernel's first pad driver leaves both pads' buttons, when a program asked.
    outdated_pad: u32,
    /// Acknowledge the vertical blank, and each root counter's interrupt, in the kernel.
    clear_pad: bool,
    clear_counter: [bool; 4],
    /// The `jmp_buf` the exception handler leaves through, when the program set one.
    hook: Option<u32>,
    /// The handler chains by priority, each element's address, the newest first.
    chains: [Vec<u32>; 4],
    /// Interrupts are off: inside a critical section.
    pub(super) critical: bool,
    /// An interrupt is being handled: its steps still to call, and the one being called.
    pub(super) handling: bool,
    pub(super) steps: VecDeque<Step>,
    pub(super) current: Option<Step>,
    /// The event a `WaitEvent` waits for.
    waiting: Option<usize>,
    seed: u32,
    token: u32,
    last_error: u32,
}

impl Default for Bios {
    fn default() -> Self {
        Self {
            blocks: BTreeMap::new(),
            heap: (0, 0),
            kernel_next: KERNEL_HEAP,
            files: Vec::new(),
            events: Vec::new(),
            pads: [(0, 0); 2],
            pads_started: false,
            outdated_pad: 0,
            clear_pad: false,
            clear_counter: [false; 4],
            hook: None,
            chains: Default::default(),
            critical: false,
            handling: false,
            steps: VecDeque::new(),
            current: None,
            waiting: None,
            seed: 0,
            token: 0,
            last_error: 0,
        }
    }
}

impl Bios {
    fn allocate(&mut self, size: u32) -> u32 {
        let size = size.max(1).next_multiple_of(8);
        let mut at = self.heap.0;
        for (&start, &len) in &self.blocks {
            if start.saturating_sub(at) >= size {
                break;
            }
            at = start + len;
        }
        if at == 0 || self.heap.1.saturating_sub(at) < size {
            return 0;
        }
        self.blocks.insert(at, size);
        at
    }

    fn event(&mut self, handle: u32) -> Option<&mut Event> {
        let index = handle.checked_sub(EVENT_HANDLES)? as usize;
        self.events.get_mut(index).filter(|e| e.status != EVENT_FREE)
    }

    /// Marks every enabled event of `class` and `spec` delivered: one that calls its function
    /// gives the function, another becomes ready.
    fn deliver_event(&mut self, class: u32, spec: u32) -> Vec<u32> {
        let mut calls = Vec::new();
        for event in self.events.iter_mut().filter(|e| e.status == EVENT_ENABLED && e.class == class && e.spec == spec) {
            if event.mode == MODE_INTERRUPT {
                calls.push(event.function);
            } else {
                event.status = EVENT_READY;
            }
        }
        calls
    }
}

/// The string functions' view of the program's memory.
fn text(memory: &mut dyn Memory, address: u32) -> Vec<u8> {
    memory.cstr(u64::from(address), 1 << 20)
}

fn put_text(memory: &mut dyn Memory, address: u32, bytes: &[u8]) {
    memory.write(u64::from(address), bytes);
    memory.set_u8(u64::from(address) + bytes.len() as u64, 0);
}

/// `strtol` and its kin: the value of the number at `address` in `base` (zero to read the base
/// from the prefix), and the address past it.
fn number(memory: &mut dyn Memory, address: u32, base: u32) -> (u32, u32) {
    let bytes = text(memory, address);
    let mut at = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
    let negative = bytes.get(at) == Some(&b'-');
    if matches!(bytes.get(at), Some(b'-' | b'+')) {
        at += 1;
    }
    let hex = bytes.get(at) == Some(&b'0') && matches!(bytes.get(at + 1), Some(b'x' | b'X'));
    let base = match base {
        0 if hex => 16,
        0 if bytes.get(at) == Some(&b'0') => 8,
        0 => 10,
        b => b,
    };
    if base == 16 && hex {
        at += 2;
    }
    let mut value = 0u32;
    while let Some(digit) = bytes.get(at).and_then(|b| (*b as char).to_digit(base)) {
        value = value.wrapping_mul(base).wrapping_add(digit);
        at += 1;
    }
    (if negative { value.wrapping_neg() } else { value }, address + at as u32)
}

impl Psx {
    /// The kernel's part of an interrupt that came due: the pad driver and the events of the
    /// interrupts pending, then the handler chains, then the exit hook. Gives whether anything of
    /// the program is to be called.
    pub(super) fn begin_exception(&mut self, memory: &mut dyn Memory, pending: u32) -> bool {
        let mut steps = VecDeque::new();
        if pending & irq::VBLANK != 0 {
            if self.bios.pads_started {
                self.poll_pads(memory);
            }
            steps.extend(self.bios.deliver_event(CLASS_COUNTER | 3, SPEC_INTERRUPT).into_iter().map(|f| Step::Call(f, 0)));
            if self.bios.clear_pad || self.bios.clear_counter[3] {
                self.i_stat &= !irq::VBLANK;
            }
        }
        for n in 0..3 {
            let bit = irq::TIMER0 << n;
            if pending & bit != 0 {
                steps.extend(self.bios.deliver_event(CLASS_COUNTER | n, SPEC_INTERRUPT).into_iter().map(|f| Step::Call(f, 0)));
                if self.bios.clear_counter[n as usize] {
                    self.i_stat &= !bit;
                }
            }
        }
        for chain in &self.bios.chains {
            for &element in chain {
                // An element: the next one, the handler, the verifier.
                let (handler, verify) = (memory.u32(u64::from(element) + 4), memory.u32(u64::from(element) + 8));
                if verify != 0 {
                    steps.push_back(Step::Verify(verify, handler));
                } else if handler != 0 {
                    steps.push_back(Step::Call(handler, 0));
                }
            }
        }
        if let Some(hook) = self.bios.hook {
            steps.push_back(Step::Hook(hook));
        }
        self.bios.steps = steps;
        self.bios.handling = !self.bios.steps.is_empty();
        self.bios.handling
    }

    /// The next step of the interrupt being handled as a call into the program.
    pub(super) fn next_step(&mut self, memory: &mut dyn Memory) -> Option<Delivery> {
        let step = self.bios.steps.pop_front()?;
        self.bios.current = Some(step);
        self.bios.token += 1;
        let token = u64::from(self.bios.token);
        Some(match step {
            Step::Call(entry, argument) => {
                Delivery { entry: entry.into(), args: [argument.into(), 0, 0, 0], stack: None, gp: None, registers: Vec::new(), token }
            }
            Step::Verify(entry, _) => Delivery { entry: entry.into(), args: [0; 4], stack: None, gp: None, registers: Vec::new(), token },
            // The buffer `setjmp` fills: `ra`, `sp`, `fp`, `s0` to `s7`, `gp`. The kernel leaves
            // through it as `longjmp` does: the saved registers back, and one in `v0`.
            Step::Hook(buffer) => {
                let word = |memory: &mut dyn Memory, i: u64| u64::from(memory.u32(u64::from(buffer) + 4 * i));
                let mut registers = vec![(2, 1), (30, word(memory, 2))];
                registers.extend((0..8).map(|n| (16 + n, word(memory, 3 + u64::from(n)))));
                Delivery {
                    entry: word(memory, 0),
                    args: [1, 0, 0, 0],
                    stack: Some(word(memory, 1)),
                    gp: Some(word(memory, 11)),
                    registers,
                    token,
                }
            }
        })
    }

    /// The end of a step: a verifier that answered gives its handler the answer.
    pub(super) fn end_step(&mut self, result: u64) {
        if let Some(Step::Verify(_, handler)) = self.bios.current.take()
            && result as u32 != 0
            && handler != 0
        {
            self.bios.steps.push_front(Step::Call(handler, result as u32));
        }
        if self.bios.steps.is_empty() {
            self.bios.handling = false;
        }
    }

    /// The kernel's pad driver, once a vertical blank: each port's state into the buffer
    /// `InitPad` gave for it, as a status byte (zero for a pad, `ff` for none), the pad's
    /// identifier and its report.
    fn poll_pads(&mut self, memory: &mut dyn Memory) {
        if self.bios.outdated_pad != 0 {
            let word = self.outdated_buttons();
            memory.set_u32(u64::from(self.bios.outdated_pad), word);
        }
        // The driver writes a pad's whole answer whatever length the program gave: the shell
        // gives two and reads the buttons past them.
        for (port, (buffer, _)) in self.bios.pads.into_iter().enumerate() {
            if buffer == 0 {
                continue;
            }
            let bytes = match self.host.input.pad(port) {
                Some(pad) => [vec![0x00, crate::pad::DIGITAL], pad.report(false)].concat(),
                None => vec![0xff, 0x00, 0xff, 0xff],
            };
            memory.write(u64::from(buffer), &bytes);
        }
    }

    /// Both pads as the kernel's first pad driver gives them: a set bit for a pressed button,
    /// the report's first byte the upper one, the second pad in the upper half.
    fn outdated_buttons(&mut self) -> u32 {
        let pad = |host: &mut crate::Host, port: usize| u32::from(host.input.pad(port).map_or(0, |p| p.buttons.swap_bytes()));
        pad(&mut self.host, 0) | pad(&mut self.host, 1) << 16
    }

    fn tty(&mut self, bytes: &[u8]) {
        self.host.console.write(bytes);
    }

    fn file(&mut self, descriptor: u32) -> Option<&mut (Box<dyn File>, u64)> {
        self.bios.files.get_mut((descriptor as usize).checked_sub(2)?)?.as_mut()
    }

    /// The BIOS function `name` on the words of `a0..a3`.
    pub(super) fn bios_call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop> {
        let a = |i: usize| args.get(i).copied().unwrap_or(0) as u32;
        let value = |v: u32| Ok(Answer::Value(u64::from(v)));
        let (a0, a1, a2) = (a(0), a(1), a(2));
        if crate::tracing() && !matches!(name, "syscall" | "std_out_putchar") {
            eprintln!("bios: {name}({a0:#x}, {a1:#x}, {a2:#x}, {:#x})", a(3));
        }
        match name {
            // The system calls: the critical section's two, which answer whether interrupts
            // were on, and the two that do nothing here.
            "syscall" => match a0 {
                1 => {
                    let was_on = !self.bios.critical;
                    self.bios.critical = true;
                    value(u32::from(was_on))
                }
                2 => {
                    self.bios.critical = false;
                    value(0)
                }
                _ => value(0),
            },
            "exit" | "SystemErrorExit" => Err(Stop::Exit(a0.cast_signed())),
            "SystemErrorBootOrDiskFailure" => {
                Err(Stop::fault(format!("the console could not boot: failure {} with the code {a1:#x}", char::from(a0 as u8))))
            }

            // The TTY and the files.
            "std_out_putchar" => {
                self.tty(&[a0 as u8]);
                value(a0)
            }
            "std_out_puts" => {
                let bytes = text(memory, a0);
                self.tty(&bytes);
                value(0)
            }
            "printf" => {
                let spec = text(memory, a0);
                let mut given = [a1, a2, a(3)].into_iter();
                let out = crate::printf::format(memory, &spec, || given.next().unwrap_or(0));
                self.tty(&out);
                value(out.len() as u32)
            }
            "std_in_getchar" | "std_in_gets" => value(0),
            "FileOpen" => {
                let name = String::from_utf8_lossy(&text(memory, a0)).into_owned();
                if name.starts_with("tty") {
                    return value(1);
                }
                let Some(file) = self.host.storage.open(&name, a1 & 2 != 0) else {
                    self.bios.last_error = 2;
                    return value(u32::MAX);
                };
                let slot = self.bios.files.iter().position(Option::is_none).unwrap_or(self.bios.files.len());
                if slot == self.bios.files.len() {
                    self.bios.files.push(None);
                }
                self.bios.files[slot] = Some((file, 0));
                value(slot as u32 + 2)
            }
            "FileClose" => {
                if let Some(slot) = (a0 as usize).checked_sub(2).and_then(|i| self.bios.files.get_mut(i)) {
                    *slot = None;
                }
                value(a0)
            }
            "FileSeek" => {
                let Some((file, position)) = self.file(a0) else { return value(u32::MAX) };
                let target = if a2 == 1 { position.wrapping_add_signed(i64::from(a1.cast_signed())) } else { u64::from(a1) };
                match file.seek(target) {
                    Some(at) => {
                        *position = at;
                        value(at as u32)
                    }
                    None => value(u32::MAX),
                }
            }
            "FileRead" => {
                if a0 < 2 {
                    return value(0);
                }
                let Some((file, position)) = self.file(a0) else { return value(u32::MAX) };
                let mut buffer = vec![0; a2 as usize];
                let read = file.read(&mut buffer);
                *position += read as u64;
                memory.write(u64::from(a1), &buffer[..read]);
                value(read as u32)
            }
            "FileWrite" => {
                let bytes = memory.bytes(u64::from(a1), a2 as usize);
                if a0 < 2 {
                    self.tty(&bytes);
                    return value(a2);
                }
                let Some((file, position)) = self.file(a0) else { return value(u32::MAX) };
                let written = file.write(&bytes);
                *position += written as u64;
                value(written as u32)
            }
            "FileGetc" => {
                let Some((file, position)) = self.file(a0) else { return value(u32::MAX) };
                let mut byte = [0; 1];
                if file.read(&mut byte) == 0 {
                    return value(u32::MAX);
                }
                *position += 1;
                value(u32::from(byte[0]))
            }
            "FilePutc" => {
                if a1 < 2 {
                    self.tty(&[a0 as u8]);
                }
                value(a0)
            }
            "get_last_error" => value(self.bios.last_error),

            // The C library's part.
            "todigit" => value((a0 as u8 as char).to_digit(36).unwrap_or(9_999_999)),
            "abs" | "labs" => value(a0.cast_signed().unsigned_abs()),
            "atoi" | "atol" => value(number(memory, a0, 10).0),
            "strtol" | "strtoul" => {
                let (parsed, end) = number(memory, a0, a2);
                if a1 != 0 {
                    memory.set_u32(u64::from(a1), end);
                }
                value(parsed)
            }
            "toupper" => value(u32::from((a0 as u8).to_ascii_uppercase())),
            "tolower" => value(u32::from((a0 as u8).to_ascii_lowercase())),
            "strlen" => value(text(memory, a0).len() as u32),
            "strcpy" => {
                let bytes = text(memory, a1);
                put_text(memory, a0, &bytes);
                value(a0)
            }
            "strncpy" => {
                let mut bytes = text(memory, a1);
                bytes.resize(a2 as usize, 0);
                memory.write(u64::from(a0), &bytes);
                value(a0)
            }
            "strcat" | "strncat" => {
                let mut tail = text(memory, a1);
                if name == "strncat" {
                    tail.truncate(a2 as usize);
                }
                let end = a0 + text(memory, a0).len() as u32;
                put_text(memory, end, &tail);
                value(a0)
            }
            "strcmp" | "strncmp" => {
                let (mut left, mut right) = (text(memory, a0), text(memory, a1));
                if name == "strncmp" {
                    left.truncate(a2 as usize);
                    right.truncate(a2 as usize);
                }
                value((left.cmp(&right) as i32).cast_unsigned())
            }
            "strchr" | "index" | "strrchr" | "rindex" => {
                let bytes = text(memory, a0);
                let found = if name.starts_with('r') || name == "strrchr" {
                    bytes.iter().rposition(|b| *b == a1 as u8)
                } else {
                    bytes.iter().position(|b| *b == a1 as u8)
                };
                value(found.map_or(0, |i| a0 + i as u32))
            }
            "strpbrk" | "strspn" | "strcspn" => {
                let (bytes, set) = (text(memory, a0), text(memory, a1));
                let run = bytes.iter().take_while(|b| set.contains(b) == (name == "strspn")).count();
                match name {
                    "strpbrk" if run < bytes.len() => value(a0 + run as u32),
                    "strpbrk" => value(0),
                    _ => value(run as u32),
                }
            }
            "strstr" => {
                let (bytes, wanted) = (text(memory, a0), text(memory, a1));
                let found = if wanted.is_empty() { Some(0) } else { bytes.windows(wanted.len()).position(|w| w == wanted) };
                value(found.map_or(0, |i| a0 + i as u32))
            }
            "bcopy" | "memcpy" | "memmove" => {
                // `bcopy` takes the source first.
                let (to, from) = if name == "bcopy" { (a1, a0) } else { (a0, a1) };
                let bytes = memory.bytes(u64::from(from), a2 as usize);
                memory.write(u64::from(to), &bytes);
                value(a0)
            }
            "bzero" => {
                memory.write(u64::from(a0), &vec![0; a1 as usize]);
                value(a0)
            }
            "memset" => {
                memory.write(u64::from(a0), &vec![a1 as u8; a2 as usize]);
                value(a0)
            }
            "bcmp" | "memcmp" => {
                let (left, right) = (memory.bytes(u64::from(a0), a2 as usize), memory.bytes(u64::from(a1), a2 as usize));
                value((left.cmp(&right) as i32).cast_unsigned())
            }
            "memchr" => {
                let bytes = memory.bytes(u64::from(a0), a2 as usize);
                value(bytes.iter().position(|b| *b == a1 as u8).map_or(0, |i| a0 + i as u32))
            }
            "rand" => {
                self.bios.seed = self.bios.seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
                value((self.bios.seed >> 16) & 0x7fff)
            }
            "srand" => {
                self.bios.seed = a0;
                value(0)
            }

            // The heaps.
            "InitHeap" => {
                self.bios.heap = (a0, a0.wrapping_add(a1 & !3));
                self.bios.blocks.clear();
                value(0)
            }
            "malloc" => value(self.bios.allocate(a0)),
            "calloc" => {
                let size = a0.wrapping_mul(a1);
                let at = self.bios.allocate(size);
                if at != 0 {
                    memory.write(u64::from(at), &vec![0; size as usize]);
                }
                value(at)
            }
            "realloc" => {
                let old = self.bios.blocks.remove(&a0).unwrap_or(0);
                let at = self.bios.allocate(a1);
                if at != 0 && a0 != 0 {
                    let kept = memory.bytes(u64::from(a0), old.min(a1) as usize);
                    memory.write(u64::from(at), &kept);
                }
                value(at)
            }
            "free" => {
                self.bios.blocks.remove(&a0);
                value(0)
            }
            "alloc_kernel_memory" => {
                let at = self.bios.kernel_next;
                let size = a0.next_multiple_of(4);
                if KERNEL_HEAP_END - at < size {
                    return value(0);
                }
                self.bios.kernel_next += size;
                value(at)
            }

            // The events.
            "OpenEvent" => {
                let event = Event { class: a0, spec: a1, mode: a2, function: a(3), status: EVENT_DISABLED };
                let slot = self.bios.events.iter().position(|e| e.status == EVENT_FREE).unwrap_or(self.bios.events.len());
                if slot == self.bios.events.len() {
                    self.bios.events.push(event);
                } else {
                    self.bios.events[slot] = event;
                }
                value(EVENT_HANDLES + slot as u32)
            }
            "CloseEvent" | "EnableEvent" | "DisableEvent" => {
                let Some(event) = self.bios.event(a0) else { return value(0) };
                event.status = match name {
                    "CloseEvent" => EVENT_FREE,
                    "EnableEvent" => EVENT_ENABLED,
                    _ => EVENT_DISABLED,
                };
                value(1)
            }
            "TestEvent" => {
                // A ready event is taken by the test.
                let ready = match self.bios.event(a0) {
                    Some(event) if event.status == EVENT_READY => {
                        event.status = EVENT_ENABLED;
                        true
                    }
                    _ => false,
                };
                value(u32::from(ready))
            }
            "WaitEvent" => {
                let Some(index) = a0.checked_sub(EVENT_HANDLES).map(|i| i as usize).filter(|i| *i < self.bios.events.len()) else {
                    return value(0);
                };
                self.bios.waiting = Some(index);
                match self.event_came() {
                    Some(result) => value(result as u32),
                    None => Ok(Answer::Wait),
                }
            }
            "DeliverEvent" => {
                for function in self.bios.deliver_event(a0, a1) {
                    self.bios.steps.push_back(Step::Call(function, 0));
                }
                value(0)
            }
            "UnDeliverEvent" => {
                for event in self.bios.events.iter_mut().filter(|e| e.status == EVENT_READY && e.class == a0 && e.spec == a1) {
                    event.status = EVENT_ENABLED;
                }
                value(0)
            }

            // The pads and the interrupt plumbing.
            "InitPad" => {
                self.bios.pads = [(a0, a1), (a2, a(3))];
                // No pad has answered yet: the buffers say so until the first poll.
                for (buffer, len) in self.bios.pads {
                    memory.write(u64::from(buffer), &vec![0xff; (len as usize).max(4)]);
                }
                value(2)
            }
            "OutdatedPadInitAndStart" => {
                self.bios.outdated_pad = a1;
                self.bios.pads_started = true;
                self.i_mask |= irq::VBLANK;
                value(0)
            }
            "OutdatedPadGetButtons" => {
                let word = self.outdated_buttons();
                value(word)
            }
            "StartPad" => {
                self.bios.pads_started = true;
                self.i_mask |= irq::VBLANK;
                value(2)
            }
            "StopPad" => {
                self.bios.pads_started = false;
                value(0)
            }
            "ChangeClearPad" => {
                self.bios.clear_pad = a0 != 0;
                value(0)
            }
            "ChangeClearRCnt" => {
                let slot = (a0 & 3) as usize;
                value(u32::from(std::mem::replace(&mut self.bios.clear_counter[slot], a1 != 0)))
            }
            "SetCustomExitFromException" => {
                self.bios.hook = Some(a0);
                value(0)
            }
            "SetDefaultExitFromException" => {
                self.bios.hook = None;
                value(0)
            }
            "ReturnFromException" => {
                // Whatever the handler was called for, the interrupt is over.
                self.bios.steps.clear();
                Ok(Answer::Leave)
            }
            "SysEnqIntRP" => {
                self.bios.chains[(a0 & 3) as usize].insert(0, a1);
                value(0)
            }
            "SysDeqIntRP" => {
                self.bios.chains[(a0 & 3) as usize].retain(|e| *e != a1);
                value(0)
            }

            // The root counters: the three timers, and the vertical blank as the fourth.
            "SetRCnt" => {
                let n = (a0 & 3) as usize;
                if n < 3 {
                    // The mode's bits: an interrupt at the target (0x1000), the system clock
                    // (0x0001, the scanline or an eighth of the clock without it).
                    let source = if a2 & 1 != 0 { 0 } else { 0x100 << u32::from(n == 2) };
                    let mode = 0x48 | source | if a2 & 0x1000 != 0 { 0x10 } else { 0 };
                    self.timer_write(n as u64 * 0x10 + 8, a1);
                    self.timer_write(n as u64 * 0x10 + 4, mode);
                }
                value(1)
            }
            "GetRCnt" => value(if a0 & 3 < 3 { self.timer_read(u64::from(a0 & 3) * 0x10) } else { 0 }),
            "ResetRCnt" => {
                if a0 & 3 < 3 {
                    self.timer_write(u64::from(a0 & 3) * 0x10, 0);
                }
                value(1)
            }
            "StartRCnt" | "StopRCnt" => {
                let line = if a0 & 3 == 3 { irq::VBLANK } else { irq::TIMER0 << (a0 & 3) };
                if name == "StartRCnt" {
                    self.i_mask |= line;
                } else {
                    self.i_mask &= !line;
                }
                value(1)
            }

            // What the kernel does to itself, which is nothing here; no file is found by a
            // search and no memory card is in a slot.
            "FlushCache"
            | "InitCard"
            | "StartCard"
            | "StopCard"
            | "EnqueueTimerAndVblankIrqs"
            | "EnqueueSyscallHandler"
            | "InstallExceptionHandlers"
            | "SysInitMemory"
            | "InitDefInt"
            | "InstallDevices"
            | "FlushStdInOutPut"
            | "AdjustA0Table"
            | "free_kernel_memory"
            | "GetC0Table"
            | "GetB0Table"
            | "strtok"
            | "_96_init"
            | "_96_remove"
            | "_bu_init"
            | "SetMemSize"
            | "GetSystemInfo"
            | "firstfile"
            | "nextfile"
            | "chdir"
            | "AddDevice"
            | "RemoveDevice"
            | "PrintInstalledDevices"
            | "FileDelete"
            | "FileRename"
            | "FileUndelete"
            | "FormatDevice"
            | "set_card_auto_format"
            | "card_write_test"
            | "_new_card"
            | "_card_chan"
            | "_card_status"
            | "_card_wait" => value(0),
            // A memory card's slot is asked and answers later by an event: with no card in it,
            // the one that says the card did not answer.
            "_card_info" | "_card_async_load_directory" | "_card_write" | "_card_read" => {
                for function in self.bios.deliver_event(CLASS_CARD, SPEC_TIMEOUT) {
                    self.bios.steps.push_back(Step::Call(function, 0));
                }
                value(1)
            }
            "GPU_cw" => {
                self.gpu.gp0(a0);
                value(0)
            }
            _ => Err(Stop::fault(format!("the BIOS function {name} is not answered"))),
        }
    }

    /// The result of the `WaitEvent` in flight, once its event is ready.
    pub(super) fn event_came(&mut self) -> Option<u64> {
        let index = self.bios.waiting?;
        let event = self.bios.events.get_mut(index)?;
        match event.status {
            EVENT_READY => {
                event.status = EVENT_ENABLED;
                self.bios.waiting = None;
                Some(1)
            }
            // A closed or disabled event never comes: the wait fails.
            EVENT_FREE | EVENT_DISABLED => {
                self.bios.waiting = None;
                Some(0)
            }
            _ => None,
        }
    }

    /// The program's RAM address of a DMA address: the 2 MiB, at `kseg0`.
    pub(super) fn ram(address: u32) -> u64 {
        RAM | u64::from(address & 0x1f_fffc)
    }
}
