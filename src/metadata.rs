//! Track tags, durations and album art, read with `lofty`.
//!
//! Everything here touches the file system and may be slow on a network
//! mount, so callers run it on worker threads.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::Accessor;

/// Extensions UnAmp lists and tries to decode (symphonia's formats).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "wav", "m4a", "mp4", "aac", "aif", "aiff", "caf", "mka",
];

/// Folder images used as album art when a track has no embedded cover,
/// matched case-insensitively by file stem, in order of preference.
const COVER_STEMS: &[&str] = &["cover", "folder", "front", "album", "albumart", "albumartsmall"];
const COVER_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Longest edge album art is downscaled to before upload as a texture.
pub const ART_MAX_EDGE: u32 = 512;

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

impl TrackInfo {
    /// "Artist - Title" when tagged, otherwise the file stem.
    pub fn display_title(&self, path: &Path) -> String {
        match (&self.artist, &self.title) {
            (Some(artist), Some(title)) => format!("{artist} - {title}"),
            (None, Some(title)) => title.clone(),
            _ => file_stem(path),
        }
    }
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Reads tags and stream properties. Untagged or unreadable files yield
/// an empty `TrackInfo` rather than an error, so they still list by name.
pub fn read_track_info(path: &Path) -> TrackInfo {
    let Ok(tagged) = lofty::read_from_path(path) else {
        return TrackInfo::default();
    };
    let props = tagged.properties();
    let duration = Some(props.duration()).filter(|d| !d.is_zero());
    let mut info = TrackInfo {
        duration,
        bitrate_kbps: props.audio_bitrate().or(props.overall_bitrate()),
        sample_rate: props.sample_rate(),
        channels: props.channels(),
        ..Default::default()
    };
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        info.title = tag.title().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        info.artist = tag.artist().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        info.album = tag.album().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        info.track = tag.track();
    }
    info
}

/// The encoded album-art image for a track: the embedded front cover (or
/// any embedded picture), else a cover image in the track's folder.
pub fn album_art_bytes(path: &Path) -> Option<Vec<u8>> {
    embedded_art(path).or_else(|| {
        let file = folder_art(path.parent()?)?;
        std::fs::read(file).ok()
    })
}

/// Decodes album art and downscales it to `ART_MAX_EDGE`.
pub fn decode_art(bytes: &[u8]) -> Option<image::RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > ART_MAX_EDGE || img.height() > ART_MAX_EDGE {
        img.thumbnail(ART_MAX_EDGE, ART_MAX_EDGE)
    } else {
        img
    };
    Some(img.into_rgba8())
}

fn embedded_art(path: &Path) -> Option<Vec<u8>> {
    let tagged = lofty::read_from_path(path).ok()?;
    let pictures: Vec<_> = tagged.tags().iter().flat_map(|t| t.pictures()).collect();
    pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first())
        .map(|p| p.data().to_vec())
}

/// Finds the best cover image among a folder's files.
pub fn folder_art(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let names: Vec<(PathBuf, String)> = entries
        .flatten()
        .map(|e| (e.path(), e.file_name().to_string_lossy().to_ascii_lowercase()))
        .collect();
    pick_cover(&names)
}

fn pick_cover(names: &[(PathBuf, String)]) -> Option<PathBuf> {
    let is_image = |name: &str| {
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| COVER_EXTENSIONS.contains(&e))
    };
    let stem = |name: &str| {
        Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    for wanted in COVER_STEMS {
        if let Some((path, _)) = names
            .iter()
            .find(|(_, name)| is_image(name) && stem(name) == *wanted)
        {
            return Some(path.clone());
        }
    }
    // Windows Media Player's "AlbumArt_{GUID}_Large.jpg" and similar.
    names
        .iter()
        .filter(|(_, name)| is_image(name) && name.starts_with("albumart"))
        .max_by_key(|(_, name)| name.contains("large"))
        .map(|(path, _)| path.clone())
}

/// Most art files kept in the cache; the oldest go first beyond this.
const ART_CACHE_MAX: usize = 256;

/// Writes album art to `~/.cache/unamp/art/`, named by a hash of its
/// content so the same cover is stored once, and returns the file. Media
/// controls (MPRIS) need art as a local file they can open: embedded art
/// has no file of its own, and a cover on a network share may not be
/// readable by the desktop shell.
pub fn cache_art(bytes: &[u8]) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let dir = dirs::cache_dir()?.join("unamp").join("art");
    std::fs::create_dir_all(&dir).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let ext = image::guess_format(bytes)
        .ok()
        .and_then(|f| f.extensions_str().first().copied())
        .unwrap_or("img");
    let path = dir.join(format!("{:016x}.{ext}", hasher.finish()));
    if !path.exists() {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
        prune_cache(&dir, ART_CACHE_MAX);
    }
    Some(path)
}

fn prune_cache(dir: &Path, max: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= max {
        return;
    }
    files.sort();
    for (_, path) in files.iter().take(files.len() - max) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<(PathBuf, String)> {
        list.iter()
            .map(|n| (PathBuf::from(n), n.to_ascii_lowercase()))
            .collect()
    }

    #[test]
    fn is_audio_matches_case_insensitively() {
        assert!(is_audio(Path::new("/a/b/Song.FLAC")));
        assert!(is_audio(Path::new("x.mp3")));
        assert!(!is_audio(Path::new("cover.jpg")));
        assert!(!is_audio(Path::new("noext")));
    }

    #[test]
    fn pick_cover_prefers_cover_over_folder() {
        let picked = pick_cover(&names(&["Folder.jpg", "Cover.PNG", "01.flac"]));
        assert_eq!(picked, Some(PathBuf::from("Cover.PNG")));
    }

    #[test]
    fn pick_cover_falls_back_to_wmp_large_art() {
        let picked = pick_cover(&names(&[
            "AlbumArt_{X}_Small.jpg",
            "AlbumArt_{X}_Large.jpg",
            "01.mp3",
        ]));
        assert_eq!(picked, Some(PathBuf::from("AlbumArt_{X}_Large.jpg")));
    }

    #[test]
    fn pick_cover_ignores_non_images() {
        assert_eq!(pick_cover(&names(&["cover.txt", "01.mp3"])), None);
    }

    #[test]
    fn prune_cache_keeps_the_newest() {
        let dir = std::env::temp_dir().join(format!("unamp-artcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            let p = dir.join(format!("{i}.jpg"));
            std::fs::write(&p, b"x").unwrap();
            let t = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000 + i);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        }
        prune_cache(&dir, 2);
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["3.jpg", "4.jpg"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_duration_handles_minutes_and_hours() {
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn display_title_falls_back_to_file_stem() {
        let path = Path::new("/m/01 Intro.flac");
        assert_eq!(TrackInfo::default().display_title(path), "01 Intro");
        let info = TrackInfo {
            artist: Some("Daft Punk".into()),
            title: Some("Da Funk".into()),
            ..Default::default()
        };
        assert_eq!(info.display_title(path), "Daft Punk - Da Funk");
    }
}
