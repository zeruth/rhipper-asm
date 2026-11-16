# Scoreboard

Regenerate: `cargo run -q -p ras -- score --write testdata/bin` from the repository's root, after merging any change to the analysis, the lift or the emit and before the check.

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 697 | 0.0 | 741.8 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 369 | 0.0 | 428.2 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 275 | 0.0 | 378.2 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 166 | 0.0 | 295.2 |
| control32-upx.exe | timeout | 3 | - | 3 | 0 | 0 | - | - | - | - | - | 10949 | 0.8 | 1900.2 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 800 | 0.0 | 813.8 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 416 | 0.0 | 473.6 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 435 | 0.0 | 462.1 |
| control_mipsel | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 579 | 1.7 | 554.4 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1663 | 0.0 | 2359.0 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 373 | 0.0 | 471.8 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 202 | 0.0 | 321.8 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 188 | 0.0 | 292.6 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 109 | 0.0 | 100.9 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 384 | 2.6 | 789.1 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 249 | 4.0 | 481.9 |
| extended32.exe | ok | 7 | 7 | 7 | 2 | 2 | 6 | 6 | 16 | 0 | 44 | 649 | 0.0 | 1423.7 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 413 | 0.0 | 598.1 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 276 | 0.0 | 434.8 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 27 | 0 | 78 | 623 | 0.0 | 1229.5 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 476 | 0.0 | 1180.7 |
| floata64.exe | crash | - | - | 7 | 0 | 0 | - | - | - | - | - | - | - | - |
| gte_psx.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 3 | 0 | 7 | 255 | 0.0 | 619.6 |
| hello32-upx.exe | timeout | 3 | - | 3 | 1 | 1 | 2 | - | - | - | - | 8910 | 0.9 | 1878.5 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 205 | 0.0 | 292.7 |
| hello64-upx.exe | timeout | 6 | - | 6 | 0 | 0 | 2 | - | 2 | 0 | 22 | 8787 | 0.7 | 1879.1 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 111 | 0.0 | 207.2 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 159 | 0.0 | 257.9 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 185 | 0.0 | 329.7 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 246 | 0.0 | 93.5 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 271 | 0.0 | 99.6 |
| hello_mipsel | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 169 | 0.0 | 260.4 |
| hello_psx.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 6 | 0 | 10 | 233 | 0.0 | 502.1 |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 114 | 0.0 | 175.4 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 316 | 0.0 | 458.9 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 219 | 0.0 | 438.4 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 181 | 0.0 | 209.9 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 288 | 0.0 | 652.8 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 162 | 0.0 | 481.5 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 357 | 0.0 | 453.8 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 244 | 0.0 | 237.7 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 412 | 0.0 | 565.5 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 299 | 0.0 | 347.8 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 11 | 0 | 28 | 382 | 0.0 | 565.4 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 217 | 0.0 | 285.7 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 470 | 0.0 | 310.6 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 8 | 0 | 8 | 250 | 0.0 | 528.0 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 380 | 0.0 | 652.6 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 236 | 0.0 | 529.7 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 465 | 0.0 | 853.8 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 308 | 0.0 | 464.3 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 324 | 0.0 | 552.5 |
| total | 48/52 ok | 336 | 324 | 343 | 5 | 5 | 271 | 267 | 732 | 0 | 1661 | 45446 | 0.6 | 1470.9 |

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
