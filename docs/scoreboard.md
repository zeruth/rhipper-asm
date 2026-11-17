# Scoreboard

Regenerate: `cargo run -q -p ras -- score --write testdata/bin` from the repository's root, after merging any change to the analysis, the lift or the emit and before the check.

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 512 | 0.0 | 673.8 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 315 | 0.0 | 409.5 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 210 | 0.0 | 266.7 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 126 | 0.0 | 246.0 |
| control32-upx.exe | timeout | 3 | 3 | 3 | 0 | 0 | 2 | 3 | 2 | 0 | 19 | 20956 | 0.4 | 497.7 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 685 | 0.0 | 424.8 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 343 | 0.0 | 393.6 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 365 | 0.0 | 397.3 |
| control_mipsel | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 498 | 2.0 | 451.8 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1388 | 0.0 | 2792.5 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 268 | 0.0 | 276.1 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 153 | 0.0 | 313.7 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 116 | 0.0 | 129.3 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 82 | 0.0 | 134.1 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 322 | 3.1 | 543.5 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 211 | 4.7 | 331.8 |
| extended32.exe | ok | 7 | 7 | 7 | 2 | 2 | 6 | 6 | 16 | 0 | 44 | 484 | 0.0 | 386.4 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 343 | 0.0 | 507.3 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 228 | 0.0 | 359.6 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 27 | 0 | 78 | 551 | 0.0 | 459.2 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 428 | 0.0 | 415.9 |
| floata64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 36 | 351 | 0.0 | 487.2 |
| gte_psx.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 3 | 0 | 7 | 190 | 0.0 | 421.1 |
| hello32-upx.exe | timeout | 3 | 3 | 3 | 1 | 1 | 2 | 3 | 2 | 0 | 22 | 16830 | 0.4 | 499.0 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 161 | 0.0 | 242.2 |
| hello64-upx.exe | timeout | 6 | 6 | 6 | 0 | 0 | 2 | 2 | 2 | 0 | 22 | 8529 | 0.7 | 1928.6 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 84 | 0.0 | 226.2 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 134 | 0.0 | 246.3 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 160 | 0.0 | 312.5 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 100 | 0.0 | 200.0 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 109 | 0.0 | 119.3 |
| hello_mipsel | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 143 | 0.0 | 244.8 |
| hello_psx.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 6 | 0 | 10 | 153 | 0.0 | 300.7 |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 87 | 0.0 | 183.9 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 259 | 0.0 | 289.6 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 180 | 0.0 | 288.9 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 157 | 0.0 | 242.0 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 222 | 0.0 | 423.4 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 124 | 0.0 | 322.6 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 290 | 0.0 | 334.5 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 202 | 0.0 | 232.7 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 337 | 0.0 | 415.4 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 249 | 0.0 | 301.2 |
| scan32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 36 | 0 | 72 | 281 | 10.7 | 213.5 |
| scan64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 36 | 0 | 72 | 200 | 15.0 | 210.0 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 11 | 0 | 28 | 312 | 0.0 | 381.4 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 179 | 0.0 | 245.8 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 409 | 0.0 | 188.3 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 8 | 0 | 8 | 180 | 0.0 | 377.8 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 294 | 0.0 | 428.6 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 192 | 0.0 | 411.5 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 373 | 0.0 | 745.3 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 261 | 0.0 | 375.5 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 165 | 0.0 | 472.7 |
| total | 51/54 ok | 351 | 351 | 351 | 5 | 5 | 288 | 290 | 844 | 0 | 1882 | 60481 | 0.5 | 727.9 |

| pending list | entries |
|---|---|
| testdata/csmith/EMIT_PENDING.txt | 67 |
| testdata/csmith/PENDING.txt | 42 |
| testdata/levels/PENDING.txt | 1 |
| testdata/vectors/arm32/PENDING.txt | 12 |
| testdata/vectors/arm64/PENDING.txt | 0 |
| testdata/vectors/fn/TREE_PENDING.txt | 0 |
| testdata/vectors/toy/PENDING.txt | 0 |
| testdata/vectors/x86/PENDING.txt | 1 |
| testdata/vectors/x86_64/PENDING.txt | 0 |
| testdata/vectors/x87/PENDING.txt | 29 |
| total | 152 |

| oracle | ignored tests |
|---|---|
| crates/ra-asm-lift/tests/oracle.rs | 0 |
| crates/ra-asm-emit/tests/oracle.rs | 15 |
| total | 15 |
