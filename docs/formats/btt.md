# `.btt` save files

Two raw struct dumps next to the executable, not in `data/`. Parser: `thefly_assets::btt`. The exe contains the names `scoot.btt`, `boite.btt` and `boitedlg.bmp`. Field meaning is owned by [`docs/re/game-spec.md`](../re/game-spec.md) section 7; this file records the byte layout and what the shipped files hold.

## `scoot.btt` (488 bytes): score table

```
10 records x 48 bytes, then 1 x f64
```

| Offset in record | Size | Field |
|---|---|---|
| 0x00 | 12 | `name`: NUL-terminated text, one byte per char (assume Latin-1), so at most 11 characters |
| 0x0c | i32 | `score`: the fly score minus 10 x kills |
| 0x10 | 18 | run statistics written by name entry (below) |
| 0x22 | 6 | never written; stack garbage in the shipped rows |
| 0x28 | f64 | `checksum`: `c(score)`, or a random filler on an empty slot |

The file ends with a f64 at offset 480: the checksum of the sum of all scores.

Checksum: `c(s) = sqrt(x)` with `x = 5.365799563 * s - sqrt(5.365799563)`, and `x = 0.1835` when `x < 0`. Empty slots (score 0) get a random filler, `rand 0..7999 / 5.63`; a score sum below 1 gives the trailer `rand 0..7999 / 6.8899`. The full rules, the insert order and the validation are in game-spec section 7.1.

Run statistics (name entry, `FUN_0041a7ac`): objects saved (i16 at 0x10), maximum life time (f32 s at 0x14), average life time (f32 s at 0x18), average price (i32 at 0x1c), flies killed (i16 at 0x20). Nothing in the original exe reads them back, and the shipped default rows hold garbage in all of 0x10..0x27. A parser must stop the name at the first NUL and ignore the statistics. A writer should zero-fill or write the statistics.

Shipped file: all 10 names are `None`. Record 0 has score 1 and checksum 1.7462474069349656 (= c(1)); every other record has score 0 and a filler checksum (1293.250, 146.892, 348.313, 87.211, 531.972, 700.178, 857.371, 965.542, 69.449 in record order from 1). The trailing f64 equals record 0's checksum bit for bit, because the scores sum to 1.

An earlier version of this file read a 40-byte name and an f64 "score" at +40. That f64 is the checksum; the real score is the i32 at +0x0c. The shipped file cannot tell the two apart for record 0 (c(1) = 1.746), which is how the mistake came about.

## `boite.btt` (8 bytes): settings

Two u32 little-endian: [0] music on (1) or off (0), [1] tutorial on or off (game-spec section 7.2). Shipped value: `1, 1`. The parser returns the raw pair.
