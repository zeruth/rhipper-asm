# Scoreboard

Regenerate: `cargo run -q -p ras -- score --write testdata/bin` from the repository's root, after merging any change to the analysis, the lift or the emit and before the check.

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 529 | 0.0 | 707.0 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 324 | 0.0 | 413.6 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 214 | 0.0 | 327.1 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 129 | 0.0 | 286.8 |
| control32-upx.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 3 | 2 | 0 | 19 | 22234 | 0.4 | 930.4 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 695 | 0.0 | 441.7 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 353 | 0.0 | 430.6 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 378 | 0.0 | 431.2 |
| control_mipsel | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 513 | 1.9 | 504.9 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1401 | 0.0 | 2766.6 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 277 | 0.0 | 328.5 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 156 | 0.0 | 339.7 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 120 | 0.0 | 158.3 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 82 | 0.0 | 134.1 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 325 | 3.1 | 658.5 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 214 | 4.7 | 359.8 |
| extended32.exe | ok | 7 | 7 | 7 | 2 | 2 | 6 | 6 | 16 | 0 | 44 | 509 | 0.0 | 569.7 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 358 | 0.0 | 561.5 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 244 | 0.0 | 413.9 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 27 | 0 | 78 | 570 | 0.0 | 1128.1 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 445 | 0.0 | 1251.7 |
| floata64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 36 | 369 | 0.0 | 810.3 |
| gte_psx.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 3 | 0 | 7 | 196 | 0.0 | 500.0 |
| hello32-upx.exe | ok | 3 | 3 | 3 | 1 | 1 | 2 | 3 | 2 | 0 | 22 | 17847 | 0.4 | 930.2 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 162 | 0.0 | 290.1 |
| hello64-upx.exe | ok | 6 | 6 | 6 | 0 | 0 | 2 | 2 | 2 | 0 | 22 | 8545 | 0.7 | 1929.0 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 86 | 0.0 | 267.4 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 137 | 0.0 | 270.1 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 164 | 0.0 | 372.0 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 103 | 0.0 | 223.3 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 111 | 0.0 | 126.1 |
| hello_mipsel | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 150 | 0.0 | 260.0 |
| hello_psx.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 6 | 0 | 10 | 161 | 0.0 | 385.1 |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 89 | 0.0 | 224.7 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 268 | 0.0 | 324.6 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 189 | 0.0 | 296.3 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 158 | 0.0 | 240.5 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 225 | 0.0 | 560.0 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 126 | 0.0 | 388.9 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 301 | 0.0 | 372.1 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 214 | 0.0 | 238.3 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 350 | 0.0 | 468.6 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 259 | 0.0 | 328.2 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 11 | 0 | 28 | 327 | 0.0 | 470.9 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 187 | 0.0 | 299.5 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 425 | 0.0 | 263.5 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 8 | 0 | 8 | 192 | 0.0 | 380.2 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 306 | 0.0 | 490.2 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 201 | 0.0 | 467.7 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 380 | 0.0 | 850.0 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 268 | 0.0 | 462.7 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 168 | 0.0 | 470.2 |
| total | 52/52 ok | 343 | 343 | 343 | 5 | 5 | 280 | 282 | 772 | 0 | 1738 | 62734 | 0.4 | 1021.1 |

| pending list | entries |
|---|---|
| testdata/csmith/EMIT_PENDING.txt | 67 |
| testdata/csmith/PENDING.txt | 43 |
| testdata/levels/PENDING.txt | 1 |
| testdata/vectors/arm32/PENDING.txt | 12 |
| testdata/vectors/arm64/PENDING.txt | 0 |
| testdata/vectors/fn/TREE_PENDING.txt | 0 |
| testdata/vectors/toy/PENDING.txt | 0 |
| testdata/vectors/x86/PENDING.txt | 1 |
| testdata/vectors/x86_64/PENDING.txt | 0 |
| testdata/vectors/x87/PENDING.txt | 29 |
| total | 153 |

| oracle | ignored tests |
|---|---|
| crates/ra-asm-lift/tests/oracle.rs | 0 |
| crates/ra-asm-emit/tests/oracle.rs | 16 |
| total | 16 |
