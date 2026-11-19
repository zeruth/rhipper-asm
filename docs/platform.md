# Platform runtimes

A console's program is half software and half machine. `crates/ra-asm-platform` is the machine's
half: the devices behind the console's registers and the BIOS or kernel the program calls, over
this machine's picture, sound, input and files. It is no emulator: the program's own code runs as
the interpreter (or, later, the port) runs it, and only what the program reaches outside itself
is here.

## The boundary

Every console crosses the same three events, which the `Platform` trait fixes:

| Event | What it is | In the trait |
| --- | --- | --- |
| call | a BIOS or kernel function, by its name, on its register arguments | `call`, and `resume` for one that waits |
| access | a read or write of a device register, at the address the memory map folds it to | `fold`, `read`, `write` |
| deliver | what the machine does to the program: an interrupt handler, a thread's entry, a callback | `advance`, `deliver`, `delivered` |

A runtime never runs the program's code itself. When an interrupt comes due it hands the run a
`Delivery` (an entry, its arguments, a stack), the run makes the call, and tells the runtime when
it ended; a kernel function that waits answers `Answer::Wait` and the run lets the clock go on
until `resume` gives its result. So the same runtime sits behind any carrier of the program.

Time is the machine's cycles, and only moves when the carrier says so (`advance`), by the
instructions the program ran: a run is the same run every time.

## What the consoles share

