# Scoreboard

Regenerate: `cargo run -q -p ras -- score --write testdata/bin` from the repository's root, after merging any change to the analysis, the lift or the emit and before the check.

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a64sys | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 10 | 0 | 24 | 258 | 0.0 | 647.3 |
| aligned32.exe | ok | 6 | 6 | 6 | 0 | 0 | 5 | 5 | 5 | 0 | 16 | 374 | 10.7 | 227.3 |
| arm1 | ok | 9 | 9 | 9 | 0 | 0 | 9 | 8 | 18 | 0 | 17 | 335 | 0.0 | 304.5 |
| armfp | ok | 16 | 16 | 16 | 0 | 0 | 16 | 15 | 71 | 0 | 71 | 688 | 0.0 | 187.5 |
| armthumb | ok | 16 | 16 | 16 | 0 | 0 | 16 | 15 | 24 | 0 | 23 | 467 | 0.0 | 295.5 |
| armthumb_stripped | ok | 16 | 16 | 16 | 0 | 0 | 16 | 15 | 24 | 0 | 23 | 467 | 0.0 | 295.5 |
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 514 | 0.0 | 671.2 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 315 | 0.0 | 409.5 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 212 | 0.0 | 264.2 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 126 | 0.0 | 246.0 |
| control32-upx.exe | timeout | 3 | 3 | 3 | 0 | 0 | 2 | 3 | 2 | 0 | 19 | 20958 | 0.4 | 497.8 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 689 | 0.0 | 425.3 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 343 | 0.0 | 393.6 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 365 | 0.0 | 397.3 |
| control_mips | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 577 | 0.0 | 351.8 |
| control_mipsel | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 547 | 0.0 | 462.5 |
| cortexm.elf | ok | 6 | 6 | 6 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 223 | 0.0 | 322.9 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1388 | 0.0 | 2792.5 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 270 | 0.0 | 274.1 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 153 | 0.0 | 313.7 |
| draw_psx.exe | timeout | 55 | - | 55 | 0 | 0 | 34 | - | 65 | 0 | 114 | 3580 | 13.1 | 538.0 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 123 | 0.0 | 154.5 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 82 | 0.0 | 134.1 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 314 | 3.2 | 468.2 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 211 | 4.7 | 331.8 |
| extended32.exe | ok | 7 | 7 | 7 | 1 | 1 | 7 | 7 | 16 | 0 | 44 | 556 | 0.0 | 375.9 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 345 | 0.0 | 504.3 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 228 | 0.0 | 359.6 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 27 | 0 | 78 | 621 | 0.0 | 458.9 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 428 | 0.0 | 415.9 |
| floata64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 36 | 351 | 0.0 | 487.2 |
| gte_psx.exe | ok | 8 | 8 | 8 | 0 | 0 | 5 | 5 | 6 | 0 | 28 | 493 | 0.0 | 525.4 |
| hello32-upx.exe | timeout | 3 | 3 | 3 | 1 | 1 | 2 | 3 | 2 | 0 | 22 | 16916 | 0.4 | 497.3 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 163 | 0.0 | 239.3 |
| hello64-upx.exe | timeout | 6 | 6 | 6 | 0 | 0 | 2 | 2 | 2 | 0 | 22 | 8524 | 0.6 | 1929.7 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 84 | 0.0 | 226.2 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 134 | 0.0 | 246.3 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 160 | 0.0 | 312.5 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 100 | 0.0 | 200.0 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 109 | 0.0 | 119.3 |
| hello_mips | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 150 | 0.0 | 233.3 |
| hello_mipsel | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 145 | 0.0 | 241.4 |
| hello_psx.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 6 | 0 | 12 | 178 | 0.0 | 427.0 |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 87 | 0.0 | 183.9 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 261 | 0.0 | 287.4 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 180 | 0.0 | 288.9 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 157 | 0.0 | 242.0 |
| lines32.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 17 | 0 | 34 | 294 | 10.2 | 210.9 |
| lines64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 17 | 0 | 34 | 215 | 14.0 | 204.7 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 224 | 0.0 | 419.6 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 124 | 0.0 | 322.6 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 292 | 0.0 | 332.2 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 202 | 0.0 | 232.7 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 339 | 0.0 | 413.0 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 249 | 0.0 | 301.2 |
| scan32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 36 | 0 | 72 | 280 | 0.0 | 214.3 |
| scan64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 36 | 0 | 72 | 196 | 0.0 | 214.3 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 11 | 0 | 28 | 376 | 0.0 | 351.1 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 179 | 0.0 | 245.8 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 409 | 0.0 | 188.3 |
| thumb1 | ok | 12 | 12 | 12 | 0 | 0 | 12 | 11 | 22 | 0 | 21 | 413 | 0.0 | 322.0 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 4 | 8 | 0 | 8 | 181 | 0.0 | 237.6 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 296 | 0.0 | 425.7 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 192 | 0.0 | 411.5 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 375 | 0.0 | 741.3 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 261 | 0.0 | 375.5 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 167 | 0.0 | 467.1 |
| total | 63/67 ok | 546 | 491 | 546 | 6 | 6 | 447 | 409 | 1136 | 0 | 2361 | 69213 | 1.1 | 686.8 |

| pending list | entries |
|---|---|
| testdata/csmith/EMIT_PENDING.txt | 67 |
| testdata/csmith/PENDING.txt | 42 |
| testdata/levels/PENDING.txt | 2 |
| testdata/vectors/arm32/PENDING.txt | 8 |
| testdata/vectors/arm64/PENDING.txt | 0 |
| testdata/vectors/fn/TREE_PENDING.txt | 0 |
| testdata/vectors/mips64el/PENDING.txt | 0 |
| testdata/vectors/toy/PENDING.txt | 0 |
| testdata/vectors/x86/PENDING.txt | 1 |
| testdata/vectors/x86_64/PENDING.txt | 0 |
| testdata/vectors/x87/PENDING.txt | 29 |
| total | 149 |

| oracle | ignored tests |
|---|---|
| crates/ra-asm-lift/tests/oracle.rs | 0 |
| crates/ra-asm-emit/tests/oracle.rs | 15 |
| total | 15 |
