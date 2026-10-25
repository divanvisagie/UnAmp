//! Converts classic Winamp 2 skins (`.wsz`, a zip of bitmaps and text
//! files) into UnAmp TOML skins. Only colours carry over — UnAmp's windows
//! are egui widgets, not bitmaps (see ADR-0009):
//!
//! - `pledit.txt`: playlist text, current-track, background and selection colours
//! - `viscolor.txt`: the analyzer background, bar gradient and peak dots
//! - `numbers.bmp` / `nums_ex.bmp`: the time digits' colour
//! - `main.bmp`: the main window's overall tone, for window backgrounds
//!
//! Skins pair their colours for Winamp's own bitmaps, not for egui's
//! panels, so every text colour is checked against what it's drawn on and
//! moved until it can be read (ADR-0033).

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use egui::Color32;

/// Files we read from a skin; anything else in the archive is ignored.
const WANTED: &[&str] = &["pledit.txt", "viscolor.txt", "numbers.bmp", "nums_ex.bmp", "main.bmp"];
/// Per-file read limit, so a malformed or hostile archive can't exhaust memory.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Colours pulled out of a `.wsz`. Every field is optional because skins
/// routinely omit files and fall back to Winamp's base skin.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WszColors {
    pub normal: Option<Color32>,
    pub current: Option<Color32>,
    pub normal_bg: Option<Color32>,
    pub selected_bg: Option<Color32>,
    /// The 24 `viscolor.txt` entries, if present and complete.
    pub vis: Option<Vec<Color32>>,
    pub digits: Option<Color32>,
    pub main_tone: Option<Color32>,
}

/// Extracts the named files (matched case-insensitively by base name,
/// whatever folder they're in) from a skin archive, each capped at
/// `MAX_FILE_BYTES`. Keys are the lowercase base names.
pub fn read_files<R: Read + Seek>(reader: R, wanted: &[&str]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("not a skin archive: {e}"))?;
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Skins are often zipped with a top-level folder and in any case.
        let Ok(name) = entry.name().map(|n| n.to_string()) else {
            continue;
        };
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_ascii_lowercase();
        if !wanted.contains(&base.as_str()) || files.contains_key(&base) {
            continue;
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_FILE_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        files.insert(base, bytes);
    }
    Ok(files)
}

/// Reads the colour-bearing files out of a `.wsz` archive.
pub fn read_wsz<R: Read + Seek>(reader: R) -> Result<WszColors, String> {
    let files = read_files(reader, WANTED)?;
    let text = |name: &str| files.get(name).map(|b| String::from_utf8_lossy(b).into_owned());
    let mut colors = WszColors::default();
    if let Some(pledit) = text("pledit.txt") {
        let get = |key: &str| ini_value(&pledit, key).and_then(|v| parse_colour(&v));
        colors.normal = get("normal");
        colors.current = get("current");
        colors.normal_bg = get("normalbg");
        colors.selected_bg = get("selectedbg");
    }
    if let Some(vis) = text("viscolor.txt") {
        colors.vis = parse_viscolor(&vis);
    }
    let bitmap = |name: &str| files.get(name).and_then(|b| image::load_from_memory(b).ok());
    if let Some(img) = bitmap("numbers.bmp").or_else(|| bitmap("nums_ex.bmp")) {
        colors.digits = brightest(&img.into_rgb8());
    }
    if let Some(img) = bitmap("main.bmp") {
        colors.main_tone = average(&img.into_rgb8());
    }
    Ok(colors)
}

/// Case-insensitive `key=value` lookup, as Winamp itself is lax about case.
pub fn ini_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_string())
    })
}

/// `#RRGGBB` or bare `RRGGBB`, as found in `pledit.txt`.
pub fn parse_colour(s: &str) -> Option<Color32> {
    let hex = s.trim().trim_start_matches('#');
    if hex.len() < 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// Parses `r,g,b, // comment` lines; returns all 24 or nothing.
pub fn parse_viscolor(text: &str) -> Option<Vec<Color32>> {
    let colors: Vec<Color32> = text
        .lines()
        .filter_map(|line| {
            let data = line.split("//").next()?;
            let mut nums = data
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<u8>().ok());
            Some(Color32::from_rgb(nums.next()??, nums.next()??, nums.next()??))
        })
        .take(24)
        .collect();
    (colors.len() == 24).then_some(colors)
}

fn luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

