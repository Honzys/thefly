//! Where the game's logical asset names live on disk, and the start-up loads built from them:
//! [`GameData`], the sound files per slot and the saves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thefly_sim::GameData;
use thefly_sim::data::{OUTLINED_SPRITES, Outline, SIZED_SPRITES, Size};
use thefly_sim::event::SOUND_FILES;
use thefly_sim::saves::{self, Persisted};
use thefly_sim::world::objects::{EXTREME_TYPES, NORMAL_TYPES};

/// A logical sprite: its `.irc` file in `data/rsc/` and its frame cut (SR §1.2, row-major).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sheet {
    pub file: &'static str,
    pub cols: u32,
    pub rows: u32,
}

/// game-spec §9 file map with the cuts from screens-and-rendering §2, §3, §4.6 and §5.
const SHEETS: &[(&str, &str, u32, u32)] = &[
    ("highscores", "ac0.irc", 1, 1),
    ("highscores_color", "ac1.irc", 1, 1),
    ("i_botte", "ac2.irc", 1, 1),
    ("i_cochon_corps", "ac3.irc", 1, 1),
    ("i_cochon_tete", "ac4.irc", 2, 1),
    ("i_credit_gaga", "ac5.irc", 3, 1),
    ("i_credit_seb", "ac6.irc", 3, 1),
    ("i_decors1", "ac7.irc", 1, 1),
    ("i_decors2", "ac8.irc", 1, 1),
    ("i_items1", "ac9.irc", 2, 4),
    ("i_nuage", "ac10.irc", 3, 1),
    ("i_PetiteMouche", "ac11.irc", 4, 1),
    ("i_soleil", "ac12.irc", 4, 2),
    ("Present", "ac13.irc", 1, 1),
    ("touches", "ac14.irc", 1, 1),
    ("page_mongol", "ac15.irc", 1, 1),
    ("boitedlg", "div0.irc", 1, 1),
    ("cacahuete", "div1.irc", 1, 1),
    ("can1", "div2.irc", 1, 1),
    ("can2", "div3.irc", 1, 1),
    ("can3", "div4.irc", 1, 1),
    ("chips1", "div5.irc", 1, 1),
    ("chips2", "div6.irc", 1, 1),
    ("clope", "div7.irc", 1, 1),
    ("color_shade", "div8.irc", 4, 1),
    ("cornichon", "div9.irc", 1, 1),
    ("DUST", "div10.irc", 3, 1),
    ("extreme1", "div11.irc", 1, 1),
    ("extreme2", "div12.irc", 1, 1),
    ("extreme3", "div13.irc", 1, 1),
    ("Goodies", "div14.irc", 5, 2),
    ("goodiesmystere", "div15.irc", 2, 1),
    ("lait", "div16.irc", 1, 1),
    ("LOADING", "div17.irc", 1, 1),
    ("LOADING2", "div18.irc", 1, 1),
    ("logo", "div19.irc", 1, 1),
    ("mais", "div20.irc", 1, 1),
    ("mandarine", "div21.irc", 1, 1),
    ("muffin", "div22.irc", 1, 1),
    ("poivriere", "div23.irc", 1, 1),
    ("pomme", "div24.irc", 1, 1),
    ("poulet", "div25.irc", 1, 1),
    ("saucisson", "div26.irc", 1, 1),
    ("saucisson1", "div27.irc", 1, 1),
    ("spriteload", "div28.irc", 3, 1),
    ("sucre", "div29.irc", 1, 1),
    ("toast", "div30.irc", 1, 1),
    ("toaster", "div31.irc", 1, 1),
    ("tomate", "div32.irc", 1, 1),
    ("winnerbck", "div33.irc", 1, 1),
    ("winnercoupe", "div34.irc", 1, 1),
    ("winnerfarmer", "div35.irc", 1, 1),
    ("winnerfly", "div36.irc", 1, 1),
    ("you_win", "div37.irc", 1, 1),
    ("zippo", "div38.irc", 1, 1),
    ("ailesgoodies", "ig0.irc", 3, 1),
    ("arene", "ig1.irc", 1, 1),
    ("Aspi", "ig2.irc", 10, 1),
    ("bckbonusbarre", "ig3.irc", 1, 1),
    ("bckbonusfond", "ig4.irc", 1, 1),
    ("blood", "ig5.irc", 3, 2),
    ("charbon", "ig6.irc", 3, 1),
    ("colle_mouche1", "ig7.irc", 1, 1),
    ("colle_mouche2", "ig8.irc", 1, 1),
    ("collines", "ig9.irc", 1, 1),
    ("crash-face0001", "ig10.irc", 1, 1),
    ("crash-face0002", "ig11.irc", 1, 1),
    ("crash-face0003", "ig12.irc", 1, 1),
    ("crash-face0004", "ig13.irc", 1, 1),
    ("crash-face0011", "ig14.irc", 1, 1),
    ("croix_goodies", "ig15.irc", 1, 1),
    ("dehors", "ig16.irc", 1, 1),
    ("eclaires", "ig17.irc", 4, 1),
    ("eclatement_fly", "ig18.irc", 11, 1),
    ("etincelle", "ig19.irc", 3, 1),
    ("Face", "ig20.irc", 10, 1),
    ("fleche_tap", "ig21.irc", 1, 1),
    ("horloge", "ig22.irc", 1, 1),
    ("Jardin", "ig23.irc", 1, 1),
    ("Maison", "ig24.irc", 1, 1),
    ("moucheron", "ig25.irc", 6, 1),
    ("moucheron1", "ig26.irc", 6, 1),
    ("onde_active", "ig27.irc", 2, 1),
    ("onde_frappe", "ig28.irc", 1, 1),
    ("Paysage", "ig29.irc", 1, 1),
    ("paysage2", "ig30.irc", 1, 1),
    ("piece", "ig31.irc", 15, 1),
    ("piecetap", "ig32.irc", 1, 1),
    ("plaque_tap", "ig33.irc", 1, 1),
    ("stardust", "ig34.irc", 3, 1),
    ("Table", "ig35.irc", 1, 1),
    ("table2", "ig36.irc", 1, 1),
    ("tapette-1", "ig37.irc", 1, 1),
    ("tapette-3", "ig38.irc", 1, 1),
    ("tapette-5", "ig39.irc", 1, 1),
    ("tapette-7", "ig40.irc", 1, 1),
    ("tapette-8", "ig41.irc", 1, 1),
    ("transi-est", "ig42.irc", 10, 1),
    ("transi-nord", "ig43.irc", 10, 1),
    ("transi-nord-est", "ig44.irc", 10, 1),
    ("transi-nord-ouest", "ig45.irc", 10, 1),
    ("transi-ouest", "ig46.irc", 10, 1),
    ("transi-sud", "ig47.irc", 10, 1),
    ("transi-sud-est", "ig48.irc", 10, 1),
    ("transi-sud-ouest", "ig49.irc", 10, 1),
    ("trenblood", "ig50.irc", 1, 1),
    ("treuil", "ig51.irc", 1, 1),
    ("treuil_corde", "ig52.irc", 1, 1),
    ("tue_mouche", "ig53.irc", 1, 1),
    ("tutoBulFarmer", "ig54.irc", 2, 1),
    ("tutoBulFly", "ig55.irc", 2, 1),
    ("vent", "ig56.irc", 4, 1),
    ("ventilo_droite", "ig57.irc", 1, 1),
    ("ventilo_gauche", "ig58.irc", 1, 1),
    ("g_background", "iig0.irc", 1, 1),
    ("g_barre_haut_tapette", "iig1.irc", 1, 1),
    ("g_barres_bas", "iig2.irc", 1, 2),
    ("g_barres_tapis", "iig3.irc", 1, 1),
    ("g_blatte", "iig4.irc", 2, 1),
    ("g_bourse_cache", "iig5.irc", 2, 1),
    ("g_boursebg", "iig6.irc", 1, 1),
    ("g_classe_bourse", "iig7.irc", 2, 1),
    ("g_flymort", "iig8.irc", 3, 1),
    ("g_Machine", "iig9.irc", 1, 1),
    ("g_Machine_cache", "iig10.irc", 1, 1),
    ("g_Selection", "iig11.irc", 3, 1),
    ("g_TapiRoulant", "iig12.irc", 1, 3),
    ("p_croix", "iig13.irc", 1, 1),
    ("p_fleche", "iig14.irc", 1, 3),
    ("p_icone_colle_mouche", "iig15.irc", 1, 1),
    ("p_icone_treuil", "iig16.irc", 1, 1),
    ("p_icone_tue_mouche", "iig17.irc", 1, 1),
    ("p_icone_ventilo_droite", "iig18.irc", 1, 1),
    ("p_icone_ventilo_gauche", "iig19.irc", 1, 1),
    ("p_traps", "iig20.irc", 1, 1),
];

