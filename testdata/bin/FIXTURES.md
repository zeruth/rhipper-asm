# The fixture binaries

One section per binary of this directory: the source it is built from, its named functions, and
for each function the constructs a lift, a recorded run or a generated vector meets in it. The
x86 and x64 PE sections come from the binary's map and its disassembly (`llvm-objdump -d`); the
foreign sections from the symbol table and the source. A function the compiler inlined has no row:
its code is in its caller's. The `.map` beside a PE is the linker's, and names every function here.

The columns:

- **Switch**: a C `switch` the compiler kept as a jump table (an indirect jump through a table of
  the image), with the jump's address. A `switch` compiled to compares or to a lookup of values
  says so and is not a jump table.
- **Computed**: a call or a jump whose target is a value of the run, not a constant of the image:
  a virtual call, a call through a function pointer or a table of them. A call through an import
  slot, or through an import's address the compiler kept in a register, is not computed.
- **Sign**: a branch on the sign of an integer (`js`, `bgez`, `tbnz #63`, or a signed compare
  with zero), with its address; a `print_int` writes the minus sign behind it.
- **x87**: the x87 instructions the function holds. Only a 32-bit build has them.
- **Globals**: the program's static data the function addresses, which a vector holds as bytes of
  the image; and, marked *(lift)*, the lift's own globals the function uses, which a recorded
  vector holds in the run's globals region from `GLOBALS` (`crates/ra-asm-verify/src/record.rs`):
  the x87 stack for every x87 function, nothing for any other.

A dash is none.

## classes32.exe

`testdata/src/classes.cpp`, x86. Two vtables for `Rect`, and calls through base pointers.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, square, rect, plain |
| describe | 0x401050 | — | virtual calls at 0x401055, 0x40106d, 0x40107c | — | — | — |
| describe_named | 0x401090 | — | virtual call at 0x401095, virtual tail jump at 0x4010ad | — | — | — |
| print_int | 0x4010b0 | — | — | js 0x4010f0 | — | out |
| print_str | 0x401130 | — | — | — | — | out |
| Square::area | 0x401160 | — | — | — | — | — |
| Square::name | 0x401170 | — | — | — | — | — |
| Shape::scale | 0x401180 | — | — | — | — | — |
| Rect::area | 0x401190 | — | — | — | — | — |
| Rect::name | 0x4011a0 | — | — | — | — | — |
| Rect::scale | 0x4011b0 | — | — | — | — | — |
| Rect::label | 0x4011c0 | — | — | — | — | — |
| Rect::weight | 0x4011d0 | — | — | — | — | — |
| Named::label | 0x4011e0 | — | — | — | — | — |
| Named::weight | 0x4011f0 | — | — | — | — | — |

## classes64.exe

`testdata/src/classes.cpp`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, square, rect, plain |
| describe | 0x140001070 | — | virtual calls at 0x14000107b, 0x140001098, 0x1400010ac | — | — | — |
| describe_named | 0x1400010c0 | — | virtual call at 0x1400010cb, virtual tail jump at 0x1400010ec | — | — | — |
| print_int | 0x1400010f0 | — | — | js 0x14000113d | — | out |
| print_str | 0x140001180 | — | — | — | — | out |
| Square::area | 0x1400011d0 | — | — | — | — | — |
| Square::name | 0x1400011e0 | — | — | — | — | — |
| Shape::scale | 0x1400011f0 | — | — | — | — | — |
| Rect::area | 0x140001200 | — | — | — | — | — |
| Rect::name | 0x140001210 | — | — | — | — | — |
| Rect::scale | 0x140001220 | — | — | — | — | — |
| Rect::label | 0x140001230 | — | — | — | — | — |
| Rect::weight | 0x140001240 | — | — | — | — | — |
| Named::label | 0x140001250 | — | — | — | — | — |
| Named::weight | 0x140001260 | — | — | — | — | — |

## computed32.exe

