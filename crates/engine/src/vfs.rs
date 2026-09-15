use formats::archive::ArcIndex;
use formats::remote::RemoteReader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
const VOLUME_SUFFIXES: [&str; 13] = [
    "12",
    "11",
    "10",
    "9",
    "8",
    "7",
    "6",
    "5",
    "4",
    "3",
    "2",
    "1",
    "",
];
pub struct Vfs {
    asset_dirs: Vec<PathBuf>,
    file_index: HashMap<String, PathBuf>,
    mem_files: HashMap<String, Arc<Vec<u8>>>,
    remote_files: HashMap<String, RemoteFile>,
    base_path: Option<PathBuf>,
    archives: Vec<ArcIndex>,
}
#[derive(Clone)]
struct RemoteFile {
    reader: Arc<dyn RemoteReader>,
    path: String,
    size: u64,
}
impl Vfs {
    pub fn new(asset_root: &Path, arc_dir: &Path) -> Self {
        let mut asset_dirs = Vec::new();
        let mut file_index = HashMap::new();
        if asset_root.is_dir() {
            asset_dirs.push(asset_root.to_path_buf());
            index_dir_recursive(asset_root, &mut file_index);
            eprintln!(
                "[VFS] indexed {} ({} files)", asset_root.display(), file_index.len()
            );
        }
        if arc_dir.is_dir() && !asset_dirs.iter().any(|dir| dir == arc_dir) {
            asset_dirs.push(arc_dir.to_path_buf());
            index_dir_recursive(arc_dir, &mut file_index);
            eprintln!(
                "[VFS] indexed loose ARC root {} ({} files total)", arc_dir.display(),
                file_index.len()
            );
        }
        let mut archives = Vec::new();
        if arc_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(arc_dir) {
                let mut arcs: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| {
                                extension.eq_ignore_ascii_case("arc")
                            })
                    })
                    .collect();
                arcs.sort();
                for arc_path in arcs {
                    match ArcIndex::open(&arc_path) {
                        Ok(idx) => {
                            eprintln!(
                                "[VFS] indexed {} ({} entries)", arc_path.display(), idx
                                .entry_count()
                            );
                            archives.push(idx);
                        }
                        Err(e) => {
                            eprintln!(
                                "[VFS] WARN: failed to index {}: {}", arc_path.display(), e
                            );
                        }
                    }
                }
            }
        }
        Self {
            asset_dirs,
            file_index,
            mem_files: HashMap::new(),
            remote_files: HashMap::new(),
            base_path: None,
            archives,
        }
    }
    pub fn with_arc_priority(mut self, priority: &[&str]) -> Self {
        self.reorder_archives(priority);
        self
    }
    fn reorder_archives(&mut self, priority: &[&str]) {
        let mut ordered: Vec<ArcIndex> = Vec::with_capacity(self.archives.len());
        let mut remaining = std::mem::take(&mut self.archives);
        for prefix in priority {
            for suffix in VOLUME_SUFFIXES {
                let want = format!("{prefix}{suffix}");
                if let Some(pos) = remaining
                    .iter()
                    .position(|idx| idx.archive_stem().eq_ignore_ascii_case(&want))
                {
                    ordered.push(remaining.swap_remove(pos));
                }
            }
        }
        remaining.sort_by_key(|idx| idx.archive_stem().to_lowercase());
        ordered.append(&mut remaining);
        self.archives = ordered;
    }
    pub fn from_memory(entries: Vec<(String, Vec<u8>)>) -> Result<Self, String> {
        let mut vfs = Self {
            asset_dirs: Vec::new(),
            file_index: HashMap::new(),
            mem_files: HashMap::new(),
            remote_files: HashMap::new(),
            base_path: None,
            archives: Vec::new(),
        };
        for (path, data) in entries {
            vfs.mount_memory(&path, data)?;
        }
        Ok(vfs)
    }
    pub fn from_remote(
        reader: Arc<dyn RemoteReader>,
        files: Vec<(String, u64)>,
    ) -> Result<Self, String> {
        let mut vfs = Self {
            asset_dirs: Vec::new(),
            file_index: HashMap::new(),
            mem_files: HashMap::new(),
            remote_files: HashMap::new(),
            base_path: None,
            archives: Vec::new(),
        };
        let mut archives = Vec::new();
        for (path, size) in files {
            let normalized = path.replace('\\', "/");
            let bare = normalized
                .rsplit('/')
                .next()
                .unwrap_or(&normalized)
                .to_lowercase();
            if bare.ends_with(".arc") {
                let index = ArcIndex::from_remote(Arc::clone(&reader), &normalized)
                    .map_err(|error| {
                        format!("failed to index remote archive {normalized}: {error}")
                    })?;
                eprintln!(
                    "[VFS] indexed remote archive {} ({} entries)", normalized, index
                    .entry_count()
                );
                archives.push(index);
                continue;
            }
            eprintln!("[VFS] mounted remote file {}", normalized);
            let entry = RemoteFile {
                reader: Arc::clone(&reader),
                path: normalized.clone(),
                size,
            };
            vfs.remote_files.insert(normalized.to_lowercase(), entry.clone());
            vfs.remote_files.insert(bare, entry);
        }
        archives.sort_by_key(|index| index.archive_stem().to_lowercase());
        vfs.archives = archives;
        Ok(vfs)
    }
    pub fn mount_memory(&mut self, path: &str, data: Vec<u8>) -> Result<(), String> {
        let normalized = path.replace('\\', "/");
        let bare = normalized.rsplit('/').next().unwrap_or(&normalized).to_lowercase();
        if bare.ends_with(".arc") {
            let index = ArcIndex::from_memory(data)
                .map_err(|error| {
                    format!("failed to index memory archive {normalized}: {error}")
                })?;
            eprintln!(
                "[VFS] indexed memory archive {} ({} entries)", normalized, index
                .entry_count()
            );
            self.archives.push(index);
            return Ok(());
        }
        let data = Arc::new(data);
        eprintln!("[VFS] mounted memory file {}", normalized);
        self.mem_files.insert(normalized.to_lowercase(), Arc::clone(&data));
        self.mem_files.insert(bare, data);
        Ok(())
    }
    pub fn with_base_path(mut self, base: PathBuf) -> Self {
        self.base_path = Some(base);
        self
    }
    pub fn find(&self, name: &str) -> Option<Vec<u8>> {
        if let Some(path) = self.find_path(name) {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
        if let Some(data) = self.mem_files.get(&name.to_lowercase()) {
            return Some(data.as_ref().clone());
        }
        if let Some(file) = self.remote_files.get(&name.to_lowercase()) {
            let size = u32::try_from(file.size).unwrap_or(u32::MAX);
            if let Ok(data) = file.reader.read_at(&file.path, 0, size) {
                return Some(data);
            }
        }
        for arc in &self.archives {
            if let Some(data) = arc.find(name) {
                return Some(data);
            }
        }
        None
    }
    pub fn find_path(&self, name: &str) -> Option<PathBuf> {
        if name.starts_with('\\') || name.starts_with('/') || name.contains(':') {
            return None;
        }
        if name.contains('/') || name.contains('\\') {
            for dir in &self.asset_dirs {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        if let Some(base) = &self.base_path {
            let candidate = base.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let name_lower = name.to_lowercase();
        let bare = name_lower.rsplit(['/', '\\']).next().unwrap_or(&name_lower);
        if let Some(path) = self.file_index.get(bare) {
            return path.is_file().then(|| path.clone());
        }
        None
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn find_remote_path(&self, name: &str) -> Option<String> {
        let name_lower = name.to_lowercase();
        if let Some(file) = self.remote_files.get(&name_lower) {
            return Some(file.path.clone());
        }
        let bare = name_lower.rsplit(['/', '\\']).next().unwrap_or(&name_lower);
        self.remote_files.get(bare).map(|file| file.path.clone())
    }
    pub fn exists(&self, name: &str) -> bool {
        if self.find_path(name).is_some() {
            return true;
        }
        if self.mem_files.contains_key(&name.to_lowercase()) {
            return true;
        }
        if self.remote_files.contains_key(&name.to_lowercase()) {
            return true;
        }
        for arc in &self.archives {
            if arc.find_name(name).is_some() {
                return true;
            }
        }
        false
    }
}
fn index_dir_recursive(dir: &Path, index: &mut HashMap<String, PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if dir_contains_patch_manifest(&path) {
                eprintln!(
                    "[VFS] skipping patch directory {} (presentation patches stay outside the game VFS)",
                    path.display()
                );
                continue;
            }
            index_dir_recursive(&path, index);
        } else if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
            index.entry(fname.to_lowercase()).or_insert(path);
        }
    }
}
fn dir_contains_patch_manifest(dir: &Path) -> bool {
    dir.join("patch.toml").is_file()
}

