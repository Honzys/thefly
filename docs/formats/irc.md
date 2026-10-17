# `.irc` image files

135 files in `data/rsc/` (`ac*`, `div*`, `ig*`, `iig*`). Backgrounds, sprites and sprite strips. Parser: `thefly_assets::irc`.

## Layout

All integers are little-endian.

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | u32 | `declared_len` | Equals the BMP `bfSize` field (file length minus 20). |
| 4 | u32 | reserved | Always 0. |
| 8 | f64 | `aux` | Derived from `declared_len` only. Ignore. |
| 16 | ... | BMP | A standard Windows bitmap: 14-byte file header (`BM`), 40-byte `BITMAPINFOHEADER`, optional palette, pixel rows. |

The BMP follows the usual rules:

- Rows are padded to 4 bytes. Positive height means bottom-up. The decoder also accepts negative height (top-down).
- Compression is 0 (uncompressed) in every file.
- Palette entries are BGRx. The palette length is `biClrUsed`, or `1 << bpp` when that field is 0.
- 16-bit pixels are RGB555. 24-bit pixels are BGR.
- The BMP ends 6 bytes before the end of the file: 4 bytes are not covered by `bfSize` and 2 more follow the pixel data. These bytes are junk (mostly zero, sometimes not). Parsers must not require `file_len == 16 + bfSize`.

## Colour key

The transparent colour is `FF 00 FF` (magenta). The rule "a pixel whose RGB equals FF00FF is transparent" holds for every depth and is what `irc::parse` applies (alpha 0, RGB kept).

## Evidence

Checked over all 135 files:

| Claim | Result |
|---|---|
| bytes 0..4 equal `bfSize` | 135 of 135 |
| bytes 4..8 are zero | 135 of 135 |
| `BM` at offset 16, DIB header size 40, compression 0 | 135 of 135 |
| bit depths | 8 bpp: 122, 24 bpp: 11, 16 bpp: 1 (`ac13`), 4 bpp: 1 (`div8`) |
| palette index 0 is FF00FF (8 bpp and 4 bpp) | 122 of 123; `ac15` has `02 2A 47` and no magenta entry |
| FF00FF appears only at index 0 | yes, in every palettised file |
| key pixels actually used | 110 of 122 8-bpp files; 3 of 11 24-bpp files |
| 8-bpp files with no key pixel (plus `ac13` and `div8`, also keyless) | `ac0 ac12 ac14 ac15 ac7 div19 ig35 ig36 ig4 iig0 iig20 iig6` |

`aux`: `aux / sqrt(declared_len)` is about 2.3164 for every file (2.3149 to 2.3164), and files of equal length have equal `aux`. So it is a pure function of the length, probably a leftover `double` computed by the tool that wrote the files. Nothing depends on it. The parser exposes it for completeness.

Pixel density: `biXPelsPerMeter` is 2834 (72 dpi) in most files, 5904, 7198 or 9840 in a few. Irrelevant to rendering.

Visual spot check (extractor output): `ac13` (16 bpp title screen), `ig37` (sprite with transparency), `div8` (4 bpp) and others decode with correct orientation and colours.

## Open questions

- Which file is used where is Task B's asset map (not in this document).
- The 12 8-bpp files without key pixels might still be drawn with colour-key blitting; the format does not say.
