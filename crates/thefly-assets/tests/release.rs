//! Parses every real file of a release. Skipped unless `THEFLY_DATA` points at the release dir.

use std::path::PathBuf;
use thefly_assets::{bor, btt, files_with_ext, irc};

fn release() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("THEFLY_DATA")?);
    dir.is_dir().then_some(dir)
}

#[test]
fn every_irc_parses_and_header_matches_bmp() {
    let Some(root) = release() else { return };
    let files = files_with_ext(&root.join("data/rsc"), "irc").unwrap();
    assert_eq!(files.len(), 135);
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let irc = irc::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let bmp_size = u32::from_le_bytes(bytes[18..22].try_into().unwrap());
        assert_eq!(irc.declared_len, bmp_size, "{}", f.display());
        assert_eq!(&bytes[4..8], &[0; 4], "{}", f.display());
    }
}

#[test]
fn every_bor_parses_with_descending_rows() {
    let Some(root) = release() else { return };
    let files = files_with_ext(&root.join("data/bor"), "bor").unwrap();
    assert_eq!(files.len(), 29);
    for f in files {
        let bor = bor::parse(&std::fs::read(&f).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(bor.height() > 0, "{}", f.display());
        assert!(
            bor.rows.iter().all(|r| r.is_sorted_by(|a, b| a >= b)),
            "{}",
            f.display()
        );
    }
}

#[test]
fn save_files_parse() {
    let Some(root) = release() else { return };
    let scores = btt::parse_scores(&std::fs::read(root.join("scoot.btt")).unwrap()).unwrap();
    assert!(scores.entries.iter().all(|e| e.name == "None"));
    // game-spec §7.1: record 0 is score 1 with checksum c(1) = 1.74625; the trailer is c(sum).
    assert_eq!(scores.entries[0].score, 1);
    assert!((scores.entries[0].checksum - 1.74625).abs() < 1e-5);
    assert_eq!(scores.trailer, scores.entries[0].checksum);
    let checksum = |s: i32| {
        let x = 5.365799563 * f64::from(s) - 5.365799563_f64.sqrt();
        (if x < 0.0 { 0.1835 } else { x }).sqrt()
    };
    for e in scores.entries.iter().filter(|e| e.score != 0) {
        assert!((e.checksum - checksum(e.score)).abs() < 1e-9, "{e:?}");
    }
    let s = btt::parse_settings(&std::fs::read(root.join("boite.btt")).unwrap()).unwrap();
    assert_eq!(s.values, [1, 1]);
}
