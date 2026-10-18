//! Asset-derived numbers the rules need: sprite frame sizes and collision outlines.
//!
//! These are gameplay inputs (spawn ranges, clamps, hit tests), not presentation, so the sim takes
//! them at construction. The frontend fills a [`GameData`] from the `.irc` and `.bor` files and
//! calls [`GameData::check`]; tests use [`GameData::synthetic`].

use std::collections::BTreeMap;

use crate::world::W;
use crate::world::objects::{EXTREME_TYPES, NORMAL_TYPES};

/// One animation frame's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

/// A collision outline: per row (y from the sprite's top), outline x values in 2-px units from
/// the sprite's left edge (fly-and-world §4.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outline {
    pub rows: Vec<Vec<u16>>,
}

impl From<thefly_assets::bor::Bor> for Outline {
    fn from(b: thefly_assets::bor::Bor) -> Self {
        Self { rows: b.rows }
    }
}

/// Sprites whose frame size the rules read, besides the table objects.
pub const SIZED_SPRITES: &[&str] = &[
    "Face",
    "Paysage",
    "paysage2",
    "Jardin",
    "collines",
    "Maison",
    "dehors",
    "arene",
    "Table",
    "table2",
    "treuil",
    "treuil_corde",
    "i_botte",
    "colle_mouche1",
    "colle_mouche2",
    "ventilo_gauche",
    "ventilo_droite",
    "tue_mouche",
    "Goodies",
    "moucheron",
    "piece",
    "piecetap",
];

/// Outlines besides the table objects'. `tue_mouche` is derived from the sprite mask by the
/// frontend (`FUN_0042dda4`: edge pixels at half width); the rest are `.bor` files.
pub const OUTLINED_SPRITES: &[&str] = &[
    "palesventi_gc",
    "palesventi_dt",
    "piedventi_gc",
    "piedventi_dt",
    "tue_mouche",
];

/// What [`GameData::check`] found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    Size(String),
    Outline(String),
    /// A main scenery layer whose width is not [`crate::world::W`] (gaps §4.2).
    WorldWidth(String, i32),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameData {
    pub sizes: BTreeMap<String, Size>,
    pub outlines: BTreeMap<String, Outline>,
}

impl GameData {
    /// Frame size of `name`. Panics on a name [`GameData::check`] would have reported.
    pub fn size(&self, name: &str) -> Size {
        match self.sizes.get(name) {
            Some(s) => *s,
            None => panic!("sprite size {name:?} missing; GameData::check was skipped"),
        }
    }

    pub fn outline(&self, name: &str) -> &Outline {
        match self.outlines.get(name) {
            Some(o) => o,
            None => panic!("outline {name:?} missing; GameData::check was skipped"),
        }
    }

    /// Every name the rules look up, so a bad data directory fails at load, not mid-match.
    pub fn check(&self) -> Result<(), Missing> {
        let objects = NORMAL_TYPES.iter().chain(&EXTREME_TYPES).map(|t| t.name);
        for name in SIZED_SPRITES.iter().copied().chain(objects.clone()) {
            if !self.sizes.contains_key(name) {
                return Err(Missing::Size(name.to_owned()));
            }
        }
        for name in ["Maison", "dehors", "arene"] {
            let w = self.size(name).w;
            if w != W {
                return Err(Missing::WorldWidth(name.to_owned(), w));
            }
        }
        for name in OUTLINED_SPRITES.iter().copied().chain(objects) {
            if !self.outlines.contains_key(name) {
                return Err(Missing::Outline(name.to_owned()));
            }
        }
        Ok(())
    }

    /// Test fixture. Sizes from the spec where it gives them (fly 120 × 94, the asset table in
    /// game-spec §9.3); the rest, and every outline, are placeholder rectangles with no claim to
    /// match the shipped data.
    pub fn synthetic() -> Self {
        let mut d = GameData::default();
        let known = [
            ("Face", 120, 94),
            ("Paysage", 800, 286),
            ("paysage2", 1000, 384),
            ("Jardin", 800, 217),
            ("collines", 2000, 258),
            ("Maison", 4000, 553),
            ("dehors", 4000, 295),
            ("arene", 4000, 402),
            ("Table", 900, 79),
            ("table2", 900, 79),
            ("treuil", 235, 151),
            ("treuil_corde", 7, 380),
            ("i_botte", 270, 575),
            ("colle_mouche1", 53, 300),
            ("colle_mouche2", 53, 300),
            ("ventilo_gauche", 666, 300),
            ("ventilo_droite", 666, 300),
            ("tue_mouche", 635, 131),
            ("Goodies", 51, 50),
            ("moucheron", 27, 27),
            ("piece", 45, 45),
            ("piecetap", 48, 44),
        ];
        for (name, w, h) in known {
            d.sizes.insert(name.to_owned(), Size { w, h });
        }
        for t in NORMAL_TYPES.iter().chain(&EXTREME_TYPES) {
            let size = Size {
                w: 2 * t.grip_x,
                h: 100,
            };
            d.sizes.insert(t.name.to_owned(), size);
            d.outlines.insert(t.name.to_owned(), Outline::rect(size));
        }
        for name in OUTLINED_SPRITES {
            d.outlines
                .insert((*name).to_owned(), Outline::rect(Size { w: 100, h: 100 }));
        }
        d
    }
}

impl Outline {
    /// A hollow rectangle covering the whole frame, in the `.bor` units.
    pub fn rect(size: Size) -> Self {
        let right = u16::try_from(size.w / 2 - 1).unwrap_or(0);
        let h = usize::try_from(size.h).unwrap_or(0);
        let rows = (0..h)
            .map(|y| {
                if y == 0 || y + 1 == h {
                    (0..=right).collect()
                } else {
                    vec![0, right]
                }
            })
            .collect();
        Self { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_data_passes_the_check() {
        let mut d = GameData::synthetic();
        assert_eq!(d.check(), Ok(()));
        assert_eq!(d.size("Face"), Size { w: 120, h: 94 });
        assert_eq!(
            GameData::default().check(),
            Err(Missing::Size("Face".into()))
        );
        d.sizes.insert("dehors".into(), Size { w: 3000, h: 295 });
        assert_eq!(d.check(), Err(Missing::WorldWidth("dehors".into(), 3000)));
    }
}
