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

Beside the three events a console says how its memory lies (`Platform::map`), which is all a
carrier has to know of it to put the machine behind a program:

| Of the `Map` | What |
| --- | --- |
| `order` | the order of a word's bytes in the console's memory and on its bus |
| `ports` | every run of addresses the device registers answer at, each mirror a run of its own |
| `ram`, `repeats` | the segments the RAM answers through beside the one the program is linked in, and how many times in a row it answers in each |
| `extras` | the memory beside the RAM (a scratchpad, the video memory), with its mirrors |

`Platform::start` is what the console's own start leaves the program (a boot program's report),
`Platform::cartridge` takes the ROM in the slot, and `Platform::interrupt` is the level of the
processor's interrupt line, for a console whose program takes its interrupts itself.

## What the consoles share

| Module | What |
| --- | --- |
| `host` | `Video`, `Audio`, `Input`, `Storage`, `Console`: what a runtime asks of this machine. `Capture` is the headless form of all of them (frames kept or written as PNG, samples kept, a pad script, the console's text); `Directory` is a disc image or a folder as the console's files |
| `window` | the host on Windows: a window, the keyboard and an XInput gamepad, wave-out sound, on the system's own calls |
| `clock` | the scheduler of the devices' events against the cycle count |
| `raster` | which pixels a triangle, a line or a rectangle covers and each pixel's weights; a console's closure makes the pixel |
| `scanout` | a frame buffer in the program's memory as the picture a video output makes of it: the pixel formats of the consoles, the bytes between two rows, either byte order, the two fields of an interlaced picture |
| `pad` | the controller both PlayStations take: its state, its report, its serial protocol; the other consoles' controllers are made of its state (`n64::controller`, `psp::buttons`, `dreamcast::condition`) |
| `adpcm` | the sound processors' ADPCM blocks |
| `disc` | a compact disc as the files a dump of one is kept in: a cue sheet with one file or one a track, or a bare `.bin`, `.img` or `.iso`; its tracks, and each sector as the 2352 bytes the drive reads |
| `iso` | a disc's ISO 9660 file system, over its user data |
| `png`, `printf` | frames as PNG (written stored, any 8-bit RGB or RGBA read), and the BIOS's `printf` |

A new console is a module that lays its devices over these and implements `Platform`; `of(name,
host)` names it by what `Image::platform` says, and `NAMES` lists the consoles that have one.
Nothing else names a console: the interpreter maps the registers, the RAM's mirrors and the
extra memory from the `Map`, so a console whose processor lifts runs as soon as its module is
there.

What every console has, and what is its own:

| Console | Registers | Picture | Input | Sound | Text | Interrupts | Kernel | Runs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `psx` | all | the GPU, in software | pad, memory cards | SPU, XA, CD-DA | TTY | its kernel's, or delivered | answered, or a ROM's own | homebrew, a BIOS ROM, a commercial disc |
| `ps2` | the EE's | the GS, in software | none (the IOP) | none (the IOP) | serial port | delivered | the EE's, answered | a program from `main` |
| `n64` | the RCP's, the PIF | the frame buffer the VI shows | controllers, through the PIF | the AI's buffers | ISViewer | the VR4300's exception | none on the console | libdragon homebrew that draws itself |
| `psp` | none in user mode | the frame buffer the display shows | the controller | the sound channels | standard output | none in user mode | the display, controller, sound and clock, answered | homebrew that draws itself |
| `dreamcast` | Holly's, the SH-4's timers and serial port | the frame buffer the video output shows | controllers, on the Maple bus | none (the AICA) | serial port | the line's level | none on the console | at its registers: no carrier runs an SH-4 program |

## The consoles

**PlayStation (`psx`).** The interrupt controller, the DMA channels (the GPU's blocks and linked
lists, the ordering table's clear, the SPU's, the CD-ROM's and the MDEC's two), the three timers,
the controller port (the pad's byte protocol, and the memory card's: a card is the file
`mc/card0.mcd` or `mc/card1.mcd` of the `--files` directory, a raw image, formatted when new;
without `--files` the slots are empty),
the macroblock decoder (its tables, colour and monochrome macroblocks at every depth), the CD-ROM
controller (commands, answers as interrupts, sector
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

**Nintendo 64 (`n64`).** The RCP behind its registers, in the words' big-endian order: the MIPS
interface's interrupt register and its mask; the video interface, which shows the 16- or 32-bit
frame buffer at its origin in RDRAM, as wide and high as its scales make the lines it scans, and
interrupts once a frame; the audio interface, which plays each buffer it is handed at the DAC's
rate and holds one more; the peripheral interface's transfers between the cartridge's ROM and
RDRAM, with the `ISViewer` in the cartridge's domain as the console's text; the serial interface
with the PIF behind it, whose RAM is run as joybus frames: a controller in each port the host
has a pad in, what it is and its buttons and stick. The RSP's memories keep what is written and
its DMA copies; a task started on it ends at once, and the RDP's commands are read for the full
sync alone. The boot code's report is in the RSP's data memory at the start (`Platform::start`).
`testdata/n64/draw.z64` is the fixture: libdragon's display, graphics and joypad modules.

**PSP (`psp`).** A user module sees no register, so the console is its calls: the display
(`sceDisplaySetMode`, `sceDisplaySetFrameBuf` in the four pixel formats, the waits for the
vertical blank, the frame count), the video memory with its uncached mirror (`sceGeEdramGetAddr`),
the controller (`sceCtrlReadBufferPositive` and its kin), the eight sound channels and the output
at a rate of the program's own (a blocking output waits for the channel), the clock, the
callbacks a program registers to be told to exit, and the cache calls. The threads, the
semaphores, the mutexes and the memory blocks stay the interpreter's (`run/host.rs`), which
sends a function it does not answer itself here. `testdata/psp/draw.prx` is the fixture.

**Dreamcast (`dreamcast`).** Holly: the three interrupt status registers and the masks of the
three levels; the Maple bus, whose command table is run from RAM (a controller in each port the
host has a pad in answers what it is and its condition, nothing else answers); the PowerVR's
video output, which shows the frame buffer its read registers name in the video memory in any of
the four depths, the two fields by turns when interlaced, with the blank's two interrupts and the
beam's status; the clock. The SH-4's own: the three timers on the peripheral clock, and the
serial port with its FIFO, which is the console's text. A render a program starts ends at once
with its interrupts.

## Carriers

**The interpreter** (`crates/ra-asm-lift/src/run/console.rs`). `ras run` on an image of a
console in `NAMES` whose processor lifts (`psx`, `ps2`, `n64`, `psp`) runs it against its
machine: the registers are devices of the interpreter's memory at every run of the map's ports,
in the console's byte order, `ra_bios_*` and `ra_ee_*` calls and a PSP module's imports go to
the runtime, the clock moves two cycles a processor instruction, and deliveries are calls into
the lifted module on a state of their own.

A Nintendo 64 program has no kernel to deliver to: the RCP's interrupt is the VR4300's
(`vr4300_line`). The line's level is the cause register's pending bit, and with the status
register's interrupts on, the line let through and no exception being handled, the run sets
the exception level and calls the function the code at the general vector jumps to (a `j`, or
an address built in a register and a `jr`), on the registers of the code it interrupts, as the
processor enters it. The handler saves and restores what it uses, and its `eret` is the
function's return. The analysis seeds that function from the vectors of the image
(`ra-asm-analysis`), since nothing of the program calls it.

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

**The port** (`ras port`, `crates/ra-asm-lift/src/port.rs`, `crates/ra-asm-port`). A console's
program as a program of this machine: the image lifted whole at the machine level, compiled, and
run in the interpreter's own run with the compiled functions in place of their IR. The machine
level is the one a kernel needs: an interrupt and a thread switch save and restore the whole
state, which only that level keeps in memory.

```
ras port bios.bin --out rebio      program.ll, port.c, image.bin, the run's library, rebio.exe
rebio.exe                          a window; --disc, --files, --headless, --frames, --frame-dir
```

- `program.ll` is the module for a compiler: each lifted function `void f(ptr state)`, every
  access to the program's memory a call (`ra_ld<bits>`, `ra_st<bits>`), and a look at the
  machine where each block of the program starts (`ra_tick`), which counts the block's
  instructions for the console's clock and is where an interrupt is taken.
- `port.c` ties it to the run: the accessors, which read and write the console's RAM
  themselves (through any segment and mirror) and ask the run for the rest (a device's
  register, the ROM, a range of RAM the run watches); the runtime's functions the lift names,
  each through the run's one entry by its number; the table of the functions by address; the
  image's bytes; and `main`.
- `ra_asm_port.dll` is the run: it lifts the image again as it starts (for the functions'
  names, the code that lives in RAM and the IR of any function the port lacks), gives each
  function its compiled code (`Vm::set_native`), and runs it as `ras run` does. A call back
  that stops the run (a fault, an exit, a handler that leaves its exception for another frame)
  unwinds through the compiled frames to the call that entered them.

The run looks at its machine every 256 instructions the compiled code counts, at once after a
store to a device's register (so the write is the device's before the next read of it), and an
interrupt is taken at the next block's start, not between two instructions of one.

`ras emit` still writes a console's raised C over the stub runtime of `runtime/rhipper.h`, which
logs each call: the raised level keeps no state in memory an interrupt could save, so it is
the port of a program, not of a kernel.

## The PlayStation's own BIOS as a program

A BIOS ROM is an image too (`psx-bios`): 512 KiB (or a replacement's 256 KiB, or 1 MiB with a
second ROM) whose kernel header at `0x100` holds a date and a maker's name, or, for a ROM of
another maker with code there, whose reset code sets the memory control's registers; mapped whole
at `0xbfc00000` and entered at the reset vector.

`ras run` on a ROM runs it whole from its reset vector, its kernel its own, the console's ROM as
a replacement's (OpenBIOS). Its lift names no BIOS function: a jump to `0xa0` is a jump, `syscall`
and `break` are the exceptions they are (`ra_psx_exception`, which also says when the instruction
is in a delay slot), and the run takes them as the processor does: the cause and the return
address into coprocessor 0, the status register's mode pushed, and the code at the vector run
until it jumps back to where the exception was taken. A handler that leaves for somewhere else
(another thread, a jump buffer, a handler of the program's own) ends the frames of the run down
to its first, and the processor goes on there. The processor of such a run raises the address
and bus errors too, and an arithmetic overflow. Interrupts are the level of the console's
interrupt line (`Platform::interrupt`) in the cause register, taken between two of the program's
instructions, lifted (`Host::boundary`) or the processor's, and `Platform::deliver` is not asked.
A write of the status register that cuts the data cache off (`ra_psx_status`) keeps the RAM as
it is until the cache is back, since the stores that flush the cache reach no memory. The
kernel's TTY is a development board's serial port, so what a program prints is taken where it
enters the kernel's `putchar`, `puts` or `write`.

`ras run --bios <rom> program.exe` runs a program over a ROM's kernel in place of the answered
one: the run starts at the ROM's reset vector, and where the kernel would enter its shell the
program is loaded and entered in the shell's stead, as the kernel's `Exec` starts one. The
program's `syscall` and `break` are the kernel's exceptions then, and its interrupt handlers the
kernel's to call. Without `--bios` the kernel's functions are answered by the runtime.

`ras info` prints what the ROM says of itself (the shell's version string, the kernel's name and
date). The R3000's coprocessor 0 (`mfc0`, `mtc0`, `rfe`) decodes and lifts for a `psx` image, the
registers a global as the VR4300's are.

**The programs a ROM keeps for RAM.** The ROM holds two more programs, which its start code
copies into RAM and runs there: the kernel and the shell. Each is a second image beside the
ROM's, as a DLL is beside a program's (`Companion`), lifted with it into one module
(`Image::psx_bios_parts`, `ra_asm_lift::Whole`):

- Their places are read from the copies themselves (`Bios::copies`): the ROM's address of the
  bytes, the RAM's and the length, built in the three argument registers within a few
  instructions of each other, of a run of bytes the code then jumps into through a register.
- A part is mapped at its `kseg0` address, and linked for the segment most of the addresses its
  code builds name it through (`Image::link_segment`): the kernel is linked for `kuseg`, so its
  `lui`/`addiu`/`jr` to `0x598` is a jump to its own code at `0x80000598`.
- A `jr` through a register the code built an address of the image in is a jump there; and the
  code of a kernel's vectors, which it keeps as bytes it copies to where the processor enters
  them (`lui r; addiu r, r, lo; jr r`), names its handlers wherever the three words lie.
- A function of a part is the code at its address only while the RAM there holds the bytes it
  was lifted from (`Lifted::resident`, `Memory::watch`): what a game loads over the shell is
  the processor's. An address is folded to the one its code was lifted at, whatever segment or
  mirror names it (`psx_lifted`).
- A lifted `jr` with `rfe` in its slot is the return of an exception's handler
  (`ra_psx_return`), and a lifted function that comes back from a call with another `ra` than
  it was called with left through a jump buffer (`psx_returned`): in both, where the code does
  not go on where it was, the processor goes on from the run's first frame.

The target ROM is `System ROM Version 2.2 12/04/95 A` (kernel `CEX-3000/1001/1002 by K.S.`,
1995-12-04), whose kernel every later console ROM kept: 278 functions of the ROM, 200 of the
kernel and 550 of the shell. What stays the processor's, about two instructions in a hundred to
the menu: the few words of each vector the kernel copies into low RAM, and the rest of a function
the processor went on in the middle of (after an interrupt whose handler left through a jump
buffer, as the shell's do), up to its return. `RA_RUN_STATS=1` says how many and from where;
`RA_PROCESSOR_TRAIL=1` with `RA_PLATFORM_TRACE=1` prints the last crossings between lifted code
and the processor when a reserved instruction's exception is taken.

## The reference tests

`C:\rhipper\ref\pcsx-redux` is a reference emulator's source (GPL: read for what the hardware
does, never copied). Its `src/mips` holds OpenBIOS and 156 hardware test programs, which build
with the MIPS toolchains in WSL (`C:\rhipper\ref\build.sh`, and `toolchain.sh` for the ones in
C++) and run through `ras run` (`C:\rhipper\ref\run-tests.ps1`, a summary line per test). A test
ends itself and prints through the development board's ports at `0x1f802082` and `0x1f802080`,
which the PlayStation runtime answers.

## What is open

- The port: its run lifts the image again as it starts (a second, where the tables of the
  functions and of the code in RAM could come with the port); a port is a console ROM's (a
  program over the answered kernel has the machine call its handlers, which a compiled function
  cannot be left for in the middle of); and its speed: the reference ROM's port runs at about
  a hundred frames a second through the intro and fifty at the menu, headless, and where a
  frame's time goes there is not measured. The raised
  C of `ras emit` runs over the stub: the runtime behind `ra_platform`, and the lift naming the
  PlayStation's device accesses as it names the PlayStation 2's, are for the port of a
  program.
- PlayStation's drive: the sound sectors of a data track (XA) and the sound tracks are read
  and not heard; a CHD image is not read (its hunks are LZMA, deflate and FLAC). A program
  loaded from a disc is the processor's, not lifted, and the kernel's lifted functions are left
  once a program patches the kernel in RAM: a game runs at about half the console's speed.
- PlayStation: a program's own exception handlers over the answered kernel (a program that
  writes the vector at `0x80000080` is never entered there; over a ROM's kernel, with `--bios`,
  it is). The devices answer at once: the reference tests of the bus's, the GPU's and the
  CD-ROM's timing fail on it. The MDEC takes a command's data whole before it decodes it.