`testdata/src/computed.c`, x86. `h_blend`, `h_fold` and `h_weigh` are named by the map alone:
nothing in the image refers to them, and only a run finds them as `apply`'s targets.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| print_int | 0x401000 | — | — | js 0x401050 | — | out |
| h_blend | 0x401083 | — | — | — | — | — |
| h_fold | 0x40108d | — | — | — | — | — |
| h_weigh | 0x40109c | — | — | — | — | — |
| mainCRTStartup | 0x4010b0 | — | — | — | — | out |
| apply | 0x401160 | — | call of the resolved pointer at 0x40116f | — | — | — |
| resolve | 0x401180 | — | — | — | — | key, masked_offsets |

## computed64.exe

`testdata/src/computed.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| print_int | 0x140001000 | — | — | js 0x14000104d | — | out |
| h_blend | 0x14000108c | — | — | — | — | — |
| h_fold | 0x140001092 | — | — | — | — | — |
| h_weigh | 0x14000109a | — | — | — | — | — |
| mainCRTStartup | 0x1400010b0 | — | — | — | — | out |
| apply | 0x140001160 | — | tail jump to the resolved pointer at 0x14000117a | — | — | — |
| resolve | 0x140001180 | — | — | — | — | key, masked_offsets |

## control32.exe

`testdata/src/control.c`, x86. The four `ops` calls of `mainCRTStartup` are direct: the compiler
read the constant table.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, table, names, sink, write_slot, write_proc |
| print_slot | 0x4012a0 | — | call through write_slot at 0x4012bf | — | — | out, write_slot |
| print_proc | 0x4012d0 | — | call through write_proc at 0x4012ef | — | — | out, write_proc |
| print_int | 0x401300 | — | — | js 0x401340 | — | out |
| collatz | 0x401380 | — | — | — | — | — |
| fib | 0x4013b0 | — | — | — | — | — |
| classify | 0x4013e0 | jump table at 0x4013e9 | — | — | — | sink, table |
| print_str | 0x4014c0 | — | — | — | — | out |
| sum_table | 0x4014f0 | — | — | — | — | table |
| sort_table | 0x401530 | — | — | — | — | table |
| fail | 0x401980 | — | — | — | — | — |
| op_add | 0x4019a0 | — | — | — | — | — |
| op_sub | 0x4019b0 | — | — | — | — | — |
| op_mul | 0x4019c0 | — | — | — | — | — |
| op_div | 0x4019d0 | — | — | — | — | — |

## control32-upx.exe

`control32.exe` packed with `upx --best`: its code is the unpacker's stub, and no map names a
function of it. The program's functions are `control32.exe`'s once the stub has run.

## control64.exe

`testdata/src/control.c`, x64; the image of `crates/ra-asm-verify/tests/replay.rs` and
`ledger.rs`. Every number the program prints is positive or zero, so a recorded run never takes
`print_int`'s sign branch; `op_div` is entered once with a divisor of six, so its block at
0x1400016a6 that calls `fail` never runs either.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, table, names, sink, write_slot, write_proc |
| print_slot | 0x1400012b0 | — | call through write_slot at 0x1400012e5 | — | — | out, write_slot |
| print_proc | 0x1400012f0 | — | call through write_proc at 0x140001325 | — | — | out, write_proc |
| print_int | 0x140001330 | — | — | js 0x14000137d | — | out |
| collatz | 0x1400013c0 | — | — | — | — | — |
| fib | 0x1400013f0 | — | — | — | — | — |
| classify | 0x140001430 | jump table at 0x14000144d | — | — | — | sink, table |
| print_str | 0x140001560 | — | — | — | — | out |
| sum_table | 0x1400015b0 | — | — | — | — | table |
| sort_table | 0x1400015f0 | — | — | — | — | table |
| fail | 0x140001640 | — | — | — | — | — |
| op_add | 0x140001660 | — | — | — | — | — |
| op_sub | 0x140001670 | — | — | — | — | — |
| op_mul | 0x140001680 | — | — | — | — | — |
| op_div | 0x140001690 | — | — | — | — | — |

## control64-O1.exe

`testdata/src/control.c` at `-O1`, x64; the round-trip test's image. Unlike the `-O2` build, the
`ops` loop of `mainCRTStartup` calls through the table.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | call through ops at 0x1400010ca | — | — | out, ops, table, names, sink, write_slot, write_proc |
| print_slot | 0x140001170 | — | call through write_slot at 0x1400011a5 | — | — | out, write_slot |
| print_proc | 0x1400011b0 | — | call through write_proc at 0x1400011e5 | — | — | out, write_proc |
| print_int | 0x1400011f0 | — | — | js 0x14000123d | — | out |
| collatz | 0x140001280 | — | — | — | — | — |
| fib | 0x1400012b0 | — | — | — | — | — |
| classify | 0x1400012e0 | jump table at 0x1400012fd | — | — | — | sink, table |
| print_str | 0x140001410 | — | — | — | — | out |
| sum_table | 0x140001460 | — | — | — | — | table |
| sort_table | 0x140001480 | — | — | — | — | table |
| fail | 0x1400014d0 | — | — | — | — | — |
| op_add | 0x1400014f0 | — | — | — | — | — |
| op_sub | 0x140001500 | — | — | — | — | — |
| op_mul | 0x140001510 | — | — | — | — | — |
| op_div | 0x140001520 | — | — | — | — | — |

## control_mipsel

`testdata/src/control.c` for little-endian MIPS, freestanding (`testdata/src/freestanding`):
`GetStdHandle`, `WriteFile`, `ExitProcess`, `GetModuleHandleA` and `GetProcAddress` are the shims
of `freestanding/windows.c`, over `fs_write` and `fs_exit`. The branches are from the
disassembly, the globals from the source.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| main | 0x202e8 | — | — | — | — | out, table, names, sink, write_slot, write_proc |
| print_slot | 0x20498 | — | call through write_slot (`jalr`) at 0x204c4 | — | — | out, write_slot |
| print_proc | 0x204d8 | — | call through write_proc (`jalr`) at 0x20504 | — | — | out, write_proc |
| print_int | 0x20518 | — | — | bgez 0x20584 | — | out |
| collatz | 0x205c4 | — | — | — | — | — |
| fib | 0x205fc | — | — | — | — | — |
| classify | 0x2065c | jump table (`jr $1`) at 0x20680 | — | — | — | sink, table |
| print_str | 0x207c8 | — | — | — | — | out |
| sum_table | 0x20810 | — | — | — | — | table |
| sort_table | 0x20840 | — | — | — | — | table |
| fail | 0x208c8 | — | — | — | — | — |
| op_add | 0x208ec | — | — | — | — | — |
| op_sub | 0x208f4 | — | — | — | — | — |
| op_mul | 0x208fc | — | — | — | — | — |
| op_div | 0x20904 | — | — | — | — | — |

## crt64.exe

`testdata/src/crt.c`, x64, over the statically linked C runtime. Only the program's own functions
are here; `crt64.pdb` names the runtime's, and the port calls the host's runtime for them.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| main | 0x140001000 | — | — | — | — | — |
| total | 0x140001110 | — | — | — | — | — |
| label | 0x140001120 | — | — | — | — | — |

## dispatch32.exe

`testdata/src/dispatch.c`, x86.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, seed |
| install | 0x4010e0 | — | — | — | — | table |
| h_add | 0x4010f0 | — | — | — | — | — |
| h_sub | 0x401100 | — | — | — | — | — |
| h_mul | 0x401110 | — | — | — | — | — |
| h_xor | 0x401120 | — | — | — | — | — |
| print_int | 0x401130 | — | — | js 0x401170 | — | out |
| dispatch | 0x4011b0 | — | call through table at 0x4011b8 | — | — | table |

## dispatch64.exe

`testdata/src/dispatch.c`, x64; the image of the mutated vectors of
`crates/ra-asm-verify/tests/generate.rs`. It has no switch: its one indirect transfer is
`dispatch`'s tail jump through `table`, whose targets only a run knows.

The program prints 42, 38, 80, 42 and 77, all positive, so no recorded vector of `print_int`
(0x140001150) takes its sign branch, the `js` at 0x14000119d: the block at 0x1400011a4 that
writes the minus sign is the one the recorded vectors leave unrun, and a vector mutated from them
runs it.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, seed |
| install | 0x140001100 | — | — | — | — | table |
| h_add | 0x140001110 | — | — | — | — | — |
| h_sub | 0x140001120 | — | — | — | — | — |
| h_mul | 0x140001130 | — | — | — | — | — |
| h_xor | 0x140001140 | — | — | — | — | — |
| print_int | 0x140001150 | — | — | js 0x14000119d, to the unrun block 0x1400011a4 | — | out |
| dispatch | 0x1400011e0 | — | tail jump through table at 0x1400011f3 | — | — | table |

## exports32.dll

`testdata/src/exports.c`, x86: named exports, `ra_hidden` by ordinal alone, a data export
`ra_counter` and a forwarder `ra_tick`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| ra_add | 0x10001000 | — | — | — | — | — |
| ra_twice | 0x10001010 | — | — | — | — | — |
| ra_hidden | 0x10001020 | — | — | — | — | — |
| DllMainCRTStartup | 0x10001030 | — | — | — | — | — |

## exports64.dll

`testdata/src/exports.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| ra_add | 0x180001000 | — | — | — | — | — |
| ra_twice | 0x180001010 | — | — | — | — | — |
| ra_hidden | 0x180001020 | — | — | — | — | — |
| DllMainCRTStartup | 0x180001030 | — | — | — | — | — |

