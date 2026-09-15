use crate::SystemTime;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
pub trait SaveStore: Send + Sync {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, name: &str, data: &[u8]) -> Result<(), String>;
    fn delete(&self, name: &str);
    fn copy(&self, src: &str, dst: &str) -> Result<(), String>;
    fn last_modified(&self, name: &str) -> Option<SystemTime>;
}
pub struct FsStore {
    root: PathBuf,
}
impl FsStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}
impl SaveStore for FsStore {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        match std::fs::read(self.path(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        if let Err(error) = std::fs::create_dir_all(&self.root) {
            return Err(format!("create {}: {}", self.root.display(), error));
        }
        std::fs::write(self.path(name), data).map_err(|error| error.to_string())
    }
    fn delete(&self, name: &str) {
        if let Err(error) = std::fs::remove_file(self.path(name)) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[STORAGE] failed to delete {}: {}", name, error);
            }
        }
    }
    fn copy(&self, src: &str, dst: &str) -> Result<(), String> {
        if let Err(error) = std::fs::create_dir_all(&self.root) {
            return Err(format!("create {}: {}", self.root.display(), error));
        }
        std::fs::copy(self.path(src), self.path(dst))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    fn last_modified(&self, name: &str) -> Option<SystemTime> {
        let modified = std::fs::metadata(self.path(name))
            .and_then(|metadata| metadata.modified())
            .ok()?;
        Some(native_system_time(modified))
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn native_system_time(value: std::time::SystemTime) -> SystemTime {
    value
}
#[cfg(target_arch = "wasm32")]
fn native_system_time(value: std::time::SystemTime) -> SystemTime {
    SystemTime::UNIX_EPOCH
        + value.duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
}
struct MemoryEntry {
    data: Vec<u8>,
    modified: SystemTime,
}
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<BTreeMap<String, MemoryEntry>>,
}
impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn names(&self) -> Vec<String> {
        self.entries.lock().unwrap().keys().cloned().collect()
    }
}
impl SaveStore for MemoryStore {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.entries.lock().unwrap().get(name).map(|entry| entry.data.clone()))
    }
    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let modified = SystemTime::now();
        self.entries
            .lock()
            .unwrap()
            .insert(
                name.to_string(),
                MemoryEntry {
                    data: data.to_vec(),
                    modified,
                },
            );
        Ok(())
    }
    fn delete(&self, name: &str) {
        self.entries.lock().unwrap().remove(name);
    }
    fn copy(&self, src: &str, dst: &str) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        let Some(source) = entries.get(src) else {
            return Err(format!("{src} does not exist"));
        };
        let entry = MemoryEntry {
            data: source.data.clone(),
            modified: source.modified,
        };
        entries.insert(dst.to_string(), entry);
        Ok(())
    }
    fn last_modified(&self, name: &str) -> Option<SystemTime> {
        self.entries.lock().unwrap().get(name).map(|entry| entry.modified)
    }
}

