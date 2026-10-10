//! Builds a throwaway UnAmp setup for `make screenshot`: a home folder
//! with a library of fake, synthesised songs (tagged WAVs with cover art),
//! plus the config and saved playlist UnAmp starts from.
//!
//!     cargo run --release --example fake_library -- <dir>
//!
//! Lays out `<dir>/home/Music/<artist>/<album>/`, `<dir>/config` and
//! `<dir>/data`, for running UnAmp with `HOME`, `XDG_CONFIG_HOME` and
//! `XDG_DATA_HOME` pointing there.

use std::f32::consts::TAU;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, Tag, TagType};

const RATE: u32 = 22_050;

struct Album {
    artist: &'static str,
    title: &'static str,
    /// Two cover colours.
    colors: [[u8; 3]; 2],
    /// Root note in Hz, and beats per minute.
    root: f32,
    bpm: f32,
    /// Track titles and lengths in seconds.
    tracks: &'static [(&'static str, u32)],
}

const ALBUMS: &[Album] = &[
    Album {
        artist: "The Pipes",
        title: "Standard Input",
        colors: [[0x1d, 0x4e, 0x89], [0xf2, 0xa6, 0x3b]],
        root: 110.0,
        bpm: 118.0,
        tracks: &[
            ("Everything Is a File", 214),
            ("Do One Thing Well", 187),
            ("Pipe Dreams", 241),
            ("Worse Is Better", 168),
        ],
    },
    Album {
        artist: "Daemon Sync",
        title: "Background Processes",
        colors: [[0x2b, 0x1a, 0x3d], [0x3b, 0xd1, 0xa0]],
        root: 98.0,
        bpm: 126.0,
        tracks: &[("fork()", 196), ("Zombie Children", 233), ("SIGHUP", 152)],
    },
    Album {
        artist: "Kernel Panic",
        title: "Core Dumped",
        colors: [[0x8a, 0x16, 0x1c], [0xf4, 0xe9, 0xd8]],
        root: 82.4,
        bpm: 140.0,
        tracks: &[
            ("Segmentation Fault", 205),
            ("Out of Memory", 178),
            ("Oops", 131),
            ("Init 0", 252),
        ],
    },
    Album {
        artist: "Null Device",
        title: "Write Only",
        colors: [[0x10, 0x10, 0x10], [0x9a, 0x9a, 0x9a]],
        root: 73.4,
        bpm: 92.0,
        tracks: &[("Bit Bucket", 224), ("Infinite Zeros", 199)],
    },
];

/// The album that's playing and the one the library is open on.
const PLAYING: usize = 0;
const BROWSING: usize = 2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("usage: fake_library <dir>")?);
    let home = root.join("home");
    let music = home.join("Music");
    let mut album_dirs = Vec::new();

    for (a, album) in ALBUMS.iter().enumerate() {
        let dir = music.join(album.artist).join(album.title);
        fs::create_dir_all(&dir)?;
        cover(album).save(dir.join("cover.png"))?;
        let mut tracks = Vec::new();
        for (t, &(title, secs)) in album.tracks.iter().enumerate() {
            let path = dir.join(format!("{:02} {title}.wav", t + 1));
            write_wav(&path, &song(album, a * 7 + t, secs))?;
            tag(&path, album, title, t as u32 + 1)?;
            // As UnAmp saves it: the playlist shows this until tags are read.
            tracks.push(format!("#EXTINF:{secs},{} - {title}\n{}\n", album.artist, path.display()));
        }
        album_dirs.push((dir, tracks));
    }

    // Where `dirs::audio_dir()` finds the Music folder.
    let config = root.join("config");
    fs::create_dir_all(config.join("unamp"))?;
    fs::write(config.join("user-dirs.dirs"), "XDG_MUSIC_DIR=\"$HOME/Music\"\n")?;
    let browse = &album_dirs[BROWSING].0;
    fs::write(
        config.join("unamp").join("config.toml"),
        format!(
            "browse_path = {:?}\nwindow_width = 1285.0\nwindow_height = 860.0\nskin = \"Default\"\n\
             show_waveform = true\neq_enabled = true\neq_bands = [4.0, 3.0, 1.5, 0.0, -1.0, -1.0, 0.0, 1.5, 3.0, 4.0]\n",
            browse.display().to_string()
        ),
    )?;

    // The playlist: the playing album, then one more.
    let data = root.join("data").join("unamp");
    fs::create_dir_all(&data)?;
    let mut m3u = String::from("#EXTM3U\n");
    for (i, entry) in album_dirs[PLAYING].1.iter().chain(&album_dirs[1].1).enumerate() {
        if i == 1 {
            m3u.push_str("#UNAMP-CURRENT\n");
        }
        m3u.push_str(entry);
    }
    fs::write(data.join("playlist.m3u8"), m3u)?;
    Ok(())
}

