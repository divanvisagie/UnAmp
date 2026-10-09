//! Library search without an index (see ADR-0015): walks the folders under
//! the current location on a worker thread, matching every typed word
//! against each file's path relative to the location, and streams results
//! back as they're found. A new query cancels the old walk.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

use crate::library::Track;
use crate::metadata;

/// Stop after this many tracks; a query this broad should be narrowed.
pub const MAX_TRACKS: usize = 2000;
pub const MAX_FOLDERS: usize = 200;
/// Seconds to wait after the last keystroke before searching.
const DEBOUNCE: f64 = 0.3;
/// Virtual file systems never worth walking when searching from `/`.
const SKIP_UNDER_ROOT: &[&str] = &["/proc", "/sys", "/dev", "/run"];

enum Msg {
    Batch {
        generation: u64,
        tracks: Vec<PathBuf>,
        folders: Vec<PathBuf>,
        scanned: usize,
    },
    Done {
        generation: u64,
        truncated: bool,
    },
}

pub struct Search {
    /// The text in the search box.
    pub query: String,
    /// The query and root the current results belong to.
    searched: Option<(String, PathBuf)>,
    generation: Arc<AtomicU64>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    pub tracks: Vec<Track>,
    pub folders: Vec<PathBuf>,
    /// Folders read so far.
    pub scanned: usize,
    pub running: bool,
    /// Stopped early at `MAX_TRACKS` / `MAX_FOLDERS`.
    pub truncated: bool,
    edited_at: Option<f64>,
}

impl Default for Search {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            query: String::new(),
            searched: None,
            generation: Arc::new(AtomicU64::new(0)),
            tx,
            rx,
            tracks: Vec::new(),
            folders: Vec::new(),
            scanned: 0,
            running: false,
            truncated: false,
            edited_at: None,
        }
    }
}

impl Search {
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// The search box changed at time `now`; search once typing pauses.
    pub fn edited(&mut self, now: f64) {
        self.edited_at = Some(now);
    }

    /// Search right away (Enter).
    pub fn submit(&mut self) {
        self.edited_at = Some(f64::NEG_INFINITY);
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.cancel();
        self.searched = None;
        self.tracks.clear();
        self.folders.clear();
        self.edited_at = None;
    }

    fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.running = false;
    }

    /// Starts or restarts the walk when needed, and takes in results.
    /// `root` is the location being searched; changing it re-runs the query.
    pub fn poll(&mut self, root: &Path, now: f64, ctx: &egui::Context) {
        let query = self.query.trim().to_string();
        let due = self.edited_at.is_some_and(|t| now - t >= DEBOUNCE);
        let root_changed = self.searched.as_ref().is_some_and(|(_, r)| r != root);
        if (due || root_changed) && self.searched.as_ref() != Some(&(query.clone(), root.to_path_buf())) {
            if query.is_empty() {
                self.clear();
            } else {
                self.start(query, root.to_path_buf(), ctx);
            }
        }
        if due {
            self.edited_at = None;
        } else if self.edited_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(DEBOUNCE));
        }

        let current = self.generation.load(Ordering::SeqCst);
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Batch { generation, tracks, folders, scanned } if generation == current => {
                    self.tracks.extend(tracks.into_iter().map(Track::new));
                    self.folders.extend(folders);
                    self.scanned = scanned;
                }
                Msg::Done { generation, truncated } if generation == current => {
                    self.running = false;
                    self.truncated = truncated;
                }
                _ => {}
            }
        }
    }

    fn start(&mut self, query: String, root: PathBuf, ctx: &egui::Context) {
        self.cancel();
        let generation = self.generation.load(Ordering::SeqCst);
        self.searched = Some((query.clone(), root.clone()));
        self.tracks.clear();
        self.folders.clear();
        self.scanned = 0;
        self.truncated = false;
        self.running = true;

        let terms = terms(&query);
        let current = Arc::clone(&self.generation);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let truncated = walk(&root, &terms, || current.load(Ordering::SeqCst) != generation, |tracks, folders, scanned| {
                let _ = tx.send(Msg::Batch { generation, tracks, folders, scanned });
                ctx.request_repaint();
            });
            let _ = tx.send(Msg::Done { generation, truncated });
            ctx.request_repaint();
        });
    }
}

/// Lowercased, whitespace-separated words; all must appear in a match.
fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn matches(haystack: &str, terms: &[String]) -> bool {
    !terms.is_empty() && terms.iter().all(|t| haystack.contains(t.as_str()))
}