## exports_host32.exe

`testdata/src/exports_host.c`, x86. `put_number` is inlined into `line`. The register calls of
`mainCRTStartup` are `GetProcAddress` kept in a register and the three exports it returned; those
of `line` are `lstrlenA` and `WriteFile` kept in registers.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | calls of the exports GetProcAddress found at 0x4010db, 0x4010f2, 0x401119 | — | — | out |
| put | 0x401170 | — | — | — | — | out |
| line | 0x4011a0 | — | — | js 0x401210 | — | out |

## exports_host64.exe

`testdata/src/exports_host.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | calls of the exports GetProcAddress found at 0x140001121, 0x14000113c, 0x140001163 | — | — | out |
| put | 0x1400011c0 | — | — | — | — | out |
| line | 0x140001200 | — | — | js 0x14000128b | — | out |

## extended32.exe

`testdata/src/extended.c`, x86 without SSE: the x87 unit at both precision controls.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | fnstcw, fldcw, fldl, fstl, fmul, fmull, fmuls, fmulp, fistpll | out; x87 stack *(lift)* |
| print_int | 0x401220 | — | — | js 0x4012c1 | — | out |
| harmonic | 0x401300 | — | — | — | fld, fld1, fldz, fidivl, fdiv, fadds, faddp, fstp | x87 stack *(lift)* |
| leibniz | 0x401370 | — | — | — | fld, fld1, fldz, fildl, fadd, fadds, faddp, fdivr, fchs, fstp | x87 stack *(lift)* |
| root | 0x4013b0 | — | — | — | fldl, fsqrt | x87 stack *(lift)* |
| third | 0x4013c0 | — | — | — | fld1, fidivl, fstpt | x87 stack *(lift)* |
| from_tbyte | 0x4013e0 | — | — | — | fldt, fstpl, fldl | x87 stack *(lift)* |

## files32.exe

`testdata/src/files.c`, x86. `open_error`'s `switch` on `GetLastError` is a lookup of the result
string in a table of `.rdata`, not a jump table.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, path |
| print_int | 0x401100 | — | — | js 0x401140 | — | out |
| write_text | 0x401180 | — | — | — | — | — |
| read_text | 0x4011e0 | — | — | — | — | — |
| print_str | 0x401240 | — | — | — | — | out |
| is_plain_file | 0x401270 | — | — | — | — | — |
| open_error | 0x401290 | lookup table, no jump | — | — | — | — |

## files64.exe

`testdata/src/files.c`, x64. `open_error`'s `switch` is a lookup, as in the 32-bit build.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, path |
| print_int | 0x140001120 | — | — | js 0x14000116d | — | out |
| write_text | 0x1400011b0 | — | — | — | — | — |
| read_text | 0x140001230 | — | — | — | — | — |
| print_str | 0x1400012c0 | — | — | — | — | out |
| is_plain_file | 0x140001310 | — | — | — | — | — |
| open_error | 0x140001330 | lookup table, no jump | — | — | — | — |

## float32.exe

`testdata/src/float.c`, x86 with SSE2: the arithmetic is scalar SSE, and the conversions to a
64-bit integer go through the x87 unit. `dot` and `mix` are inlined. The program prints -7, so its
run takes `print_int`'s sign branch.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | fnstcw, fldcw, fldl, fistpll | out, zero, seven; x87 stack *(lift)* |
| print_int | 0x401210 | — | — | js 0x4012b1 | — | out |
| milli | 0x4012f0 | — | — | — | fnstcw, fldcw, fldl, fistpll | x87 stack *(lift)* |
| mean | 0x401340 | — | — | — | — | — |
| newton_sqrt | 0x401370 | — | — | — | — | — |
| classify | 0x401580 | — | — | — | — | — |

## float64.exe

`testdata/src/float.c`, x64. `mix` is inlined.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, zero, seven |
| print_int | 0x140001210 | — | — | js 0x14000126c | — | out |
| milli | 0x1400012b0 | — | — | — | — | — |
| mean | 0x1400012c0 | — | — | — | — | — |
| newton_sqrt | 0x1400012f0 | — | — | — | — | — |
| dot | 0x140001500 | — | — | — | — | — |
| classify | 0x140001530 | — | — | — | — | — |

## floata64.exe

`testdata/src/float.c`, ARM64 Windows. Its `blr` calls are imports loaded from their slots. The
branches are from the disassembly, the globals from the source.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, zero, seven |
| print_int | 0x140001198 | — | — | tbnz #63 0x1400011e4 | — | out |
| milli | 0x140001230 | — | — | — | — | — |
| mean | 0x140001248 | — | — | — | — | — |
| newton_sqrt | 0x140001278 | — | — | — | — | — |
| dot | 0x1400012ac | — | — | — | — | — |
| classify | 0x1400012b4 | — | — | — | — | — |

## gte_psx.exe

`testdata/src/psx/gte.c`, a PlayStation PS-EXE (MIPS R3000, PSn00bSDK); `gte_psx.map` is nm over
the ELF. The GTE is reached through coprocessor 2 instructions, and `printf` through the BIOS's
A(3Fh) call. `print_vertex` is inlined. From the source and the symbol table.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| triple | 0x80010000 | — | — | — | — | cube, names |
| single | 0x80010160 | — | — | — | — | — |
| main | 0x800101dc | — | — | — | — | cube, near_vertex, wide_vertex |
| InitGeom | 0x800102f0 | — | — | — | — | — |
| printf | 0x80010354 | — | — | — | — | — |
| _start_inner | 0x80010370 | — | — | — | — | — |

## hello32.exe

`testdata/src/hello.c`, x86. `add` is inlined.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x401000 | — | — | — | — | — |
| mainCRTStartup | 0x401010 | — | — | — | — | — |

## hello32-upx.exe

`hello32.exe` packed with `upx --best`: the code is the unpacker's stub, and no map names a
function of it.

## hello64.exe

`testdata/src/hello.c`, x64. `add` is inlined.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x140001000 | — | — | — | — | — |
| mainCRTStartup | 0x140001010 | — | — | — | — | — |

## hello64-upx.exe

`hello64.exe` packed with `upx --best`: the code is the unpacker's stub, and no map names a
function of it.

## hello_linux64

`testdata/src/hello_linux.c`, an x86-64 Linux ELF; `write` and `exit` through the PLT. `add` is
inlined.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x2013b0 | — | — | — | — | — |
| _start | 0x2013c0 | — | — | — | — | — |

## hello_linux_a64

`testdata/src/hello_linux.c`, an ARM64 Linux ELF; `write` and `exit` through the PLT.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x2103bc | — | — | — | — | — |
| _start | 0x2103c4 | — | — | — | — | — |

## hello_macos_arm64

`testdata/src/hello_macos.c`, an ARM64 Mach-O; `write` and `exit` through the stubs.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| _square | 0x100000400 | — | — | — | — | — |
| _main | 0x100000408 | — | — | — | — | — |

## hello_macos_x64

`testdata/src/hello_macos.c`, an x86-64 Mach-O; `write` and `exit` through the stubs.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| _square | 0x100000590 | — | — | — | — | — |
| _main | 0x1000005a0 | — | — | — | — | — |

## hello_mipsel

`testdata/src/hello.c` for little-endian MIPS, freestanding, over the shims of
`freestanding/windows.c` (`GetStdHandle`, `WriteFile`, `ExitProcess`).

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x20208 | — | — | — | — | — |
| main | 0x20210 | — | — | — | — | — |

## hello_psx.exe

`testdata/src/psx/hello.c`, a PlayStation PS-EXE; `printf` is the BIOS's A(3Fh) call. From the
source and `hello_psx.map`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| add.constprop.0 | 0x80010000 | — | — | — | — | — |
| square | 0x80010008 | — | — | — | — | — |
| main | 0x80010018 | — | — | — | — | — |
| printf | 0x8001007c | — | — | — | — | — |
| _start_inner | 0x80010098 | — | — | — | — | — |
| InitHeap | 0x800101b0 | — | — | — | — | — |

## helloa64.exe

`testdata/src/hello.c`, ARM64 Windows. Its `blr` calls are imports loaded from their slots.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x140001000 | — | — | — | — | — |
| mainCRTStartup | 0x140001008 | — | — | — | — | — |

## inline32.exe

`testdata/src/inline.c`, x86: `score` inlined into the three `show_*`. This `print_int` takes an
unsigned and has no sign branch. The compiler folded `names`, never written, into the strings'
addresses.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out |
| show_low | 0x4011a0 | — | — | — | — | — |
| show_high | 0x4011f0 | — | — | — | — | — |
| show_from | 0x401240 | — | — | — | — | — |
| print_int | 0x401270 | — | — | — | — | out |

## inline64.exe

`testdata/src/inline.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out |
| show_low | 0x1400011b0 | — | — | — | — | — |
| show_high | 0x140001210 | — | — | — | — | — |
| show_from | 0x140001270 | — | — | — | — | — |
| print_int | 0x1400012c0 | — | — | — | — | out |