/// The sheet for a logical name; names match exactly, as the sim spells them.
pub fn sheet(name: &str) -> Option<Sheet> {
    SHEETS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|&(_, file, cols, rows)| Sheet { file, cols, rows })
}

/// Joint, the shipped display face (SR "Joint").
pub const JOINT: &str = "JOINPD__.TTF";
/// shortcut: Arial Black is a Windows system face and is not shipped: a user-supplied copy in `rsc/`,
/// then the OS fonts, then the shipped `casual.ttf` (wider, so wide text goes through `Gfx::text_fit`)
/// stands in until a metric-compatible free face is chosen (SR §2.3, §3.3, §5.2, §5.3).
pub const ARIAL_BLACK: &str = "ariblk.ttf|Arial Black.ttf|casual.ttf";

/// Case-insensitive lookup of `stem.wav` among the files of `dir`.
fn find_case_insensitive(files: &[PathBuf], stem: &str, ext: &str) -> Option<String> {
    files.iter().find_map(|p| {
        let name = p.file_name()?.to_str()?;
        let (s, e) = name.rsplit_once('.')?;
        (s.eq_ignore_ascii_case(stem) && e.eq_ignore_ascii_case(ext)).then(|| name.to_owned())
    })
}

/// The on-disk file name of every [`SOUND_FILES`] slot, by slot index.
pub fn sound_files(sons: &Path) -> Result<Vec<String>, String> {
    let files = thefly_assets::files_with_ext(sons, "wav")
        .map_err(|e| format!("{}: {e}", sons.display()))?;
    SOUND_FILES
        .iter()
        .map(|stem| {
            find_case_insensitive(&files, stem, "wav")
                .ok_or_else(|| format!("sound {stem}.wav missing in {}", sons.display()))
        })
        .collect()
}

