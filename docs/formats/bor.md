# `.bor` outline files

29 files in `data/bor/` (extensions mix `.bor` and `.BOR`; match case-insensitively). Names are food and prop objects: `cacahuete`, `can1..3`, `chips1/2`, `clope`, `cornichon`, `extreme1..3`, `lait`, `mais`, `mandarine`, `muffin`, `palesventi_dt/gc`, `piedventi_dt/gc`, `poivriere`, `pomme`, `poulet`, `saucisson`, `saucisson1`, `sucre`, `toast`, `toaster`, `tomate`, `zippo`. Parser: `thefly_assets::bor`.

## Layout

A flat array of u16 little-endian values, no header. Value `0xFFFF` ends a row. The file holds exactly 500 rows.

```
row 0: x x x ... FFFF
row 1: FFFF            <- empty row
...
row 499: FFFF
```

Row index is the y coordinate (top row = 0) in the object's own pixel space; each value is an x coordinate. Within a row the values are in descending order.

Row 500 does not exist: the last value in the file is the terminator of row 499.

## Meaning (hypothesis, strongly supported)

Each file is the one-pixel-wide outline (border; "bor" is probably short for "bord") of one object's silhouette, stored per scanline. Points are the pixels on the edge of the shape, so:

- A row of a convex shape has two values (right edge, left edge).
- A row on a horizontal edge lists a contiguous run of x values.
- A row crossing a concave part lists more than two values, still descending.
- Rows above and below the shape are empty. Empty leading rows exist (the shape starts at y=2 to y=89 depending on the file).

Likely use: collision or hit testing against the object (the swatter hitting a falling food item, or the fly landing on it). Which of the two is Task B's question.

## Evidence

- Every file: length is even, exactly 500 terminators, nothing after the last one, all rows descending.
- Rendering the points as pixels gives clean closed outlines of recognisable objects: `cacahuete` is a peanut, `toaster` a rounded appliance body, `piedventi_dt` a fan stand with blades. See `thefly-extract` output `bor/*.png`.
- Extent over the 29 files: x in 1..229, outline height 24 to 298 rows. Never anything near 500, so 500 is just a fixed capacity.
- The point count per file is 119 to 826.

## Pairing with `.irc`

There is no same-name `.irc` for any `.bor`. A scan of all 8-bpp `.irc` images for a sprite whose opaque edge matches the outline found no match, so the object art is either in the 24/16-bpp files, in a sprite sheet with another origin, or drawn from another source. The task spec expected a same-name pair; that expectation is false. Validation against an `.irc` is not possible without the exe's asset map.

## Open questions

- Which `.irc` frame each outline belongs to, and the origin (is x relative to the sprite's left edge, or to the centre).
- Whether the outline is 4- or 8-connected and whether it is closed; a few rows end in a duplicated x (`... 8 8`), which looks like the same pixel visited twice by a contour tracer.
- Meaning of the `_dt` and `_gc` suffixes (probably droite/gauche, right/left; French).