## libmain_a64.so

`testdata/src/libmain.c`, an ARM64 Android shared library; `strlen` through the PLT. The
`b.lt` of `ra_score` is its loop's guard on `n`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| ra_add | 0x1000047c | — | — | — | — | — |
| ra_score | 0x10000484 | — | — | b.lt 0x10000498 (loop guard) | — | — |
| weigh | 0x100004dc | — | — | — | — | weights |
| ra_name_length | 0x100004f4 | — | — | — | — | names |
| ra_rename | 0x10000518 | — | — | — | — | names |

## linked32.exe

`testdata/src/linked.c`, x86, importing `exports32.dll`. `put_number` is inlined into `line`, and
the register calls are imports kept in registers. The program prints -1, so its run takes the
sign branch.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, ra_counter (import) |
| line | 0x401080 | — | — | js 0x4010f0 | — | out |

## linked64.exe

`testdata/src/linked.c`, x64, importing `exports64.dll`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, ra_counter (import) |
| line | 0x140001090 | — | — | js 0x14000111b | — | out |

## obfusc32.exe

`testdata/src/obfusc.c`, x86. `put` is inlined, and `WriteFile` kept in a register. `steps_of`'s
flattened `switch` over its state compiles to a tree of compares, not a jump table. This
`print_int` writes no sign.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, enc_banner, enc_steps, enc_tail, opaque_x, opaque_y |
| decrypt | 0x4010e0 | — | — | — | — | — |
| print_int | 0x401160 | — | — | — | — | out |
| steps_of | 0x4011d0 | compare tree, no jump | — | — | — | — |