/// The UI click (`Event::Click`), `i_bouton.WAV` on disk.
pub fn click_file(sons: &Path) -> Option<String> {
    let files = thefly_assets::files_with_ext(sons, "wav").ok()?;
    find_case_insensitive(&files, "i_bouton", "wav")
}

fn read_irc(data: &Path, file: &str) -> Result<thefly_assets::irc::Irc, String> {
    let path = data.join("rsc").join(file);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    thefly_assets::irc::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Frame size from a sheet's pixel size and its cut.
pub fn frame_size(sheet: Sheet, size: (u32, u32)) -> Size {
    let w = size.0 / sheet.cols.max(1);
    let h = size.1 / sheet.rows.max(1);
    Size {
        w: i32::try_from(w).unwrap_or(i32::MAX),
        h: i32::try_from(h).unwrap_or(i32::MAX),
    }
}

/// `FUN_0042dda4` as fly-and-world §4.2 describes it: per row, the opaque pixels that touch
/// transparency or the image edge, at half x (the `.bor` 2-px units).
pub fn mask_outline(width: u32, height: u32, rgba: &[u8]) -> Outline {
    let (w, h) = (width as usize, height as usize);
    let opaque = |x: usize, y: usize| rgba.get((y * w + x) * 4 + 3).is_some_and(|&a| a > 0);
    let solid = |x: isize, y: isize| {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && opaque(x as usize, y as usize)
    };
    let edge = |x: usize, y: usize| {
        let (xi, yi) = (x as isize, y as isize);
        let around = [(xi - 1, yi), (xi + 1, yi), (xi, yi - 1), (xi, yi + 1)];
        opaque(x, y) && around.iter().any(|&(ax, ay)| !solid(ax, ay))
    };
    let rows = (0..h)
        .map(|y| {
            let mut xs: Vec<u16> = (0..w)
                .filter(|&x| edge(x, y))
                .filter_map(|x| u16::try_from(x / 2).ok())
                .collect();
            xs.dedup();
            xs
        })
        .collect();
    Outline { rows }
}

fn bor_files(data: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let dir = data.join("bor");
    let files = thefly_assets::files_with_ext(&dir, "bor")
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(files
        .into_iter()
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_ascii_lowercase();
            Some((stem, p))
        })
        .collect())
}

fn read_bor(path: &Path) -> Result<Outline, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bor = thefly_assets::bor::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Outline::from(bor))
}

/// The rules' cut where it differs from the sheet: the bonus round loads `piece` 45 × 45 out of
/// its 45 × 50 frames (SR §9 table, gaps §7.3 coinH = 45).
fn rule_size(name: &str, s: Size) -> Size {
    match name {
        "piece" => Size { w: s.w, h: 45 },
        _ => s,
    }
}

