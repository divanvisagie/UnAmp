//! The sidebar's folder tree: a collapsible tree under the current location
//! whose subfolders are listed on worker threads the first time a folder is
//! expanded, so a slow or hung network share never blocks the UI
//! (see ADR-0014).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use egui::collapsing_header::CollapsingState;

enum Children {
    Loading,
    Loaded(Vec<(PathBuf, String)>),
    Failed(String),
}

struct Listing {
    path: PathBuf,
    result: Result<Vec<(PathBuf, String)>, String>,
}

pub struct FolderTree {
    root: PathBuf,
    root_label: String,
    children: HashMap<PathBuf, Children>,
    tx: mpsc::Sender<Listing>,
    rx: mpsc::Receiver<Listing>,
    /// Folder whose ancestors should be opened (and which should be
    /// scrolled into view) once the tree has loaded far enough to show it.
    reveal: Option<PathBuf>,
}

impl FolderTree {
    pub fn new(root: PathBuf, root_label: String) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            root,
            root_label,
            children: HashMap::new(),
            tx,
            rx,
            reveal: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_label(&self) -> &str {
        &self.root_label
    }

    /// Re-roots the tree. Listings already loaded are kept.
    pub fn set_root(&mut self, root: PathBuf, label: String) {
        self.root = root;
        self.root_label = label;
    }

    /// Opens `path` and the folders leading to it, and scrolls it into view.
    pub fn reveal(&mut self, path: &Path) {
        self.reveal = path.starts_with(&self.root).then(|| path.to_path_buf());
    }

    /// Forgets `path`'s listing so it's read again next time it's shown.
    pub fn refresh(&mut self, path: &Path) {
        if !matches!(self.children.get(path), Some(Children::Loading)) {
            self.children.remove(path);
        }
    }

    pub fn poll(&mut self) {
        while let Ok(Listing { path, result }) = self.rx.try_recv() {
            let children = match result {
                Ok(list) => Children::Loaded(list),
                Err(e) => Children::Failed(e),
            };
            self.children.insert(path, children);
        }
    }

    fn request(&mut self, path: &Path, ctx: &egui::Context) {
        self.children.insert(path.to_path_buf(), Children::Loading);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = list_subfolders(&path);
            let _ = tx.send(Listing { path, result });
            ctx.request_repaint();
        });
    }

    /// Draws the tree; returns a folder the user clicked.
    pub fn show(&mut self, ui: &mut egui::Ui, current: &Path) -> Option<PathBuf> {
        let mut clicked = None;
        let root = self.root.clone();
        let label = self.root_label.clone();
        self.node(ui, &root, &label, current, true, &mut clicked);
        clicked
    }

    fn node(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        name: &str,
        current: &Path,
        is_root: bool,
        clicked: &mut Option<PathBuf>,
    ) {
        let id = ui.make_persistent_id(("folder_tree", path));
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, is_root);
        // Opens the folders leading to the revealed one, and that folder
        // itself, so clicking a folder both selects and expands it.
        let revealing = self.reveal.as_deref().is_some_and(|r| r.starts_with(path));
        if revealing && !state.is_open() {
            state.set_open(true);
        }
        let is_current = path == current;
        let icon = if is_current { "\u{1F4C2}" } else { "\u{1F4C1}" };
        let label = format!("{icon} {name}");

        let header = |ui: &mut egui::Ui, clicked: &mut Option<PathBuf>| {
            let resp = ui.selectable_label(is_current, &label).on_hover_text(path.display().to_string());
            if resp.clicked() {
                *clicked = Some(path.to_path_buf());
            }
            resp
        };

        // A folder known to have no subfolders gets no expander.
        if matches!(self.children.get(path), Some(Children::Loaded(c)) if c.is_empty()) {
            let resp = ui
                .horizontal(|ui| {
                    ui.add_space(ui.spacing().icon_width + ui.spacing().icon_spacing);
                    header(ui, clicked)
                })
                .inner;
            self.finish_reveal(path, &resp);
            return;
        }

        let header_resp = state.show_header(ui, |ui| header(ui, clicked));
        let (_, header_out, _) = header_resp.body(|ui| {
            match self.children.get(path) {
                None => {
                    self.request(path, ui.ctx());
                    ui.spinner();
                }
                Some(Children::Loading) => {
                    ui.spinner();
                }
                Some(Children::Failed(e)) => {
                    ui.label(egui::RichText::new(e.as_str()).weak().small());
                }
                Some(Children::Loaded(list)) => {
                    for (child, child_name) in list.clone() {
                        self.node(ui, &child, &child_name, current, false, clicked);
                    }
                }
            }
        });
        self.finish_reveal(path, &header_out.inner);
    }

    fn finish_reveal(&mut self, path: &Path, resp: &egui::Response) {
        if self.reveal.as_deref() == Some(path) {
            resp.scroll_to_me(Some(egui::Align::Center));
            self.reveal = None;
        }
    }
}

/// Subfolders of `dir`, sorted case-insensitively, hidden ones skipped.
fn list_subfolders(dir: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    })?;
    let mut out: Vec<(PathBuf, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            // d_type avoids a stat per entry; only symlinks need following.
            let is_dir = match entry.file_type() {
                Ok(ft) if ft.is_symlink() => path.is_dir(),
                Ok(ft) => ft.is_dir(),
                Err(_) => false,
            };
            is_dir.then_some((path, name))
        })
        .collect();
    out.sort_by_key(|(_, name)| name.to_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_visible_subfolders_sorted() {
        let dir = std::env::temp_dir().join(format!("unamp-tree-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["beta", "Alpha", ".hidden", "gamma/deeper"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("song.mp3"), b"").unwrap();
        let names: Vec<String> = list_subfolders(&dir).unwrap().into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["Alpha", "beta", "gamma"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_folder_is_an_error() {
        assert!(list_subfolders(Path::new("/definitely/not/here")).is_err());
    }

    #[test]
    fn reveal_ignores_paths_outside_the_root() {
        let mut tree = FolderTree::new(PathBuf::from("/music"), "Music".into());
        tree.reveal(Path::new("/elsewhere/x"));
        assert!(tree.reveal.is_none());
        tree.reveal(Path::new("/music/a/b"));
        assert_eq!(tree.reveal.as_deref(), Some(Path::new("/music/a/b")));
    }
}