/// The brightest pixel: digit bitmaps are lit glyphs on a dark background.
fn brightest(img: &image::RgbImage) -> Option<Color32> {
    img.pixels()
        .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
        .max_by(|a, b| luminance(*a).total_cmp(&luminance(*b)))
}

fn average(img: &image::RgbImage) -> Option<Color32> {
    let n = img.pixels().len() as u64;
    if n == 0 {
        return None;
    }
    let (r, g, b) = img.pixels().fold((0u64, 0u64, 0u64), |(r, g, b), p| {
        (r + p[0] as u64, g + p[1] as u64, b + p[2] as u64)
    });
    Some(Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8))
}

/// WCAG's relative luminance, from linear light.
fn relative_luminance(c: Color32) -> f32 {
    let linear = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// WCAG's contrast ratio, from 1 (none) to 21 (black on white).
fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (relative_luminance(a), relative_luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Contrast WCAG asks of body text.
const TEXT_CONTRAST: f32 = 4.5;
/// Contrast for weak text and large text, WCAG's minimum for either.
const WEAK_CONTRAST: f32 = 3.0;
/// How far a list's background may move toward black or white before its
/// text has to change instead.
const MAX_SURFACE_SHIFT: f32 = 0.5;

/// The least contrast `fg` has against any of `bgs`.
fn worst(fg: Color32, bgs: &[Color32]) -> f32 {
    bgs.iter().map(|&bg| contrast(fg, bg)).fold(f32::INFINITY, f32::min)
}

/// `colour`, mixed toward white or black, whichever gets there in the
/// smaller step, until `score` reaches `min`; if neither does, the mix that
/// scored best.
fn nearest(colour: Color32, score: impl Fn(Color32) -> f32, min: f32) -> Color32 {
    let mut best = colour;
    if score(colour) >= min {
        return colour;
    }
    for step in 1..=20 {
        let t = step as f32 / 20.0;
        for end in [Color32::WHITE, Color32::BLACK] {
            let c = mix(colour, end, t);
            if score(c) >= min {
                return c;
            }
            if score(c) > score(best) {
                best = c;
            }
        }
    }
    best
}

/// `fg`, changed as little as it takes to read on every one of `bgs`.
fn readable(fg: Color32, bgs: &[Color32], min: f32) -> Color32 {
    nearest(fg, |c| worst(c, bgs), min)
}

/// `bg`, changed as little as it takes for `fg` to read on it.
fn behind(bg: Color32, fg: Color32, min: f32) -> Color32 {
    nearest(bg, |c| contrast(fg, c), min)
}

/// The playlist's own pair, text on its background, made readable: the
/// background moves away from the text first, up to `MAX_SURFACE_SHIFT`,
/// since the text colour is what a skin is recognised by; then the text.
fn separate(fg: Color32, bg: Color32, min: f32) -> (Color32, Color32) {
    if contrast(fg, bg) >= min {
        return (fg, bg);
    }
    let away = if relative_luminance(fg) >= relative_luminance(bg) {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    let mut shifted = bg;
    for step in 1..=10 {
        shifted = mix(bg, away, MAX_SURFACE_SHIFT * step as f32 / 10.0);
        if contrast(fg, shifted) >= min {
            return (fg, shifted);
        }
    }
    (readable(fg, &[shifted], min), shifted)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// The first line of every converted file, so an outdated conversion can
/// be recognised and redone (see `convert_new_in`).
const HEADER: &str = "# Converted from the Winamp skin";
/// In every conversion whose colours were checked for contrast (ADR-0033).
const CONTRAST_CHECKED: &str = "# Text colours are checked for contrast";

/// Writes a theme TOML (format `skin::FORMAT`) from extracted colours.
/// `name` becomes the skin's menu name; `source` is noted in the header.
pub fn to_toml(name: &str, source: &str, c: &WszColors) -> String {
    // The playlist's text on its background, readable first: the rest are
    // checked against these.
    let (text, surface) = match (c.normal, c.normal_bg) {
        (Some(fg), Some(bg)) => {
            let (fg, bg) = separate(fg, bg, TEXT_CONTRAST);
            (Some(fg), Some(bg))
        }
        pair => pair,
    };
    // Window background: the main window's tone, else the playlist's, made
    // dark or light enough for the text.
    let background = match (c.main_tone.or(surface), text) {
        (Some(bg), Some(fg)) => Some(behind(bg, fg, TEXT_CONTRAST)),
        (bg, _) => bg,
    };
    let dark = background.is_none_or(|bg| luminance(bg) < 0.5);
    let toward = if dark { Color32::WHITE } else { Color32::BLACK };
    let hover = background.map(|bg| mix(bg, toward, 0.12));
    let surface_alt = surface.zip(text).map(|(bg, fg)| behind(mix(bg, fg, 0.06), fg, TEXT_CONTRAST));
    let panels: Vec<Color32> = [background, surface, surface_alt].into_iter().flatten().collect();
    // Strong text is on hovered and pressed controls too.
    let mut under_strong = panels.clone();
    under_strong.extend(hover);
    let text_strong = c.current.map(|fg| readable(fg, &under_strong, TEXT_CONTRAST));
    let text_weak = text
        .zip(surface)
        .map(|(fg, bg)| readable(mix(fg, bg, 0.4), &panels, WEAK_CONTRAST));
    let accent = c.selected_bg;
    // The skin's own text on the selection if it reads there, else the
    // current track's colour, else the text, changed until it reads.
    let accent_text = accent.and_then(|bg| {
        [text, text_strong]
            .into_iter()
            .flatten()
            .find(|&fg| contrast(fg, bg) >= TEXT_CONTRAST)
            .or_else(|| text.map(|fg| readable(fg, &[bg], TEXT_CONTRAST)))
    });
    let display = c.vis.as_deref().map(|v| v[0]);
    let time = c.digits.map(|fg| match display.or(background) {
        Some(bg) => readable(fg, &[bg], TEXT_CONTRAST),
        None => fg,
    });

    let mut out = String::new();
    out.push_str(&format!(
        "{HEADER} {source:?} by UnAmp.\n\
         # Only colours carry over; edit freely — this file isn't regenerated\n\
         # while it exists. Delete it and use Skins → Reload to convert again.\n\
         {CONTRAST_CHECKED} against what they're drawn on, and\n\
         # moved toward black or white where the skin's own pairs don't read.\n\n"
    ));
    out.push_str(&format!("format = {}\n", crate::skin::FORMAT));
    out.push_str(&format!("name = {}\n", toml_string(name)));
    out.push_str(&format!("base = \"{}\"\n", if dark { "dark" } else { "light" }));
    out.push_str("corner_radius = 0\nshadows = false\n\n[colors]\n");

    let mut line = |key: &str, value: Option<Color32>, note: &str| {
        if let Some(v) = value {
            out.push_str(&format!("{key} = \"{}\"   # {note}\n", hex(v)));
        }
    };
    line("background", background, "main.bmp average tone");
    line("surface", surface, "pledit NormalBG");
    line("surface_alt", surface_alt, "NormalBG tinted with Normal");
    line("border", background.map(|bg| mix(bg, toward, 0.3)), "background, lifted");
    line("text", text, "pledit Normal");
    line("text_strong", text_strong, "pledit Current");
    line("text_weak", text_weak, "Normal faded toward NormalBG");
    line("accent", accent, "pledit SelectedBG");
    line("accent_text", accent_text, "pledit Normal, or Current, on SelectedBG");
    line("link", text_strong, "pledit Current (now-playing track)");

    out.push_str("\n[controls]\n");
    let mut line = |key: &str, value: Option<Color32>, note: &str| {
        if let Some(v) = value {
            out.push_str(&format!("{key} = \"{}\"   # {note}\n", hex(v)));
        }
    };
    line("background", background, "same as the window background");
    line("hover", hover, "background, lifted");
    line("pressed", surface, "pledit NormalBG");

    out.push_str(&format!(
        "\n[app.unamp]\n\
         # Draw the Player, Equalizer and Playlist from the skin's own bitmaps.\n\
         # Remove this line to use UnAmp's regular windows in these colours.\n\
         classic = {}\n",
        toml_string(source)
    ));
    let vis = c.vis.as_deref();
    let mut line = |key: &str, value: Option<Color32>, note: &str| {
        if let Some(v) = value {
            out.push_str(&format!("{key} = \"{}\"   # {note}\n", hex(v)));
        }
    };
    line("display", display, "viscolor 0, analyzer background");
    line("time", time, "numbers.bmp digit colour");
    line("title", text, "pledit Normal");
    line("spectrum_peak", vis.map(|v| v[23]), "viscolor 23, peak dots");
    if let Some(v) = vis {
        // Every analyzer colour, bottom (viscolor 17) to top (viscolor 2).
        let all: Vec<String> = (2..=17).rev().map(|i| format!("\"{}\"", hex(v[i]))).collect();
        out.push_str(&format!("spectrum = [{}]   # viscolor 17 → 2\n", all.join(", ")));
    }
    out
}

/// Where to keep a conversion this converter wrote before, to redo it: one
/// from before the theme format had a version goes to `.toml.v0`, and one
/// from before contrast was checked to `.toml.v1`. `None` for anything
/// else, a current conversion or a skin someone wrote.
fn outdated_conversion(text: &str) -> Option<&'static str> {
    if !text.starts_with(HEADER) {
        return None;
    }
    if toml::from_str::<toml::Table>(text).is_ok_and(|t| !t.contains_key("format")) {
        Some("toml.v0")
    } else if !text.contains(CONTRAST_CHECKED) {
        Some("toml.v1")
    } else {
        None
    }
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// For each `.wsz` in `dir` without a same-named `.toml`, writes one.
/// Existing TOML files are left alone so hand edits survive, except an
/// outdated conversion (see `outdated_conversion`), which is redone with
/// the old file kept beside it.
pub fn convert_new_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        let is_wsz = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("wsz"));
        if !is_wsz {
            continue;
        }
        let target: PathBuf = path.with_extension("toml");
        if target.exists() {
            let text = std::fs::read_to_string(&target).unwrap_or_default();
            let Some(backup) = outdated_conversion(&text) else {
                continue;
            };
            if let Err(e) = std::fs::rename(&target, target.with_extension(backup)) {
                errors.push(format!("{}: {e}", target.display()));
                continue;
            }
        }
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let result = std::fs::File::open(&path)
            .map_err(|e| e.to_string())
            .and_then(read_wsz)
            .and_then(|colors| {
                std::fs::write(&target, to_toml(&name, &file_name, &colors)).map_err(|e| e.to_string())
            });
        if let Err(e) = result {
            errors.push(format!("{file_name}: {e}"));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const VISCOLOR: &str = "0,0,0, // color 0 = black\n24,33,41, // dots\n239,49,16, // top\n\
        206,41,16,\n214,90,0,\n214,102,0,\n214,115,0,\n198,123,8,\n222,165,24,\n214,181,33,\n\
        189,222,41,\n148,222,33,\n41,206,16,\n50,190,16,\n57,181,16,\n49,156,8,  // 15\n\
        41,148,0,\n24,132,8,   // 17 = bottom\n255,255,255,\n214,214,222,\n181,189,189,\n\
        160,170,175,\n148,156,165,\n150, 150, 150, // 23 = peak dots\n";

    fn bmp(color: [u8; 3], lit: Option<[u8; 3]>) -> Vec<u8> {
        let mut img = image::RgbImage::from_pixel(8, 8, image::Rgb(color));
        if let Some(lit) = lit {
            img.put_pixel(3, 3, image::Rgb(lit));
        }
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Bmp).unwrap();
        out.into_inner()
    }

    fn wsz(files: &[(&str, Vec<u8>)]) -> Cursor<Vec<u8>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored))
                .unwrap();
            zip.write_all(data).unwrap();
        }
        Cursor::new(zip.finish().unwrap().into_inner())
    }

    #[test]
    fn reads_colours_case_insensitively_and_through_folders() {
        let archive = wsz(&[
            ("Squall/PLEDIT.TXT", b"[Text]\r\nNormal=#00FF00\r\ncurrent=FFFFFF\r\nNormalBG=#000000\r\nSelectedBG=#0000C6\r\nFont=Arial\r\n".to_vec()),
            ("Squall/VISCOLOR.TXT", VISCOLOR.as_bytes().to_vec()),
            ("Squall/NUMBERS.BMP", bmp([0, 0, 0], Some([0, 230, 0]))),
            ("Squall/MAIN.BMP", bmp([40, 40, 60], None)),
        ]);
        let c = read_wsz(archive).unwrap();
        assert_eq!(c.normal, Some(Color32::from_rgb(0, 255, 0)));
        assert_eq!(c.current, Some(Color32::WHITE));
        assert_eq!(c.selected_bg, Some(Color32::from_rgb(0, 0, 0xC6)));
        let vis = c.vis.unwrap();
        assert_eq!(vis[2], Color32::from_rgb(239, 49, 16));
        assert_eq!(vis[23], Color32::from_rgb(150, 150, 150));
        assert_eq!(c.digits, Some(Color32::from_rgb(0, 230, 0)));
        assert_eq!(c.main_tone, Some(Color32::from_rgb(40, 40, 60)));
    }

    #[test]
    fn missing_files_leave_fields_empty() {
        let c = read_wsz(wsz(&[("readme.txt", b"hi".to_vec())])).unwrap();
        assert_eq!(c, WszColors::default());
    }

    #[test]
    fn incomplete_viscolor_is_ignored() {
        assert!(parse_viscolor("0,0,0,\n1,2,3,\n").is_none());
    }

    #[test]
    fn not_a_zip_is_an_error() {
        assert!(read_wsz(Cursor::new(b"definitely not a zip".to_vec())).is_err());
    }

    #[test]
    fn converted_toml_is_a_valid_skin() {
        let c = WszColors {
            normal: Some(Color32::from_rgb(0, 255, 0)),
            current: Some(Color32::WHITE),
            normal_bg: Some(Color32::BLACK),
            selected_bg: Some(Color32::from_rgb(0, 0, 0xC6)),
            vis: parse_viscolor(VISCOLOR),
            digits: Some(Color32::from_rgb(0, 230, 0)),
            main_tone: Some(Color32::from_rgb(40, 40, 60)),
        };
        let text = to_toml("Squall \"Leonhart\"", "Squall.wsz", &c);
        let skin = crate::skin::Skin::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(skin.name, "Squall \"Leonhart\"");
        assert_eq!(skin.base, Some(patina::skin::Base::Dark));
        assert!(skin.warnings.is_empty(), "{:?}", skin.warnings);
        let (section, warnings) = crate::skin::unamp_section(&skin);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(section.classic.as_deref(), Some("Squall.wsz"));
        let spectrum = section.spectrum.unwrap();
        assert_eq!(spectrum.len(), 16);
        assert_eq!(spectrum[0].0, Color32::from_rgb(24, 132, 8));
        assert_eq!(spectrum[15].0, Color32::from_rgb(239, 49, 16));
    }

    /// The colours a converted skin's text is drawn on, and the text.
    fn pairs(text: &str) -> Vec<(&'static str, Color32, Color32)> {
        let t: toml::Table = toml::from_str(text).unwrap();
        let get = |table: &str, key: &str| {
            t[table].get(key).and_then(|v| parse_colour(v.as_str()?))
        };
        let colors = |key| get("colors", key);
        let mut out = Vec::new();
        for bg in ["background", "surface", "surface_alt"] {
            for fg in ["text", "text_strong", "link"] {
                out.push((fg, colors(fg).unwrap(), colors(bg).unwrap()));
            }
            out.push(("text_weak", colors("text_weak").unwrap(), colors(bg).unwrap()));
        }
        out.push(("text_strong", colors("text_strong").unwrap(), get("controls", "hover").unwrap()));
        out.push(("accent_text", colors("accent_text").unwrap(), colors("accent").unwrap()));
        let unamp = |key: &str| parse_colour(t["app"]["unamp"][key].as_str().unwrap()).unwrap();
        out.push(("time", unamp("time"), unamp("display")));
        out
    }

    fn assert_readable(text: &str) {
        for (key, fg, bg) in pairs(text) {
            let min = if key == "text_weak" { WEAK_CONTRAST } else { TEXT_CONTRAST };
            assert!(contrast(fg, bg) >= min, "{key} {} on {}: {:.2}\n{text}", hex(fg), hex(bg), contrast(fg, bg));
        }
    }

    #[test]
    fn text_reads_on_a_mid_tone_window_and_a_bright_selection() {
        // Shakira_04: orange on a brown window, and on a lighter orange selection.
        let c = WszColors {
            normal: Some(Color32::from_rgb(0xEB, 0x8B, 0x1C)),
            current: Some(Color32::WHITE),
            normal_bg: Some(Color32::BLACK),
            selected_bg: Some(Color32::from_rgb(0xFB, 0xAD, 0x29)),
            vis: parse_viscolor(VISCOLOR),
            digits: Some(Color32::from_rgb(0xF8, 0xA4, 0x20)),
            main_tone: Some(Color32::from_rgb(0x88, 0x5B, 0x45)),
        };
        let text = to_toml("Shakira", "Shakira.wsz", &c);
        assert_readable(&text);
        // The skin's orange is kept; the window darkens instead.
        assert!(text.contains("text = \"#EB8B1C\""), "{text}");
    }

    #[test]
    fn green_on_blue_keeps_its_green() {
        // Windows_XP: green playlist text on a mid blue, on a lighter blue window.
        let c = WszColors {
            normal: Some(Color32::from_rgb(0, 255, 0)),
            current: Some(Color32::WHITE),
            normal_bg: Some(Color32::from_rgb(0x3F, 0x83, 0xE9)),
            selected_bg: Some(Color32::from_rgb(0, 0, 0xC0)),
            vis: parse_viscolor(VISCOLOR),
            digits: Some(Color32::WHITE),
            main_tone: Some(Color32::from_rgb(0x6D, 0x8E, 0xDD)),
        };
        let text = to_toml("XP", "XP.wsz", &c);
        assert_readable(&text);
        assert!(text.contains("text = \"#00FF00\""), "{text}");
    }

    #[test]
    fn readable_pairs_are_left_alone() {
        let c = WszColors {
            normal: Some(Color32::from_rgb(0, 255, 0)),
            current: Some(Color32::WHITE),
            normal_bg: Some(Color32::BLACK),
            selected_bg: Some(Color32::from_rgb(0, 0, 0xC6)),
            vis: parse_viscolor(VISCOLOR),
            digits: Some(Color32::from_rgb(0, 248, 0)),
            main_tone: Some(Color32::from_rgb(0x28, 0x28, 0x3C)),
        };
        let text = to_toml("Classic", "Classic.wsz", &c);
        assert_readable(&text);
        for line in ["background = \"#28283C\"", "surface = \"#000000\"", "text = \"#00FF00\"", "accent_text = \"#00FF00\""] {
            assert!(text.contains(line), "{line}\n{text}");
        }
    }

    #[test]
    fn empty_skin_still_converts() {
        let text = to_toml("Bare", "Bare.wsz", &WszColors::default());
        let skin = crate::skin::Skin::parse(&text).unwrap();
        assert!(skin.warnings.is_empty(), "{:?}", skin.warnings);
    }

    #[test]
    fn convert_new_in_writes_once_and_keeps_edits() {
        let dir = std::env::temp_dir().join(format!("unamp-wsz-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = wsz(&[("pledit.txt", b"Normal=#00FF00".to_vec())]).into_inner();
        std::fs::write(dir.join("Squall.wsz"), archive).unwrap();

        assert!(convert_new_in(&dir).is_empty());
        let toml_path = dir.join("Squall.toml");
        assert!(std::fs::read_to_string(&toml_path).unwrap().contains("name = \"Squall\""));

        std::fs::write(&toml_path, "name = \"Edited\"\n").unwrap();
        convert_new_in(&dir);
        assert_eq!(std::fs::read_to_string(&toml_path).unwrap(), "name = \"Edited\"\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_format_conversions_are_redone_with_a_backup() {
        let dir = std::env::temp_dir().join(format!("unamp-wsz-redo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = wsz(&[("pledit.txt", b"Normal=#00FF00".to_vec())]).into_inner();
        std::fs::write(dir.join("Squall.wsz"), archive).unwrap();
        let old = format!("{HEADER} \"Squall.wsz\" by UnAmp.\nname = \"Squall\"\n[player]\ntime = \"#00FF00\"\n");
        std::fs::write(dir.join("Squall.toml"), &old).unwrap();

        assert!(convert_new_in(&dir).is_empty());
        let redone = std::fs::read_to_string(dir.join("Squall.toml")).unwrap();
        assert!(redone.contains("format = 1"), "{redone}");
        assert_eq!(std::fs::read_to_string(dir.join("Squall.toml.v0")).unwrap(), old);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversions_from_before_the_contrast_check_are_redone_with_a_backup() {
        let dir = std::env::temp_dir().join(format!("unamp-wsz-contrast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = wsz(&[("pledit.txt", b"Normal=#00FF00".to_vec())]).into_inner();
        std::fs::write(dir.join("Squall.wsz"), archive).unwrap();
        let old = format!("{HEADER} \"Squall.wsz\" by UnAmp.\nformat = 1\nname = \"Squall\"\n");
        std::fs::write(dir.join("Squall.toml"), &old).unwrap();

        assert!(convert_new_in(&dir).is_empty());
        let redone = std::fs::read_to_string(dir.join("Squall.toml")).unwrap();
        assert!(redone.contains(CONTRAST_CHECKED), "{redone}");
        assert_eq!(std::fs::read_to_string(dir.join("Squall.toml.v1")).unwrap(), old);
        // Redone once: the new conversion stays.
        assert!(convert_new_in(&dir).is_empty());
        assert_eq!(std::fs::read_to_string(dir.join("Squall.toml")).unwrap(), redone);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
