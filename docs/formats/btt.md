# `.btt` save files

Two raw struct dumps next to the executable, not in `data/`. Parser: `thefly_assets::btt`. The exe contains the names `scoot.btt`, `boite.btt` and `boitedlg.bmp`.

## `scoot.btt` (488 bytes): score table

```
10 records x 48 bytes, then 1 x f64
```

| Offset in record | Size | Field |
|---|---|---|
| 0 | 40 | `name`: NUL-terminated text, one byte per char (assume Latin-1) |
| 40 | f64 | `score` |

The file ends with a f64 at offset 480.

Important: the bytes of `name` after the NUL are not zeroed. They hold uninitialised memory (stack pointers such as `00 F0 FD 7F`, small integers like `2F 00`). A parser must stop at the first NUL and ignore the rest, and a writer should zero-fill. The shipped file differs from a fresh table only by that garbage.

Shipped file: all 10 names are `None`. Scores in order: 1.746, 1293.250, 146.892, 348.313, 87.211, 531.972, 700.178, 857.371, 965.542, 69.449. They are not sorted, so the table is probably an unsorted default or written by a test build. The trailing f64 equals `entries[0].score` (1.7462474069349656) bit for bit.

Evidence: "None" appears at offsets 0x00, 0x30, 0x60 ... 0x1B0 (stride 48, 10 hits); the f64 at `record + 40` decodes to plausible magnitudes (1 to 1300) for every record, and the 8 trailing bytes equal the first record's f64.

What a score is (points or seconds) and whether lower or higher is better is not decided here; the values look like times in seconds or points. Task B should settle this from the scoring code and the in-game high score screen.

Open: the meaning of the trailing f64 (a copy of the first score? the score of the last game?).

## `boite.btt` (8 bytes): settings

Two u32 little-endian. Shipped value: `1, 1`.

Probable meaning: flags for the start-up dialog (the exe has a `boitedlg.bmp`; "boite" is French for box, as in dialog box), for example "show intro" and "show this dialog". This is a guess from the name only. Task B should confirm. The parser returns the raw pair.