- PlayStation 2: ps2sdk's start routine stops at its first `mfc0`/`di` (the R5900's rows have no
  coprocessor 0), so a program runs from `main`, as the fixture does. The IOP and its modules,
  and so the pads, SPU2 and files, which go through the SIF; the VIFs and the vector units'
  microprograms; the IPU. The GS's local memory is laid out a row after a row, not in the
  hardware's pages and blocks, which shows where a program reads a buffer in another format
  than it wrote it.
- Nintendo 64: the RSP runs no task (one a program starts ends at once, so sound mixed and
  display lists built on it come to nothing) and the RDP draws nothing: a program that draws
  through `rdpq` shows what it cleared its buffer to. The processor's timer interrupt (`Count`
  against `Compare`) is not raised, the cartridge has no save memory, and the 64DD is absent.
  A program that puts no jump at the general vector takes no interrupt.
- PSP: the graphics engine runs no display list (`sceGeListEnQueue` stops the run, so `sceGu`
  programs do), the media engine and the files beside the standard output are not there, and a
  callback is never called. A function's arguments past the fourth (`t0` and up) do not reach
  the runtime.
- Dreamcast: no carrier runs an SH-4 program (the decoder is there, the lift is not), so the
  runtime is tested at its registers. The tile accelerator and the renderer, the AICA and its
  processor, the GD-ROM drive and the boot ROM's system calls are not there.
- The Xbox (`xbox` images load and lift) has no runtime: its kernel's exports and the NV2A.
- A port (`ras port`) is a PlayStation ROM's: the other consoles run in the interpreter alone.
- Threads run nested in the thread that started them: a thread that waits for the one below it
  never wakes.
- A function only an address reaches (a handler, a callback) is found on a PS-X EXE by the
  `nm` symbol list beside it (`<name>.map`), or by the address its code builds; one the analysis
  misses runs on the processor, not lifted. A PlayStation 2 program's run still stops there.