## obfusc64.exe

`testdata/src/obfusc.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, enc_banner, enc_steps, enc_tail, opaque_x, opaque_y |
| decrypt | 0x140001150 | — | — | — | — | — |
| print_int | 0x1400011e0 | — | — | — | — | out |
| steps_of | 0x140001270 | compare tree, no jump | — | — | — | — |

## resources32.exe

`testdata/src/resources.c`, x86, with the string table and RCDATA of `resources.rc`. The `jle` of
`print_string_resource` is its `n > 0`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, counters |
| print_string_resource | 0x401230 | — | — | jle 0x40124c | — | — |
| print_rcdata | 0x401270 | — | — | — | — | — |
| print_int | 0x4012e0 | — | — | js 0x401320 | — | out |
| square_of | 0x401360 | — | — | — | — | squares |
| print_str | 0x401370 | — | — | — | — | out |
| name_of | 0x4013a0 | — | — | — | — | names |
| tenths_of | 0x4013b0 | — | — | — | — | halves |
| count | 0x4013d0 | — | — | — | — | counters |

## resources64.exe

`testdata/src/resources.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, counters |
| print_string_resource | 0x140001240 | — | — | jle 0x140001261 | — | — |
| print_rcdata | 0x140001280 | — | — | — | — | — |
| print_int | 0x140001310 | — | — | js 0x14000135d | — | out |
| square_of | 0x1400013a0 | — | — | — | — | squares |
| print_str | 0x1400013b0 | — | — | — | — | out |
| name_of | 0x140001400 | — | — | — | — | names |
| tenths_of | 0x140001410 | — | — | — | — | halves |
| count | 0x140001430 | — | — | — | — | counters |