/// Breadth-first walk under `root`, so shallow (usually more relevant)
/// matches arrive first. Symlinked folders aren't followed, which rules out
/// loops. Calls `emit` with new results after each folder that had any (or
/// every 64 folders, to report progress) and returns whether it stopped
/// early at the result limits. Stops quietly when `cancelled()`.
fn walk(
    root: &Path,
    terms: &[String],
    cancelled: impl Fn() -> bool,
    mut emit: impl FnMut(Vec<PathBuf>, Vec<PathBuf>, usize),
) -> bool {
    let mut queue = VecDeque::from([root.to_path_buf()]);
    let (mut track_count, mut folder_count, mut scanned) = (0, 0, 0);
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).to_string_lossy().to_lowercase();

    while let Some(dir) = queue.pop_front() {
        if cancelled() {
            return false;
        }
        scanned += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let dir_matched = matches(&rel(&dir), terms);
        let (mut tracks, mut folders) = (Vec::new(), Vec::new());
        let mut names: Vec<(PathBuf, bool)> = entries
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| Some((e.path(), e.file_type().ok()?.is_dir())))
            .collect();
        names.sort_by_key(|(p, _)| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for (path, is_dir) in names {
            if is_dir {
                if root == Path::new("/") && SKIP_UNDER_ROOT.iter().any(|s| path == Path::new(s)) {
                    continue;
                }
                // Report a folder where the match first becomes complete, not
                // every subfolder of a matching one.
                if !dir_matched && matches(&rel(&path), terms) && folder_count < MAX_FOLDERS {
                    folder_count += 1;
                    folders.push(path.clone());
                }
                queue.push_back(path);
            } else if metadata::is_audio(&path) && matches(&rel(&path), terms) {
                track_count += 1;
                tracks.push(path);
                if track_count >= MAX_TRACKS {
                    emit(tracks, folders, scanned);
                    return true;
                }
            }
        }
        if !tracks.is_empty() || !folders.is_empty() || scanned % 64 == 0 {
            emit(tracks, folders, scanned);
        }
    }
    emit(Vec::new(), Vec::new(), scanned);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A small music tree; `name` keeps parallel tests out of each other's way.
    fn library(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("unamp-search-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, files) in [
            ("Daft Punk/Homework", &["01 Daftendirekt.flac", "03 Revolution 909.flac", "cover.jpg"][..]),
            ("Daft Punk/Discovery", &["01 One More Time.mp3"][..]),
            ("Air/Moon Safari", &["01 La femme d'argent.mp3"][..]),
            (".hidden", &["secret homework.mp3"][..]),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            for f in files {
                std::fs::write(root.join(dir).join(f), b"").unwrap();
            }
        }
        root
    }

    fn run(root: &Path, query: &str) -> (Vec<String>, Vec<String>, bool) {
        let found = RefCell::new((Vec::new(), Vec::new()));
        let truncated = walk(root, &terms(query), || false, |t, f, _| {
            let mut found = found.borrow_mut();
            found.0.extend(t);
            found.1.extend(f);
        });
        let (t, f) = found.into_inner();
        let names = |v: Vec<PathBuf>| {
            v.iter()
                .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        (names(t), names(f), truncated)
    }

    #[test]
    fn all_words_must_match_somewhere_in_the_path() {
        let root = library("words");
        let (tracks, folders, truncated) = run(&root, "daft homework");
        assert_eq!(tracks, ["Daft Punk/Homework/01 Daftendirekt.flac", "Daft Punk/Homework/03 Revolution 909.flac"]);
        assert_eq!(folders, ["Daft Punk/Homework"]);
        assert!(!truncated);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn matching_is_case_insensitive_and_skips_hidden_and_non_audio() {
        let root = library("case");
        let (tracks, _, _) = run(&root, "MOON");
        assert_eq!(tracks, ["Air/Moon Safari/01 La femme d'argent.mp3"]);
        let (tracks, _, _) = run(&root, "secret");
        assert!(tracks.is_empty(), "{tracks:?}");
        // Only cover.jpg has "jpg" in its path, and it isn't audio.
        let (tracks, _, _) = run(&root, "jpg");
        assert!(tracks.is_empty(), "{tracks:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_is_reported_once_not_with_every_subfolder() {
        let root = library("folder_once");
        let (_, folders, _) = run(&root, "daft");
        assert_eq!(folders, ["Daft Punk"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelled_walk_stops_without_results() {
        let root = library("cancel");
        let mut emitted = 0;
        walk(&root, &terms("daft"), || true, |_, _, _| emitted += 1);
        assert_eq!(emitted, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(!matches("anything", &terms("   ")));
    }
}
