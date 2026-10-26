//! `cargo xtask icon-sets`: the icon libraries FigCraft ships (Lucide, Phosphor, Material Symbols),
//! packed from their published npm packages into `assets/icon-sets/<set>.txt.gz`. Each package is
//! pinned by version and its registry sha512, fetched with `curl`, unpacked with `tar` and checked
//! before anything is read. The packs and the licence files are committed, so building never needs
//! the network; run this only to move to a newer release (and update ATTRIBUTION.md with it).
//!
//! Pack format (UTF-8 text, gzip): header lines `key<TAB>value` up to a `---` line, then one line
//! per icon: `name<TAB>tags (comma separated)<TAB>` then one SVG body per style, in the header's
//! `styles` order. A body is the inside of the `<svg>` element, or `=` when it is the same as the
//! style before it, or empty when the icon has no such style.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine as _;
use sha2::Digest as _;

struct Source {
    set: &'static str,
    title: &'static str,
    package: &'static str,
    version: &'static str,
    tarball: &'static str,
    integrity: &'static str,
    licence: &'static str,
    homepage: &'static str,
    view_box: &'static str,
    attrs: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        set: "lucide",
        title: "Lucide",
        package: "lucide-static",
        version: "1.55.0",
        tarball: "https://registry.npmjs.org/lucide-static/-/lucide-static-1.55.0.tgz",
        integrity: "sha512-h18lunTFsIJWnY0FMRPjXeO9KcAtNi7fDqtqgWHOFbssfgUWsctn/pA7bpiyrd4g1DuBOJ24CsRnPfkYblwm/A==",
        licence: "ISC",
        homepage: "https://lucide.dev",
        view_box: "0 0 24 24",
        attrs: r#"fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round""#,
    },
    Source {
        set: "phosphor",
        title: "Phosphor",
        package: "@phosphor-icons/core",
        version: "2.1.1",
        tarball: "https://registry.npmjs.org/@phosphor-icons/core/-/core-2.1.1.tgz",
        integrity: "sha512-v4ARvrip4qBCImOE5rmPUylOEK4iiED9ZyKjcvzuezqMaiRASCHKcRIuvvxL/twvLpkfnEODCOJp5dM4eZilxQ==",
        licence: "MIT",
        homepage: "https://phosphoricons.com",
        view_box: "0 0 256 256",
        attrs: r#"fill="currentColor""#,
    },
    Source {
        set: "material",
        title: "Material Symbols",
        package: "@material-symbols/svg-400",
        version: "0.48.0",
        tarball: "https://registry.npmjs.org/@material-symbols/svg-400/-/svg-400-0.48.0.tgz",
        integrity: "sha512-W20MjbiDbLJQ5yi4CPz2y6lCZ9bk3pJ1d5Ra+uErvtj/guVrCNfcYyMZn3KqHpDyyKVfBGkuGHVcdi5h+5+EqA==",
        licence: "Apache-2.0",
        homepage: "https://fonts.google.com/icons",
        view_box: "0 -960 960 960",
        attrs: r#"fill="currentColor""#,
    },
];

const PHOSPHOR_STYLES: &[&str] = &["regular", "thin", "light", "bold", "fill", "duotone"];
const MATERIAL_STYLES: &[&str] = &["outlined", "outlined-fill", "rounded", "rounded-fill", "sharp", "sharp-fill"];

pub fn generate(root: &Path) -> bool {
    let work = root.join("target/icon-sets-src");
    let out = root.join("assets/icon-sets");
    if std::fs::create_dir_all(&work).is_err() || std::fs::create_dir_all(&out).is_err() {
        eprintln!("icon-sets: cannot create {} or {}", work.display(), out.display());
        return false;
    }
    let mut ok = true;
    for src in SOURCES {
        match pack(src, &work, &out) {
            Ok(summary) => eprintln!("icon-sets: {summary}"),
            Err(e) => {
                eprintln!("icon-sets: {}: {e}", src.set);
                ok = false;
            }
        }
    }
    ok
}

fn pack(src: &Source, work: &Path, out: &Path) -> Result<String, String> {
    let pkg = fetch(src, work)?;
    let (styles, icons) = match src.set {
        "lucide" => (Vec::new(), lucide(&pkg)?),
        "phosphor" => (PHOSPHOR_STYLES.to_vec(), phosphor(&pkg)?),
        _ => (MATERIAL_STYLES.to_vec(), material(&pkg)?),
    };
    let mut text = format!(
        "figcraft-icon-set\t1\nset\t{}\ntitle\t{}\npackage\t{}@{}\nlicence\t{}\nhomepage\t{}\nviewBox\t{}\nattrs\t{}\nstyles\t{}\n---\n",
        src.set,
        src.title,
        src.package,
        src.version,
        src.licence,
        src.homepage,
        src.view_box,
        src.attrs,
        styles.join(",")
    );
    let count = icons.len();
    for (name, (tags, bodies)) in &icons {
        text.push_str(name);
        text.push('\t');
        text.push_str(&tags.join(","));
        let mut prev: Option<&str> = None;
        for b in bodies {
            text.push('\t');
            match prev {
                Some(p) if !b.is_empty() && p == b => text.push('='),
                _ => text.push_str(b),
            }
            prev = Some(b);
        }
        text.push('\n');
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let bytes = gz.finish().map_err(|e| e.to_string())?;
    let file = out.join(format!("{}.txt.gz", src.set));
    std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
    let licence = std::fs::read(pkg.join("LICENSE")).map_err(|e| format!("LICENSE: {e}"))?;
    std::fs::write(out.join(format!("LICENSE-{}.txt", src.set)), licence).map_err(|e| e.to_string())?;
    Ok(format!("{} {}: {count} icons, {} KB packed ({} KB text)", src.set, src.version, bytes.len() / 1024, text.len() / 1024))
}

/// The package unpacked under `work`, after its tarball matched the pinned sha512.
fn fetch(src: &Source, work: &Path) -> Result<PathBuf, String> {
    let dir = work.join(format!("{}-{}", src.set, src.version));
    let tgz = work.join(format!("{}-{}.tgz", src.set, src.version));
    if !tgz.exists() {
        let st = Command::new("curl").args(["-sSfL", src.tarball, "-o"]).arg(&tgz).status().map_err(|e| format!("curl: {e}"))?;
        if !st.success() {
            return Err(format!("download of {} failed", src.tarball));
        }
    }
    let bytes = std::fs::read(&tgz).map_err(|e| e.to_string())?;
    let digest = format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(&bytes)));
    if digest != src.integrity {
        let _ = std::fs::remove_file(&tgz);
        return Err(format!("{} does not match its pinned sha512 (got {digest}); nothing was read", src.tarball));
    }
    if !dir.join("package").exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let st = Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&dir).status().map_err(|e| format!("tar: {e}"))?;
        if !st.success() {
            return Err("tar failed".into());
        }
    }
    Ok(dir.join("package"))
}

