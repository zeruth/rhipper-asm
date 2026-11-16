# Scoreboard

Regenerate: `cargo run -q -p ras -- score --write testdata/bin` from the repository's root, after merging any change to the analysis, the lift or the emit and before the check.

The board of the fixtures, written by `ras score --write` over `testdata/bin`. `cargo test -q -p ras --test score committed` recomputes it: a number worse than this file's fails with the program and the column, a better one asks for `ras score --write`, so a change's diff shows what it moved.

| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | discharged | checked | open | lines | gotos ‰ | casts ‰ |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| classes32.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 8 | 0 | 37 | 624 | 0.0 | 826.9 |
| classes64.exe | ok | 15 | 15 | 15 | 0 | 0 | 15 | 15 | 10 | 0 | 35 | 327 | 0.0 | 461.8 |
| computed32.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 5 | 0 | 31 | 243 | 0.0 | 423.9 |
| computed64.exe | ok | 4 | 4 | 4 | 0 | 0 | 3 | 3 | 11 | 0 | 25 | 139 | 0.0 | 338.1 |
| control32-upx.exe | timeout | 3 | - | 3 | 0 | 0 | - | - | - | - | - | 10735 | 0.8 | 1936.3 |
| control32.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 755 | 0.0 | 860.9 |
| control64-O1.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 32 | 0 | 68 | 378 | 0.0 | 505.3 |
| control64.exe | ok | 15 | 15 | 15 | 0 | 0 | 14 | 14 | 66 | 0 | 133 | 395 | 0.0 | 501.3 |
| control_mipsel | ok | 23 | 23 | 23 | 1 | 1 | 17 | 17 | 11 | 0 | 70 | 546 | 1.8 | 569.6 |
| crt64.exe | ok | 15 | 15 | 15 | 0 | 0 | 3 | 3 | 4 | 0 | 8 | 1418 | 0.0 | 2766.6 |
| dispatch32.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 332 | 0.0 | 527.1 |
| dispatch64.exe | ok | 8 | 8 | 8 | 0 | 0 | 8 | 8 | 15 | 0 | 33 | 164 | 0.0 | 384.1 |
| exports32.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 152 | 0.0 | 361.8 |
| exports64.dll | ok | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 82 | 0.0 | 134.1 |
| exports_host32.exe | ok | 3 | 3 | 3 | 0 | 0 | 2 | 2 | 8 | 0 | 25 | 348 | 2.9 | 839.1 |
| exports_host64.exe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 8 | 0 | 25 | 223 | 4.5 | 524.7 |
| extended32.exe | ok | 7 | 7 | 7 | 2 | 2 | 6 | 6 | 16 | 0 | 44 | 593 | 0.0 | 701.5 |
| files32.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 369 | 0.0 | 666.7 |
| files64.exe | ok | 7 | 7 | 7 | 0 | 0 | 5 | 5 | 14 | 0 | 28 | 240 | 0.0 | 475.0 |
| float32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 27 | 0 | 78 | 590 | 0.0 | 1262.7 |
| float64.exe | ok | 7 | 7 | 7 | 0 | 0 | 7 | 7 | 36 | 0 | 72 | 445 | 0.0 | 1258.4 |
| floata64.exe | crash | - | - | 7 | 0 | 0 | - | - | - | - | - | - | - | - |
| gte_psx.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 3 | 0 | 7 | 206 | 0.0 | 752.4 |
| hello32-upx.exe | timeout | 3 | - | 3 | 1 | 1 | 2 | - | - | - | - | 8668 | 0.9 | 1928.1 |
| hello32.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 174 | 0.0 | 344.8 |
| hello64-upx.exe | timeout | 6 | - | 6 | 0 | 0 | 2 | - | 2 | 0 | 22 | 8551 | 0.7 | 1929.8 |
| hello64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 2 | 86 | 0.0 | 267.4 |
| hello_linux64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 140 | 0.0 | 292.9 |
| hello_linux_a64 | ok | 5 | 5 | 5 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 164 | 0.0 | 372.0 |
| hello_macos_arm64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 2 | 103 | 0.0 | 223.3 |
| hello_macos_x64 | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 2 | 0 | 4 | 120 | 0.0 | 225.0 |
| hello_mipsel | ok | 8 | 8 | 8 | 1 | 1 | 4 | 4 | 3 | 0 | 5 | 154 | 0.0 | 285.7 |
| hello_psx.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 6 | 0 | 10 | 172 | 0.0 | 668.6 |
| helloa64.exe | ok | 2 | 2 | 2 | 0 | 0 | 2 | 2 | 1 | 0 | 1 | 89 | 0.0 | 224.7 |
| inline32.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 280 | 0.0 | 517.9 |
| inline64.exe | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 38 | 0 | 76 | 189 | 0.0 | 481.5 |
| libmain_a64.so | ok | 6 | 6 | 6 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 158 | 0.0 | 240.5 |
| linked32.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 251 | 0.0 | 745.0 |
| linked64.exe | ok | 2 | 2 | 2 | 0 | 0 | 1 | 1 | 4 | 0 | 8 | 135 | 0.0 | 563.0 |
| obfusc32.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 323 | 0.0 | 501.5 |
| obfusc64.exe | ok | 4 | 4 | 4 | 0 | 0 | 4 | 4 | 5 | 0 | 10 | 214 | 0.0 | 252.3 |
| resources32.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 369 | 0.0 | 628.7 |
| resources64.exe | ok | 9 | 9 | 9 | 0 | 0 | 9 | 9 | 62 | 0 | 124 | 262 | 0.0 | 385.5 |
| structs32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 11 | 0 | 28 | 344 | 0.0 | 575.6 |
| structs64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 13 | 0 | 26 | 187 | 0.0 | 315.5 |
| throws64.exe | ok | 10 | 10 | 10 | 0 | 0 | 8 | 8 | 19 | 0 | 56 | 436 | 0.0 | 321.1 |
| thumb32 | ok | 5 | 5 | 5 | 0 | 0 | 5 | 5 | 8 | 0 | 8 | 233 | 0.0 | 557.9 |
| tls32.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 339 | 0.0 | 728.6 |
| tls64.exe | ok | 6 | 6 | 6 | 0 | 0 | 6 | 6 | 12 | 0 | 24 | 204 | 0.0 | 563.7 |
| window32.exe | ok | 5 | 5 | 5 | 0 | 0 | 1 | 1 | 17 | 0 | 32 | 417 | 0.0 | 949.6 |
| window64.exe | ok | 5 | 5 | 5 | 0 | 0 | 2 | 2 | 17 | 0 | 32 | 270 | 0.0 | 522.2 |
| xbe32.xbe | ok | 3 | 3 | 3 | 0 | 0 | 3 | 3 | 12 | 0 | 24 | 210 | 0.0 | 852.4 |
| total | 48/52 ok | 336 | 324 | 343 | 5 | 5 | 271 | 267 | 732 | 0 | 1661 | 42546 | 0.6 | 1554.7 |

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
