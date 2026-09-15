use super::native_script_filename;
use crate::patch::PatchBundle;
use crate::vfs::Vfs;
use formats::script::MjoFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vm::host::LoadedScript;
pub(crate) trait ScriptSource {
    fn resolve(&self, name: &str) -> Option<LoadedScript>;
}
pub(crate) struct MjoSource<'a> {
    pub(crate) vfs: &'a Vfs,
    pub(crate) patch: Option<&'a PatchBundle>,
}
impl ScriptSource for MjoSource<'_> {
    fn resolve(&self, name: &str) -> Option<LoadedScript> {
        let filename = native_script_filename(name);
        let data = self.vfs.find(&filename)?;
        let mut mjo = MjoFile::parse(&data).ok()?;
        if let Some(patch) = self.patch {
            match patch.apply_script_transitions(&filename, &mut mjo.data) {
                Ok(0) => {}
                Ok(count) => {
                    eprintln!(
                        "[PATCH] {}: replaced {} dot-matrix transition request(s) with fade",
                        filename, count
                    )
                }
                Err(error) => {
                    eprintln!(
                        "[PATCH] {}: transition override ignored: {}", filename, error
                    )
                }
            }
        }
        Some(loaded_script(filename, mjo))
    }
}
pub(crate) fn loaded_script(filename: String, mjo: MjoFile) -> LoadedScript {
    LoadedScript {
        name: filename,
        code_crc32: formats::crypto::crc32(&mjo.data),
        code: mjo.data,
        main_offset: mjo.main_offset,
        line_count: mjo.line_count,
        entries: mjo.entries.iter().map(|e| (e.name_hash, e.offset)).collect(),
    }
}
pub struct IrSource<'a> {
    pub(crate) root: &'a Path,
}
impl IrSource<'_> {
    fn find(&self, stem: &str) -> Option<PathBuf> {
        let wanted = stem.to_ascii_lowercase();
        let mut result = None;
        let mut stack = vec![self.root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("ir"))
                    && path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.to_ascii_lowercase() == wanted)
                {
                    result = Some(path);
                    break;
                }
            }
            if result.is_some() {
                break;
            }
        }
        result
    }
}
#[derive(Default)]
pub struct IrTranslations {
    pub text: Vec<(u32, crate::patch::Message)>,
    pub display: Vec<crate::patch::Message>,
}
impl IrSource<'_> {
    pub fn resolve_with_translations(
        &self,
        name: &str,
    ) -> Option<(LoadedScript, IrTranslations)> {
        let filename = native_script_filename(name);
        let stem = filename.strip_suffix(".mjo").unwrap_or(&filename);
        let path = self.find(stem)?;
        let text = std::fs::read_to_string(&path).ok()?;
        let (loaded, translations) = assemble_ir(filename, &text)?;
        eprintln!("[SCRIPT] IR override {:?} from {}", name, path.display());
        Some((loaded, translations))
    }
}
fn assemble_ir(filename: String, text: &str) -> Option<(LoadedScript, IrTranslations)> {
    let module = vm::ir::from_text(text).ok()?;
    let translations = IrTranslations {
        text: module
            .text_translations()
            .into_iter()
            .map(|(offset, source, text)| (
                offset,
                crate::patch::Message {
                    source,
                    text,
                },
            ))
            .collect(),
        display: module
            .display_translations()
            .into_iter()
            .filter_map(|(pattern, payload, text)| {
                let source = pattern.or(payload)?;
                Some(crate::patch::Message {
                    source: Some(source),
                    text,
                })
            })
            .collect(),
    };
    let bytes = vm::ir::to_mjo_bytes(&module).ok()?;
    let mjo = MjoFile::parse(&bytes).ok()?;
    Some((loaded_script(filename, mjo), translations))
}
pub(crate) struct MemoryIrSource {
    pub(crate) files: Arc<HashMap<String, String>>,
}
impl MemoryIrSource {
    pub fn resolve_with_translations(
        &self,
        name: &str,
    ) -> Option<(LoadedScript, IrTranslations)> {
        let filename = native_script_filename(name);
        let stem = filename.strip_suffix(".mjo").unwrap_or(&filename);
        let text = self.files.get(&stem.to_ascii_lowercase())?;
        let (loaded, translations) = assemble_ir(filename, text)?;
        eprintln!("[SCRIPT] IR override {:?} from in-memory patch scripts", name);
        Some((loaded, translations))
    }
}
impl ScriptSource for MemoryIrSource {
    fn resolve(&self, name: &str) -> Option<LoadedScript> {
        self.resolve_with_translations(name).map(|(loaded, _)| loaded)
    }
}
impl ScriptSource for IrSource<'_> {
    fn resolve(&self, name: &str) -> Option<LoadedScript> {
        self.resolve_with_translations(name).map(|(loaded, _)| loaded)
    }
}