## structs32.exe

`testdata/src/structs.c`, x86: a record behind a pointer. `scaled` converts to a 64-bit integer
through the x87 unit.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out |
| fill | 0x4010a0 | — | — | — | — | — |
| print_int | 0x4010d0 | — | — | js 0x401171 | — | out |
| weigh | 0x4011b0 | — | — | — | — | — |
| name_length | 0x4011e0 | — | — | — | — | — |
| scaled | 0x401200 | — | — | — | fnstcw, fldcw, fldl, fildll, fstpl, fistpll | x87 stack *(lift)* |

## structs64.exe

`testdata/src/structs.c`, x64; the image of the generated inputs of
`crates/ra-asm-verify/tests/generate.rs`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out |
| fill | 0x1400010a0 | — | — | — | — | — |
| print_int | 0x1400010e0 | — | — | js 0x14000113c | — | out |
| weigh | 0x140001180 | — | — | — | — | — |
| name_length | 0x1400011b0 | — | — | — | — | — |
| scaled | 0x1400011d0 | — | — | — | — | — |

## throws64.exe

`testdata/src/throws.cpp`, x64, over vcruntime's throw: the `catch$`, `dtor$` and `$ehgcr` entries
are the funclets and continuations the frame handler reaches. The program prints -1 and -2, so its
run takes `print_int`'s sign branch, and `check`'s `n < 0` throws.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out |
| print_int | 0x140001070 | — | — | js 0x1400010bd | — | out |
| attempt | 0x140001100 | — | — | — | — | — |
| $ehgcr_2_1 | 0x14000111a | — | — | — | — | — |
| catch$2 of attempt | 0x140001130 | — | — | — | — | — |
| catch$3 of attempt | 0x140001170 | — | — | — | — | — |
| nested | 0x1400011b0 | — | — | — | — | throw info of Error |
| $ehgcr_3_6 | 0x1400011da | — | — | — | — | — |
| catch$1 of nested | 0x1400011e0 | — | — | — | — | — |
| catch$5 of nested | 0x140001220 | — | — | — | — | — |
| print_str | 0x140001250 | — | — | — | — | out |
| relay | 0x1400012a0 | — | — | — | — | — |
| dtor$2 of relay | 0x1400012e0 | — | — | — | — | — |
| check | 0x140001300 | — | — | js 0x140001327 | — | throw info of int and Range |
| dtor$5 of check | 0x140001370 | — | — | — | — | — |
| Guard::~Guard | 0x140001390 | — | — | — | — | — |
| dtor$4 of Guard::~Guard | 0x1400013d0 | — | — | — | — | — |

