//! Dump a Fly release to PNGs, copied sounds and `manifest.json`.
//! Usage: thefly-extract <release-dir> <out-dir>

use serde_json::{Value, json};
use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use thefly_assets::{bor, btt, files_with_ext, irc};

type Res<T> = Result<T, Box<dyn Error>>;

fn write_png(path: &Path, w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Res<()> {
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), w, h);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(data)?;
    Ok(())
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_lowercase()
}

fn extract_irc(path: &Path, out: &Path) -> Res<Value> {
    let irc = irc::parse(&fs::read(path)?)?;
    let img = &irc.image;
    let name = stem(path);
    write_png(
        &out.join(format!("{name}.png")),
        img.width,
        img.height,
        png::ColorType::Rgba,
        &img.rgba,
    )?;
    Ok(
        json!({ "name": name, "width": img.width, "height": img.height,
        "bits_per_pixel": irc.bits_per_pixel, "declared_len": irc.declared_len }),
    )
}

fn extract_bor(path: &Path, out: &Path) -> Res<Value> {
    let bor = bor::parse(&fs::read(path)?)?;
    let (w, h) = (bor.width().max(1), bor.height().max(1));
    let mut gray = vec![0u8; w * h];
    for (x, y) in bor.points() {
        gray[y * w + usize::from(x)] = 255;
    }
    let name = stem(path);
    write_png(
        &out.join(format!("{name}.png")),
        w as u32,
        h as u32,
        png::ColorType::Grayscale,
        &gray,
    )?;
    Ok(
        json!({ "name": name, "width": bor.width(), "height": bor.height(),
        "points": bor.points().count() }),
    )
}

/// Run `f` on every file; failures go to stderr and are counted, not fatal.
fn each(
    dir: &Path,
    ext: &str,
    failures: &mut u32,
    mut f: impl FnMut(&Path) -> Res<Value>,
) -> Res<Vec<Value>> {
    let mut rows = Vec::new();
    for path in files_with_ext(dir, ext)? {
        match f(&path) {
            Ok(v) => rows.push(v),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                *failures += 1;
            }
        }
    }
    Ok(rows)
}

fn copy_sounds(dir: &Path, out: &Path) -> Res<Vec<Value>> {
    fs::create_dir_all(out)?;
    let mut names = Vec::new();
    for ext in ["wav", "mid"] {
        for p in files_with_ext(dir, ext)? {
            let file = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            fs::copy(&p, out.join(&file))?;
            names.push(Value::String(file));
        }
    }
    Ok(names)
}

fn save_files(root: &Path) -> Res<Value> {
    let scores = btt::parse_scores(&fs::read(root.join("scoot.btt"))?)?;
    let settings = btt::parse_settings(&fs::read(root.join("boite.btt"))?)?;
    let entries: Vec<Value> = scores
        .entries
        .iter()
        .map(|e| json!({ "name": e.name, "score": e.score }))
        .collect();
    Ok(json!({ "scores": entries, "scores_trailer": scores.trailer, "settings": settings.values }))
}

fn run(root: &Path, out: &Path) -> Res<u32> {
    let (png_dir, bor_dir) = (out.join("png"), out.join("bor"));
    fs::create_dir_all(&png_dir)?;
    fs::create_dir_all(&bor_dir)?;
    let mut failures = 0;
    let irc = each(&root.join("data/rsc"), "irc", &mut failures, |p| {
        extract_irc(p, &png_dir)
    })?;
    let bor = each(&root.join("data/bor"), "bor", &mut failures, |p| {
        extract_bor(p, &bor_dir)
    })?;
    let sons = copy_sounds(&root.join("data/sons"), &out.join("sons"))?;
    let manifest = json!({ "irc": irc, "bor": bor, "sons": sons, "btt": save_files(root)? });
    fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!(
        "irc: {}, bor: {}, sons: {}, failures: {failures}",
        irc.len(),
        bor.len(),
        sons.len()
    );
    Ok(failures)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, root, out] = args.as_slice() else {
        eprintln!("usage: thefly-extract <release-dir> <out-dir>");
        std::process::exit(2);
    };
    match run(Path::new(root), Path::new(out)) {
        Ok(0) => {}
        Ok(_) => std::process::exit(1),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