/// Builds and checks everything the rules read from the data directory.
pub fn game_data(data: &Path) -> Result<GameData, String> {
    let mut d = GameData::default();
    let objects = NORMAL_TYPES.iter().chain(&EXTREME_TYPES).map(|t| t.name);
    for name in SIZED_SPRITES.iter().copied().chain(objects.clone()) {
        let sheet = sheet(name).ok_or_else(|| format!("no file for sprite {name}"))?;
        let irc = read_irc(data, sheet.file)?;
        let size = (irc.image.width, irc.image.height);
        d.sizes
            .insert(name.to_owned(), rule_size(name, frame_size(sheet, size)));
        if name == "tue_mouche" {
            let img = &irc.image;
            let outline = mask_outline(img.width, img.height, &img.rgba);
            d.outlines.insert(name.to_owned(), outline);
        }
    }
    let bors = bor_files(data)?;
    let bor_names = OUTLINED_SPRITES.iter().copied().chain(objects);
    for name in bor_names.filter(|n| *n != "tue_mouche") {
        let path = bors
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| format!("{name}.bor missing"))?;
        d.outlines.insert(name.to_owned(), read_bor(path)?);
    }
    d.check()
        .map_err(|m| format!("bad data directory: {m:?}"))?;
    Ok(d)
}

/// Where a save file is read from: the working directory first (where the game writes it), then
/// the release folder that holds `data/`.
fn save_path(data: &Path, file: &str) -> Option<PathBuf> {
    let here = PathBuf::from(file);
    if here.is_file() {
        return Some(here);
    }
    let release = data.parent()?.join(file);
    release.is_file().then_some(release)
}

/// The saves through the sim's decoders; a missing or corrupt file keeps the default.
pub fn persisted(data: &Path) -> Persisted {
    let read = |file| save_path(data, file).and_then(|path| std::fs::read(path).ok());
    let mut p = Persisted::default();
    if let Some(o) = read("boite.btt").and_then(|b| saves::decode_options(&b).ok()) {
        p.options = o;
    }
    if let Some(s) = read("scoot.btt").and_then(|b| saves::decode_scores(&b).ok()) {
        p.scores = s;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> Option<PathBuf> {
        let d = PathBuf::from(std::env::var_os("THEFLY_DATA")?).join("data");
        d.is_dir().then_some(d)
    }

    #[test]
    fn every_sim_sprite_has_a_sheet() {
        let objects = NORMAL_TYPES.iter().chain(&EXTREME_TYPES).map(|t| t.name);
        for name in SIZED_SPRITES.iter().copied().chain(objects) {
            assert!(sheet(name).is_some(), "{name}");
        }
        assert_eq!(sheet("Face").map(|s| s.file), Some("ig20.irc"));
        assert_eq!(sheet("p_traps").map(|s| s.file), Some("iig20.irc"));
    }

    #[test]
    fn frame_size_divides_the_sheet() {
        let face = sheet("Face").unwrap();
        assert_eq!(frame_size(face, (1200, 94)), Size { w: 120, h: 94 });
        let goodies = sheet("Goodies").unwrap();
        assert_eq!(frame_size(goodies, (255, 100)), Size { w: 51, h: 50 });
    }

    #[test]
    fn mask_outline_keeps_border_pixels_at_half_x() {
        // 4 x 3, fully opaque: rows 0 and 2 are all edge, row 1 only its ends
        let rgba = vec![255; 4 * 3 * 4];
        let o = mask_outline(4, 3, &rgba);
        assert_eq!(o.rows, vec![vec![0, 1], vec![0, 1], vec![0, 1]]);
        // a transparent pixel has no outline point
        let mut rgba = vec![0; 3 * 4];
        rgba[4 + 3] = 255;
        assert_eq!(mask_outline(3, 1, &rgba).rows, vec![vec![0]]);
    }

    #[test]
    fn every_sound_slot_resolves_to_a_file_in_sons() {
        let Some(data) = data_dir() else {
            eprintln!("THEFLY_DATA unset; skipping");
            return;
        };
        let files = sound_files(&data.join("sons")).unwrap();
        assert_eq!(files.len(), SOUND_FILES.len());
        assert_eq!(files[15], "j_birds2.wav");
        for f in &files {
            assert!(data.join("sons").join(f).is_file(), "{f}");
        }
        assert!(click_file(&data.join("sons")).is_some());
    }

    #[test]
    fn real_game_data_matches_the_spec_sizes() {
        let Some(data) = data_dir() else {
            eprintln!("THEFLY_DATA unset; skipping");
            return;
        };
        let real = game_data(&data).unwrap();
        let spec = GameData::synthetic();
        for name in SIZED_SPRITES {
            assert_eq!(real.size(name), spec.size(name), "{name}");
        }
        let zapper = real.outline("tue_mouche");
        assert_eq!(zapper.rows.len(), 131);
    }
}