## thumb32

`testdata/src/thumb.c`, a 32-bit ARM Linux ELF in Thumb-2, over `svc` alone. `larger`'s select is
an IT block, not a branch.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| square | 0x20118 | — | — | — | — | — |
| larger | 0x2011c | — | — | — | — | — |
| sum | 0x20124 | — | — | — | — | — |
| digits | 0x2013c | — | — | — | — | — |
| _start | 0x20184 | — | — | — | — | values |

## tls32.exe

`testdata/src/tls.c`, x86: thread-local storage and a TLS callback. `worker` is a thread's start,
reached only through `CreateThread`; its signed test is the loop's guard on `3 * k`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, seen, attached_process, attached_thread, _tls_index |
| run | 0x4010b0 | — | — | — | — | — |
| print_str | 0x4010e0 | — | — | — | — | out |
| print_int | 0x401110 | — | — | js 0x401150 | — | out |
| on_tls | 0x401190 | — | — | — | — | attached_process, attached_thread |
| worker | 0x4011b0 | — | — | jg 0x4011b9 (loop guard) | — | seen, _tls_index |

## tls64.exe

`testdata/src/tls.c`, x64. `run` tail-jumps to `CloseHandle` through its import slot.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, seen, attached_process, attached_thread, _tls_index |
| run | 0x1400010d0 | — | — | — | — | — |
| print_str | 0x140001120 | — | — | — | — | out |
| print_int | 0x140001170 | — | — | js 0x1400011bd | — | out |
| on_tls | 0x140001200 | — | — | — | — | attached_process, attached_thread |
| worker | 0x140001220 | — | — | jle 0x140001222 (loop guard) | — | seen, _tls_index |

