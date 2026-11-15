# Scoreboard

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 934 | 0.0 | 616.7 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 465 | 0.0 | 344.1 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 390 | 0.0 | 435.9 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 196 | 0.0 | 250.0 |
| control32-upx.exe | timeout | - | - | 3 | 0 | 0 | - | - | - | - | - | 11456 | 0.8 | 2712.3 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 1276 | 0.0 | 670.8 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 548 | 0.0 | 450.7 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 553 | 0.0 | 415.9 |
| control_mipsel | ok | 65 | 65 | 65 | 6 | 4 | 1 | 1 | 18 | 0 | 135 | 2125 | 0.0 | 600.9 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1720 | 0.0 | 2284.3 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 518 | 0.0 | 438.2 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 242 | 0.0 | 276.9 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 200 | 0.0 | 275.0 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 109 | 0.0 | 100.9 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 633 | 3.2 | 565.6 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 343 | 2.9 | 414.0 |
| extended32.exe | ok | 7 | 7 | 7 | 2 | 2 | 6 | 1 | 16 | 0 | 44 | 890 | 0.0 | 1130.3 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 683 | 0.0 | 532.9 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 373 | 0.0 | 348.5 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 4 | 27 | 0 | 78 | 814 | 0.0 | 996.3 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 564 | 0.0 | 1012.4 |
| floata64.exe | crash | - | - | 7 | 0 | 0 | - | - | - | - | - | - | - | - |
| gte_psx.exe | error | - | - | - | - | - | - | - | - | - | - | - | - | - |
| hello32-upx.exe | timeout | - | - | 3 | 1 | 1 | - | - | - | - | - | 9445 | 0.8 | 2640.8 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 241 | 0.0 | 344.4 |
| hello64-upx.exe | timeout | 6 | - | 6 | 0 | 0 | 2 | - | - | - | - | 9107 | 0.7 | 2714.8 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 119 | 0.0 | 218.5 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 166 | 0.0 | 253.0 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 203 | 0.0 | 300.5 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 252 | 0.0 | 87.3 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 284 | 0.0 | 112.7 |
| hello_mipsel | ok | 1 | 1 | 1 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 85 | 0.0 | 141.2 |
| hello_psx.exe | error | - | - | - | - | - | - | - | - | - | - | - | - | - |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 118 | 0.0 | 169.5 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 439 | 0.0 | 471.5 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 258 | 0.0 | 387.6 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 201 | 0.0 | 194.0 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 377 | 0.0 | 591.5 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 218 | 0.0 | 431.2 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 557 | 0.0 | 407.5 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 337 | 0.0 | 189.9 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 691 | 0.0 | 519.5 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 382 | 0.0 | 288.0 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 5 | 11 | 0 | 28 | 538 | 0.0 | 503.7 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 261 | 0.0 | 256.7 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 716 | 0.0 | 234.6 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 8 | 0 | 8 | 303 | 0.0 | 478.5 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 549 | 0.0 | 581.1 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 293 | 0.0 | 433.4 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 687 | 0.0 | 722.0 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 380 | 0.0 | 397.4 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 437 | 0.0 | 599.5 |
| total | 46/52 ok | 355 | 349 | 368 | 9 | 7 | 240 | 230 | 725 | 0 | 1682 | 52676 | 0.5 | 1816.8 |

- gte_psx.exe: C:\Users\tyler\Documents\code\rhipper-asm\var\worktrees\213\testdata\bin\gte_psx.exe is not a PE image this loader reads: the DOS magic at offset 0x0 is 0x5350, not MZ
- hello_psx.exe: C:\Users\tyler\Documents\code\rhipper-asm\var\worktrees\213\testdata\bin\hello_psx.exe is not a PE image this loader reads: the DOS magic at offset 0x0 is 0x5350, not MZ

| pending list | entries |
|---|---|
| testdata/csmith/EMIT_PENDING.txt | 67 |
| testdata/csmith/PENDING.txt | 43 |
| testdata/levels/PENDING.txt | 1 |
| testdata/vectors/arm32/PENDING.txt | 12 |
| testdata/vectors/arm64/PENDING.txt | 0 |
| testdata/vectors/x86/PENDING.txt | 1 |
| testdata/vectors/x86_64/PENDING.txt | 0 |
| testdata/vectors/x87/PENDING.txt | 29 |
| total | 153 |

| oracle | ignored tests |
|---|---|
| crates/ra-asm-lift/tests/oracle.rs | 0 |
| crates/ra-asm-emit/tests/oracle.rs | 18 |
| total | 18 |