| Module | What |
| --- | --- |
| `host` | `Video`, `Audio`, `Input`, `Storage`, `Console`: what a runtime asks of this machine. `Capture` is the headless form of all of them (frames kept or written as PNG, samples kept, a pad script, the console's text); `Directory` is a disc image or a folder as the console's files |
| `window` | the host on Windows: a window, the keyboard and an XInput gamepad, wave-out sound, on the system's own calls |
| `clock` | the scheduler of the devices' events against the cycle count |
| `raster` | which pixels a triangle, a line or a rectangle covers and each pixel's weights; a console's closure makes the pixel |
| `pad` | the controller both PlayStations take: its state, its report, its serial protocol |
| `adpcm` | the sound processors' ADPCM blocks |
| `iso` | a disc as an ISO 9660 image, cooked or raw |
| `png`, `printf` | frames as PNG (written stored, any 8-bit RGB or RGBA read), and the BIOS's `printf` |

A new console is a module that lays its devices over these and implements `Platform`; `of(name,
host)` names it by what `Image::platform` says.

## The consoles

**PlayStation (`psx`).** The interrupt controller, the DMA channels (the GPU's blocks and linked
lists, the ordering table's clear, the SPU's and the CD-ROM's), the three timers, the controller
port (the pad's byte protocol), the CD-ROM controller (commands, answers as interrupts, sector
reads off the host's disc), the GPU (every `GP0` drawing command in software: fills, flat, shaded
and textured polygons, lines and polylines, rectangles, the three copies, the drawing environment,
dithering, the four blends, the mask bit; `GP1`'s display side) and the SPU (24 ADPCM voices with
their envelopes and the reverb unit, mixed at 44100 Hz). The BIOS: the `A0`, `B0` and `C0` functions by
name (the TTY and `printf`, files through the host's storage, the string and memory functions, the
heap, events, the pad driver, the root counters, the handler chains, the exit hook) and the two
critical-section system calls.

**PlayStation 2 (`ps2`).** The EE's side: the interrupt controller, the timers, the serial port,
the GIF with its DMA channel (normal and chained), the graphics synthesizer (the GIF's three packet
forms, the general registers, points, lines, strips, fans and sprites, shading, texturing with the
texture functions and the two indexed formats, the scissor, the alpha, destination-alpha and depth
tests, the blend, host-to-local and local-to-local transfers; its privileged registers and the
picture they select) and the EE kernel's functions by name (the start routine's stack and heap,
threads, semaphores, interrupt and DMA handlers, alarms, the GS's mask and mode).

## Carriers

**The interpreter** (`crates/ra-asm-lift/src/run/console.rs`). `ras run` on a `psx` or `ps2`
image runs it against its machine: the registers are devices of the interpreter's memory at
every mirror, `ra_bios_*` and `ra_ee_*` calls go to the runtime, the clock moves two cycles a
processor instruction, and deliveries are calls into the lifted module on a state of their own.

```
ras run game.exe                          a window; keyboard and gamepad; closes with the window
ras run --frames 8 --frame-dir out/f game.exe
ras run --headless --pad 30:start --pad 60:1:up+cross --disc game.iso game.exe
```

**The processor, for what the lift has no function for** (`crates/ra-asm-lift/src/run/r3000.rs`).
A PlayStation program has code the analysis never sees as a function of the image: code it
copies into RAM and runs there, code it reaches through another segment of the address space,
and what the R3000 does that no function of IR says (a branch in a delay slot). The run has the
processor for it: the instructions at an address run straight from the program's memory over
the same `%State`, with the load delay, from a function's entry to its return. Lifted and
unlifted code call each other: a jump that lands on a lifted function's entry runs the lifted
function, and where the lift cannot go on in a function (`give_up`) it calls the rest of the
function by its address. A call no lifted function answers is no longer the end of a run.

**The port.** Not yet. `ras emit` still writes a console's port over the stub runtime of
`runtime/rhipper.h`, which logs each call. The runtime of a port is this crate behind that
interface.

## The PlayStation's own BIOS as a program

A BIOS ROM is an image too (`psx-bios`): 512 KiB (or a replacement's 256 KiB, or 1 MiB with a
second ROM) whose kernel header at `0x100` holds a date and a maker's name, or, for a ROM of
another maker with code there, whose reset code sets the memory control's registers; mapped whole
at `0xbfc00000` and entered at the reset vector.

`ras run` on a ROM has two forms, by what the ROM is:

- **The shell over the answered kernel.** A ROM whose start code copies a shell into RAM (the
  console's own) runs as that shell, a PS-X EXE of the bytes the copy takes, with the kernel's
  functions answered by the runtime.
- **The ROM itself, its kernel its own.** Any other ROM (OpenBIOS) runs whole from its reset
  vector. Its lift names no BIOS function: a jump to `0xa0` is a jump, `syscall` and `break` are
  the exceptions they are (`ra_psx_exception`), and the run takes them as the processor does:
  the cause and the return address into coprocessor 0, the status register's mode pushed, and the
  code at the vector run until it jumps back to where the exception was taken. Interrupts are
  the level of the console's interrupt line (`Platform::interrupt`), looked at between two
  instructions, and `Platform::deliver` is not asked. A write of the status register that cuts
  the data cache off (`ra_psx_status`) keeps the RAM as it is until the cache is back, since the
  stores that flush the cache reach no memory. What the ROM copies into RAM (its vectors, its
  tables' dispatchers, its shell) is the processor's. `ras info` prints what the ROM says of itself (the
shell's version string, the kernel's name and date). The R3000's coprocessor 0 (`mfc0`, `mtc0`,
`rfe`) decodes and lifts for a `psx` image, the registers a global as the VR4300's are.

The target ROM is `System ROM Version 2.2 12/04/95 A` (kernel `CEX-3000/1001/1002 by K.S.`,
1995-12-04), whose kernel every later console ROM kept. From its reset vector the analysis finds
75 functions, and `ras emit` writes them. What it does not reach yet is the rest of the ROM:

- The kernel is a second program inside the ROM: the reset code copies `0x8bf0` bytes from
  `0xbfc10000` to `0xa0000500` and jumps there, and that code is linked to run in RAM (its first
  instruction jumps to `0x598`). It is a second image beside the ROM's, as a DLL is beside a
  program's (`Companion`), with its place read from the copy loop's own constants.
- The functions of the `A0`, `B0` and `C0` tables and the shell are reached through tables the
  kernel builds in RAM; they are seeds once the tables' ROM sources are read.
- Running it whole is the second form above, which a ROM without a shell copy takes today; the
  console's own ROM still takes the first. Its kernel and its shell both run from RAM, so run
  whole they are all the processor's until the kernel is an image beside the ROM's.

## The reference tests

`C:\rhipper\ref\pcsx-redux` is a reference emulator's source (GPL: read for what the hardware
does, never copied). Its `src/mips` holds OpenBIOS and 156 hardware test programs, which build
with the MIPS toolchains in WSL (`C:\rhipper\ref\build.sh`, and `toolchain.sh` for the ones in
C++) and run through `ras run` (`C:\rhipper\ref\run-tests.ps1`, a summary line per test). A test
ends itself and prints through the development board's ports at `0x1f802082` and `0x1f802080`,
which the PlayStation runtime answers.

## What is open

- The port's runtime: this crate behind `ra_platform` for the C and Rust ports, and the lift
  naming the PlayStation's device accesses as it names the PlayStation 2's.
- PlayStation: the MDEC; memory cards; a program's own exception handlers over the answered
  kernel (a test that writes the vector at `0x80000080` is never entered there); a kernel that
  returns from an exception into another thread than the one it interrupted; a program run over
  a ROM's own kernel in place of the answered one.
- PlayStation 2: ps2sdk's start routine stops at its first `mfc0`/`di` (the R5900's rows have no
  coprocessor 0), so a program runs from `main`, as the fixture does. The IOP and its modules,
  and so the pads, SPU2 and files, which go through the SIF; the VIFs and the vector units'
  microprograms; the IPU. The GS's local memory is laid out a row after a row, not in the
  hardware's pages and blocks, which shows where a program reads a buffer in another format
  than it wrote it.
- Threads run nested in the thread that started them: a thread that waits for the one below it
  never wakes.
- A function only an address reaches (a handler, a callback) is found on a PS-X EXE by the
  `nm` symbol list beside it (`<name>.map`), or by the address its code builds; one the analysis
  misses runs on the processor, not lifted. A PlayStation 2 program's run still stops there.