## upx-sections.txt

Not a binary: the section names and bytes of the three packed fixtures.

## window32.exe

`testdata/src/window.c`, x86, over user32. `window_proc` is reached only as the window class's
procedure, and its default case tail-jumps to `DefWindowProcA` through the import slot.
`message_loop` keeps its three imports in registers; its signed tests are `GetMessageA(...) > 0`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x401000 | — | — | — | — | out, paints |
| window_proc | 0x401170 | jump table at 0x401187 | — | — | — | paints |
| print_str | 0x401260 | — | — | — | — | out |
| message_loop | 0x401290 | — | — | jle 0x4012a8, jg 0x4012d1 | — | — |
| print_int | 0x4012e0 | — | — | js 0x401320 | — | out |

## window64.exe

`testdata/src/window.c`, x64.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| mainCRTStartup | 0x140001000 | — | — | — | — | out, paints |
| window_proc | 0x140001180 | jump table at 0x1400011a4 | — | — | — | paints |
| print_str | 0x1400012b0 | — | — | — | — | out |
| message_loop | 0x140001300 | — | — | jle 0x14000131e, jg 0x14000135a | — | — |
| print_int | 0x140001370 | — | — | js 0x1400013bd | — | out |

## xbe32.xbe

`testdata/src/xbe.c`, an original Xbox XBE (x86) at base 0x10000. The kernel is called through
`xbe_kernel_thunks`, the table the XBE header names and the kernel fills before the entry runs.
From the source and `xbe32.map`.

| Function | Address | Switch | Computed | Sign | x87 | Globals |
|---|---|---|---|---|---|---|
| XbeEntry | 0x11000 | — | — | — | — | xbe_kernel_thunks |
| fib | 0x111a0 | — | — | — | — | — |
| checksum | 0x111d0 | — | — | — | — | — |