type Icons = BTreeMap<String, (Vec<String>, Vec<String>)>;

fn lucide(pkg: &Path) -> Result<Icons, String> {
    let tags: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(pkg.join("tags.json")).unwrap_or_default()).unwrap_or_default();
    let mut icons = Icons::new();
    for (name, path) in svgs(&pkg.join("icons"))? {
        let t = tags.get(&name).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|t| t.as_str()).map(clean_tag).collect()).unwrap_or_default();
        icons.insert(name, (t, vec![body(&path)?]));
    }
    Ok(icons)
}

fn phosphor(pkg: &Path) -> Result<Icons, String> {
    let meta = std::fs::read_to_string(pkg.join("dist/index.mjs")).unwrap_or_default();
    let tags = phosphor_tags(&meta);
    let mut icons = Icons::new();
    for (i, style) in PHOSPHOR_STYLES.iter().enumerate() {
        for (file, path) in svgs(&pkg.join("assets").join(style))? {
            let name = if *style == "regular" { file } else { file.strip_suffix(&format!("-{style}")).unwrap_or(&file).to_string() };
            let entry = icons
                .entry(name.clone())
                .or_insert_with(|| (tags.get(&name).cloned().unwrap_or_default(), vec![String::new(); PHOSPHOR_STYLES.len()]));
            if let Some(slot) = entry.1.get_mut(i) {
                *slot = body(&path)?;
            }
        }
    }
    Ok(icons)
}

/// `name: "…"` then its `tags: [ … ]` (and an alias's name) from the package's metadata module.
fn phosphor_tags(meta: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for chunk in meta.split("\n  {\n").skip(1) {
        let quoted = |key: &str| chunk.split(key).nth(1).and_then(|r| r.split('"').nth(1)).map(str::to_string);
        let Some(name) = quoted("    name: ") else { continue };
        let mut tags: Vec<String> = chunk
            .split("tags: [")
            .nth(1)
            .and_then(|r| r.split(']').next())
            // `*new*` / `*updated*` are release markers, not words to search by.
            .map(|list| list.split('"').skip(1).step_by(2).filter(|t| !t.starts_with('*')).map(clean_tag).collect())
            .unwrap_or_default();
        if let Some(alias) = chunk.split("alias: { name: ").nth(1).and_then(|r| r.split('"').nth(1)) {
            tags.push(clean_tag(alias));
        }
        out.insert(name, tags);
    }
    out
}

fn material(pkg: &Path) -> Result<Icons, String> {
    let mut icons = Icons::new();
    for (i, style) in MATERIAL_STYLES.iter().enumerate() {
        let (dir, fill) = style.split_once('-').map_or((*style, false), |(d, _)| (d, true));
        for (file, path) in svgs(&pkg.join(dir))? {
            let name = match (fill, file.strip_suffix("-fill")) {
                (true, Some(n)) => n.to_string(),
                (false, None) => file,
                _ => continue,
            };
            let entry = icons.entry(name).or_insert_with(|| (Vec::new(), vec![String::new(); MATERIAL_STYLES.len()]));
            if let Some(slot) = entry.1.get_mut(i) {
                *slot = body(&path)?;
            }
        }
    }
    Ok(icons)
}

/// Every `*.svg` in `dir` as (file stem, path), sorted.
fn svgs(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out: Vec<(String, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "svg"))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().to_string(), p)))
        .collect();
    out.sort();
    Ok(out)
}

/// The inside of a file's `<svg>` element on one line.
fn body(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let start = text.find("<svg").and_then(|i| text.get(i..)?.find('>').map(|j| i + j + 1)).ok_or_else(|| format!("{}: no <svg>", path.display()))?;
    let end = text.rfind("</svg>").ok_or_else(|| format!("{}: no </svg>", path.display()))?;
    let inner = text.get(start..end).unwrap_or_default();
    Ok(inner.split_whitespace().collect::<Vec<_>>().join(" ").replace("> <", "><"))
}

fn clean_tag(t: &str) -> String {
    t.replace([',', '\t', '\n'], " ").trim().to_lowercase()
}