/// A loop of four chords over a bass line, kick and hi-hats: enough going
/// on across the spectrum for the visualiser and waveform to look alive.
fn song(album: &Album, seed: usize, secs: u32) -> Vec<i16> {
    // Semitone offsets of each chord's root, varied per track.
    const PROGRESSIONS: [[i32; 4]; 4] = [[0, 5, 7, 3], [0, 8, 3, 10], [0, 7, 9, 5], [0, 3, 8, 7]];
    let progression = PROGRESSIONS[seed % 4];
    let beat = 60.0 / (album.bpm + (seed % 3) as f32 * 4.0);
    let n = (secs * RATE) as usize;
    let mut out = Vec::with_capacity(n);
    let mut noise = 0x1234_5678u32 ^ seed as u32;
    let mut prev_noise = 0.0;

    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let bar = (t / (beat * 4.0)) as usize;
        let chord_root = album.root * 2f32.powf(progression[bar % 4] as f32 / 12.0);
        let in_beat = t % beat;
        let in_half = t % (beat / 2.0);
        // Quiet intro and outro.
        let fade = (t / 3.0).min(1.0) * ((secs as f32 - t) / 4.0).clamp(0.0, 1.0);

        let bass = (TAU * chord_root * t).sin() * 0.30;
        let pad: f32 = [1.0, 1.26, 1.5, 2.0]
            .iter()
            .map(|r| {
                let f = chord_root * 2.0 * r;
                (TAU * f * t).sin() + 0.4 * (TAU * 2.0 * f * t).sin() + 0.2 * (TAU * 3.0 * f * t).sin()
            })
            .sum::<f32>()
            * 0.05
            * (0.7 + 0.3 * (TAU * t / (beat * 8.0)).sin());
        let kick_freq = 45.0 + 80.0 * (-in_beat * 30.0).exp();
        let kick = (TAU * kick_freq * in_beat).sin() * (-in_beat * 9.0).exp() * 0.55;
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        // A difference of white noise leans to the highs, like a hi-hat.
        let hat = (white - prev_noise) * (-in_half * 40.0).exp() * if in_beat > beat / 2.0 { 0.18 } else { 0.07 };
        prev_noise = white;
        let lead = if bar % 8 >= 4 {
            let note = chord_root * 4.0 * [1.0, 1.5, 1.26, 2.0][(t / (beat / 2.0)) as usize % 4];
            (TAU * note * t).sin() * (-in_half * 6.0).exp() * 0.12
        } else {
            0.0
        };

        let sample = ((bass + pad + kick + hat + lead) * fade).clamp(-1.0, 1.0);
        out.push((sample * 0.8 * i16::MAX as f32) as i16);
    }
    out
}

fn write_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut f = std::io::BufWriter::new(fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&RATE.to_le_bytes())?;
    f.write_all(&(RATE * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()
}

fn tag(path: &Path, album: &Album, title: &str, track: u32) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = lofty::read_from_path(path)?;
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_artist(album.artist.to_string());
    tag.set_album(album.title.to_string());
    tag.set_title(title.to_string());
    tag.set_track(track);
    file.insert_tag(tag);
    file.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

/// A simple abstract cover: a diagonal gradient between the album's two
/// colours, with rings around an off-centre point.
fn cover(album: &Album) -> image::RgbImage {
    const SIZE: u32 = 300;
    let [a, b] = album.colors;
    let mix = |t: f32| -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        [0, 1, 2].map(|c| (a[c] as f32 + (b[c] as f32 - a[c] as f32) * t) as u8)
    };
    image::RgbImage::from_fn(SIZE, SIZE, |x, y| {
        let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);
        let d = ((u - 0.68).powi(2) + (v - 0.35).powi(2)).sqrt();
        let ring = ((d * 28.0).sin() * 0.5 + 0.5) * (1.0 - d * 1.6).max(0.0);
        image::Rgb(mix((u + v) * 0.35 + ring * 0.6))
    })
}
