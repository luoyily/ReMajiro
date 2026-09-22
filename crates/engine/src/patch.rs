use crate::render_model::PresentationImage;
use encoding_rs::SHIFT_JIS;
use formats::remote::RemoteReader;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vm::text::{tokenize, ControlCode, ControlCodeKind, TextToken};
use vm::TextSite;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationConfig {
    pub scale: u32,
    pub output_width: Option<u32>,
    pub output_height: Option<u32>,
}
impl Default for PresentationConfig {
    fn default() -> Self {
        Self {
            scale: 1,
            output_width: None,
            output_height: None,
        }
    }
}
#[derive(Debug, Deserialize)]
struct Manifest {
    format: u32,
    #[serde(default)]
    id: String,
    #[serde(default)]
    localization: Option<LocalizationManifest>,
    #[serde(default)]
    presentation: Option<PresentationManifest>,
    #[serde(default)]
    transitions: Option<TransitionManifest>,
    #[serde(default)]
    save_compatibility: Option<SaveCompatibilityManifest>,
}
#[derive(Debug, Deserialize)]
struct SaveCompatibilityManifest {
    bidirectional: Option<bool>,
}
#[derive(Debug, Deserialize)]
struct LocalizationManifest {
    locale: String,
    #[serde(default)]
    fonts: Vec<PathBuf>,
}
#[derive(Debug, Deserialize)]
struct PresentationManifest {
    scale: u32,
    #[serde(default)]
    output_width: Option<u32>,
    #[serde(default)]
    output_height: Option<u32>,
}
#[derive(Debug, Deserialize)]
struct TransitionManifest {
    #[serde(default)]
    dot_matrix: Option<DotMatrixTransitionManifest>,
}
#[derive(Debug, Deserialize)]
struct DotMatrixTransitionManifest {
    replacement: TransitionReplacement,
    #[serde(default)]
    duration_ms: Option<u32>,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TransitionReplacement {
    Fade,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TransitionConfig {
    dot_matrix_fade: bool,
}
#[derive(Debug, Deserialize)]
struct ScriptCatalogFile {
    format: u32,
    #[serde(default)]
    code_crc32: Option<CrcValue>,
    messages: HashMap<String, Message>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum CrcValue {
    Number(u32),
    Text(CrcText),
}
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
struct CrcText(String);
impl CrcValue {
    fn parse(self) -> Result<u32, String> {
        match self {
            Self::Number(value) => Ok(value),
            Self::Text(value) => {
                let text = value.0.trim().trim_start_matches("0x");
                u32::from_str_radix(text, 16)
                    .map_err(|error| {
                        format!("invalid hexadecimal code_crc32 {text:?}: {error}")
                    })
            }
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub source: Option<String>,
    pub text: String,
}
#[derive(Debug)]
struct ScriptCatalog {
    code_crc32: Option<u32>,
    messages: HashMap<u32, Message>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalizedToken {
    Text(String),
    Control(ControlCode),
}
pub struct PatchBundle {
    root: PathBuf,
    source: PatchSource,
    id: String,
    locale: Option<String>,
    fonts: Vec<PathBuf>,
    presentation: PresentationConfig,
    transitions: TransitionConfig,
    one_way_save_titles: bool,
    catalogs: HashMap<String, ScriptCatalog>,
    replay_messages: Vec<Message>,
    images: HashMap<String, PathBuf>,
    ir_root: Option<PathBuf>,
    ir_scripts: usize,
    ir_names: Vec<String>,
    files: HashMap<String, PathBuf>,
}
#[derive(Clone)]
enum PatchSource {
    Disk,
    Remote(Arc<dyn RemoteReader>),
}
impl PatchBundle {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join("patch.toml");
        let source = fs::read_to_string(&manifest_path)
            .map_err(|error| {
                format!("failed to read {}: {error}", manifest_path.display())
            })?;
        let mut manifest: Manifest = toml::from_str(&source)
            .map_err(|error| {
                format!("failed to parse {}: {error}", manifest_path.display())
            })?;
        if manifest.format != 1 {
            return Err(
                format!(
                    "unsupported patch format {} in {}; expected 1", manifest.format,
                    manifest_path.display()
                ),
            );
        }
        let presentation = manifest_presentation(
            &manifest,
            &manifest_path.display().to_string(),
        )?;
        let transitions = manifest_transitions(
            &mut manifest,
            &manifest_path.display().to_string(),
        )?;
        let one_way_save_titles = manifest
            .save_compatibility
            .as_ref()
            .is_some_and(|save| !save.bidirectional.unwrap_or(true));
        let catalogs = load_catalogs(&root.join("text"))?;
        let replay_messages = catalogs
            .values()
            .flat_map(|catalog| catalog.messages.values())
            .filter(|message| message.source.is_some())
            .cloned()
            .collect();
        let images = index_images(&root.join("images"))?;
        let text_root = root.join("text");
        let mut ir_root = None;
        let mut ir_scripts = 0usize;
        if text_root.is_dir() {
            for entry in fs::read_dir(&text_root)
                .map_err(|error| {
                    format!("failed to scan {}: {error}", text_root.display())
                })?
            {
                let entry = entry
                    .map_err(|error| {
                        format!("failed to scan {}: {error}", text_root.display())
                    })?;
                if entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ir"))
                {
                    ir_scripts += 1;
                }
            }
            if ir_scripts > 0 {
                ir_root = Some(text_root);
            }
        }
        let files = index_files(&root.join("files"))?;
        let (locale, fonts) = manifest
            .localization
            .map_or(
                (None, Vec::new()),
                |value| {
                    (
                        Some(value.locale),
                        value.fonts.into_iter().map(|path| root.join(path)).collect(),
                    )
                },
            );
        let id = if manifest.id.trim().is_empty() {
            root.file_name().and_then(|name| name.to_str()).unwrap_or("patch").to_owned()
        } else {
            manifest.id
        };
        Ok(Self {
            root,
            source: PatchSource::Disk,
            id,
            locale,
            fonts,
            presentation,
            transitions,
            one_way_save_titles,
            catalogs,
            replay_messages,
            images,
            ir_root,
            ir_scripts,
            ir_names: Vec::new(),
            files,
        })
    }
    pub fn from_remote(
        reader: Arc<dyn RemoteReader>,
        listing: &[(String, u64)],
    ) -> Result<Self, String> {
        let mut manifests: Vec<&str> = listing
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| {
                let lower = name.to_lowercase();
                lower == "patch.toml" || lower.ends_with("/patch.toml")
            })
            .collect();
        manifests.sort();
        let Some(manifest_name) = manifests.first() else {
            return Err("no patch.toml in the game directory listing".into());
        };
        for extra in manifests.iter().skip(1) {
            eprintln!("[PATCH] ignoring extra patch manifest {extra}");
        }
        let root = PathBuf::from(manifest_name)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let mut prefix = root.to_string_lossy().to_string();
        if !prefix.is_empty() && !prefix.ends_with('/') {
            prefix.push('/');
        }
        let manifest_bytes = read_remote(&reader, manifest_name)?;
        let manifest_text = String::from_utf8(manifest_bytes)
            .map_err(|error| {
                format!("patch manifest {manifest_name} is not UTF-8: {error}")
            })?;
        let mut manifest: Manifest = toml::from_str(&manifest_text)
            .map_err(|error| format!("failed to parse {manifest_name}: {error}"))?;
        if manifest.format != 1 {
            return Err(
                format!(
                    "unsupported patch format {} in {manifest_name}; expected 1",
                    manifest.format
                ),
            );
        }
        let presentation = manifest_presentation(&manifest, manifest_name)?;
        let transitions = manifest_transitions(&mut manifest, manifest_name)?;
        let one_way_save_titles = manifest
            .save_compatibility
            .as_ref()
            .is_some_and(|save| !save.bidirectional.unwrap_or(true));
        let text_prefix = format!("{prefix}text/");
        let images_prefix = format!("{prefix}images/");
        let files_prefix = format!("{prefix}files/");
        let mut catalogs = HashMap::new();
        let mut images = HashMap::new();
        let mut files = HashMap::new();
        let mut ir_scripts = 0usize;
        let mut ir_names = Vec::new();
        for (name, _) in listing.iter().filter(|(name, _)| name.starts_with(&prefix)) {
            let lower = name.to_lowercase();
            let relative = &name[prefix.len()..];
            if let Some(rest) = lower.strip_prefix(&text_prefix) {
                if rest.ends_with(".json") {
                    let source_bytes = read_remote(&reader, name)?;
                    let source = String::from_utf8(source_bytes)
                        .map_err(|error| {
                            format!("text catalog {name} is not UTF-8: {error}")
                        })?;
                    let (key, catalog) = parse_catalog_file(&source, name)?;
                    catalogs.insert(key, catalog);
                } else if rest.ends_with(".ir") {
                    ir_scripts += 1;
                }
            } else if let Some(rest) = lower.strip_prefix(&images_prefix) {
                if !rest.ends_with(".png") {
                    continue;
                }
                images_insert(
                    &mut images,
                    canonical_resource_name(relative),
                    PathBuf::from(name),
                );
            } else if let Some(rest) = lower.strip_prefix(&files_prefix) {
                files_insert(&mut files, canonical_file_name(rest), PathBuf::from(name));
            }
            if relative.to_lowercase().starts_with("text/") && lower.ends_with(".ir") {
                ir_names.push(name.clone());
            }
        }
        let ir_root = (ir_scripts > 0).then(|| root.join("text"));
        let replay_messages = catalogs
            .values()
            .flat_map(|catalog| catalog.messages.values())
            .filter(|message| message.source.is_some())
            .cloned()
            .collect();
        let (locale, fonts) = manifest
            .localization
            .map_or(
                (None, Vec::new()),
                |value| {
                    (
                        Some(value.locale),
                        value.fonts.into_iter().map(|path| root.join(path)).collect(),
                    )
                },
            );
        let id = if manifest.id.trim().is_empty() {
            root.file_name().and_then(|name| name.to_str()).unwrap_or("patch").to_owned()
        } else {
            manifest.id
        };
        Ok(Self {
            root,
            source: PatchSource::Remote(reader),
            id,
            locale,
            fonts,
            presentation,
            transitions,
            one_way_save_titles,
            catalogs,
            replay_messages,
            images,
            ir_root,
            ir_scripts,
            ir_names,
            files,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn font_paths(&self) -> &[PathBuf] {
        &self.fonts
    }
    pub fn presentation(&self) -> PresentationConfig {
        self.presentation.clone()
    }
    pub fn one_way_save_titles(&self) -> bool {
        self.one_way_save_titles
    }
    pub(crate) fn apply_script_transitions(
        &self,
        script_name: &str,
        code: &mut [u8],
    ) -> Result<usize, String> {
        const PUSH_INT: u16 = 0x0800;
        const DOT_MATRIX_TRANSITION: i32 = -100_109;
        const ORDINARY_FADE_TRANSITION: i32 = -100119;
        if !self.transitions.dot_matrix_fade {
            return Ok(0);
        }
        let canonical = canonical_script_name(script_name.as_bytes());
        if matches!(canonical.as_str(), "transit" | "transit_top") {
            return Ok(0);
        }
        let mut operands = Vec::new();
        let mut ip = 0usize;
        while ip < code.len() {
            let size = vm::opcode::instruction_size(code, ip)
                .ok_or_else(|| {
                    format!(
                        "cannot inspect transition calls in {script_name}: invalid instruction at 0x{ip:08X}"
                    )
                })?;
            let opcode = u16::from_le_bytes([code[ip], code[ip + 1]]);
            if opcode == PUSH_INT {
                let operand = ip + 2;
                let value = i32::from_le_bytes(
                    code[operand..operand + 4]
                        .try_into()
                        .expect("PUSH_INT size was validated"),
                );
                if value == DOT_MATRIX_TRANSITION {
                    operands.push(operand);
                }
            }
            ip += size;
        }
        for operand in &operands {
            code[*operand..*operand + 4]
                .copy_from_slice(&ORDINARY_FADE_TRANSITION.to_le_bytes());
        }
        Ok(operands.len())
    }
    pub fn image_count(&self) -> usize {
        self.images.values().collect::<std::collections::HashSet<_>>().len()
    }
    pub fn ir_root(&self) -> Option<&Path> {
        self.ir_root.as_deref()
    }
    pub fn ir_script_count(&self) -> usize {
        self.ir_scripts
    }
    pub fn file_count(&self) -> usize {
        self.files.values().collect::<std::collections::HashSet<_>>().len()
    }
    pub fn file_override(&self, name: &str) -> Option<Vec<u8>> {
        let path = self.file_override_path(name)?;
        self.read_bytes(path).ok()
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, String> {
        match &self.source {
            PatchSource::Disk => {
                fs::read(path)
                    .map_err(|error| {
                        format!("failed to read {}: {error}", path.display())
                    })
            }
            PatchSource::Remote(reader) => read_remote(reader, &path.to_string_lossy()),
        }
    }
    pub fn font_bytes(&self) -> Vec<Vec<u8>> {
        self.fonts.iter().filter_map(|path| self.read_bytes(path).ok()).collect()
    }
    pub fn ir_script_bytes(&self) -> Vec<(String, String)> {
        self.ir_names
            .iter()
            .filter_map(|name| {
                let bytes = self.read_bytes(Path::new(name)).ok()?;
                let text = String::from_utf8(bytes).ok()?;
                let stem = Path::new(name).file_stem()?.to_str()?.to_ascii_lowercase();
                Some((stem, text))
            })
            .collect()
    }
    fn file_override_path(&self, name: &str) -> Option<&Path> {
        if name.is_empty() {
            return None;
        }
        self.files
            .get(&canonical_file_name(name))
            .or_else(|| {
                let leaf = name
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(name)
                    .to_lowercase();
                self.files.get(&leaf)
            })
            .map(PathBuf::as_path)
    }
    pub fn presentation_image(
        &self,
        logical_name: &str,
        logical_width: u32,
        logical_height: u32,
    ) -> Result<Option<Arc<PresentationImage>>, String> {
        if self.presentation.scale == 1 {
            return Ok(None);
        }
        let Some(path) = self.image_path(logical_name) else {
            return Ok(None);
        };
        let bytes = self.read_bytes(path)?;
        let image = decode_png_bytes(&bytes, &path.to_string_lossy())?;
        let expected_width = logical_width.saturating_mul(self.presentation.scale);
        let expected_height = logical_height.saturating_mul(self.presentation.scale);
        if image.width != expected_width || image.height != expected_height {
            return Err(
                format!(
                    "presentation image {} is {}x{}, expected {}x{} for logical resource {} at scale {}",
                    path.display(), image.width, image.height, expected_width,
                    expected_height, logical_name, self.presentation.scale
                ),
            );
        }
        Ok(Some(Arc::new(image)))
    }
    pub fn localize(&self, site: &TextSite, raw: &[u8]) -> Option<Vec<LocalizedToken>> {
        if site.render_offset == usize::MAX {
            return self.localize_replay(raw);
        }
        let script = canonical_script_name(&site.script_name);
        let catalog = self.catalogs.get(&script)?;
        if catalog.code_crc32.is_some_and(|expected| expected != site.code_crc32) {
            eprintln!(
                "[PATCH] text catalog ignored for {}: code CRC is {:08X}, catalog expects {:08X}",
                script, site.code_crc32, catalog.code_crc32.unwrap()
            );
            return None;
        }
        let message = catalog.messages.get(&(site.render_offset as u32))?;
        localize_tokens(raw, message)
    }
    fn localize_replay(&self, raw: &[u8]) -> Option<Vec<LocalizedToken>> {
        localize_replay_messages(&self.replay_messages, raw)
    }
    fn image_path(&self, logical_name: &str) -> Option<&Path> {
        let canonical = canonical_resource_name(logical_name);
        self.images
            .get(&canonical)
            .or_else(|| {
                Path::new(&canonical)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| self.images.get(name))
            })
            .map(PathBuf::as_path)
    }
}
fn load_catalogs(root: &Path) -> Result<HashMap<String, ScriptCatalog>, String> {
    let mut catalogs = HashMap::new();
    if !root.exists() {
        return Ok(catalogs);
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("failed to scan {}: {error}", root.display()))?
    {
        let entry = entry
            .map_err(|error| format!("failed to scan {}: {error}", root.display()))?;
        let path = entry.path();
        if !path.is_file()
            || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let (key, catalog) = parse_catalog_file(&source, &path.to_string_lossy())?;
        catalogs.insert(key, catalog);
    }
    Ok(catalogs)
}
fn parse_catalog_file(
    source: &str,
    display: &str,
) -> Result<(String, ScriptCatalog), String> {
    let path = Path::new(display);
    let file: ScriptCatalogFile = serde_json::from_str(source)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if file.format != 1 {
        return Err(
            format!(
                "unsupported text catalog format {} in {}; expected 1", file.format, path
                .display()
            ),
        );
    }
    let mut messages = HashMap::new();
    for (offset, message) in file.messages {
        let normalized = offset.trim().trim_start_matches("0x");
        let offset = u32::from_str_radix(normalized, 16)
            .map_err(|error| {
                format!(
                    "invalid TEXT_RENDER offset {offset:?} in {}: {error}", path
                    .display()
                )
            })?;
        messages.insert(offset, message);
    }
    let code_crc32 = file.code_crc32.map(CrcValue::parse).transpose()?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("catalog filename is not Unicode: {}", path.display()))?;
    let key = canonical_script_name(name.as_bytes());
    Ok((
        key,
        ScriptCatalog {
            code_crc32,
            messages,
        },
    ))
}
fn manifest_presentation(
    manifest: &Manifest,
    display: &str,
) -> Result<PresentationConfig, String> {
    Ok(
        match manifest.presentation {
            Some(ref value) => {
                if value.scale == 0 || value.scale > 4 {
                    return Err(
                        format!(
                            "presentation.scale must be an integer from 1 through 4, got {} in {display}",
                            value.scale
                        ),
                    );
                }
                PresentationConfig {
                    scale: value.scale,
                    output_width: value.output_width,
                    output_height: value.output_height,
                }
            }
            None => PresentationConfig::default(),
        },
    )
}
fn manifest_transitions(
    manifest: &mut Manifest,
    display: &str,
) -> Result<TransitionConfig, String> {
    Ok(
        match manifest.transitions.take().and_then(|value| value.dot_matrix) {
            Some(value) => {
                if let Some(duration) = value.duration_ms {
                    if duration > 60_000 {
                        return Err(
                            format!(
                                "transitions.dot_matrix.duration_ms must be at most 60000, got {duration} in {display}"
                            ),
                        );
                    }
                }
                match value.replacement {
                    TransitionReplacement::Fade => {
                        TransitionConfig {
                            dot_matrix_fade: true,
                        }
                    }
                }
            }
            None => TransitionConfig::default(),
        },
    )
}
fn read_remote(reader: &Arc<dyn RemoteReader>, name: &str) -> Result<Vec<u8>, String> {
    let size = reader.size_of(name)?;
    let size = u32::try_from(size)
        .map_err(|_| format!("{name} is too large to read whole"))?;
    reader.read_at(name, 0, size)
}
fn images_insert(images: &mut HashMap<String, PathBuf>, key: String, path: PathBuf) {
    if let Some(filename) = Path::new(&key).file_name().and_then(|name| name.to_str()) {
        images.entry(filename.to_owned()).or_insert_with(|| path.clone());
    }
    images.insert(key, path);
}
fn files_insert(files: &mut HashMap<String, PathBuf>, key: String, path: PathBuf) {
    if let Some(filename) = Path::new(&key).file_name().and_then(|name| name.to_str()) {
        files.entry(filename.to_owned()).or_insert_with(|| path.clone());
    }
    files.insert(key, path);
}
fn index_images(root: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut images = HashMap::new();
    if !root.exists() {
        return Ok(images);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("failed to scan {}: {error}", directory.display()))?
        {
            let entry = entry
                .map_err(|error| {
                    format!("failed to scan {}: {error}", directory.display())
                })?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("png")) {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let key = canonical_resource_name(&relative.to_string_lossy());
            images.insert(key.clone(), path.clone());
            if let Some(filename) = Path::new(&key)
                .file_name()
                .and_then(|name| name.to_str())
            {
                images.entry(filename.to_owned()).or_insert(path);
            }
        }
    }
    Ok(images)
}
fn index_files(root: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut files = HashMap::new();
    if !root.exists() {
        return Ok(files);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("failed to scan {}: {error}", directory.display()))?
        {
            let entry = entry
                .map_err(|error| {
                    format!("failed to scan {}: {error}", directory.display())
                })?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let key = canonical_file_name(&relative.to_string_lossy());
            files.insert(key.clone(), path.clone());
            if let Some(filename) = Path::new(&key)
                .file_name()
                .and_then(|name| name.to_str())
            {
                files.entry(filename.to_owned()).or_insert(path);
            }
        }
    }
    Ok(files)
}
pub(crate) fn canonical_script_name(bytes: &[u8]) -> String {
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
    let bytes = bytes
        .rsplit(|byte| matches!(* byte, b'/' | b'\\'))
        .next()
        .unwrap_or(bytes);
    let decoded = std::str::from_utf8(bytes)
        .map(std::borrow::Cow::Borrowed)
        .unwrap_or_else(|_| SHIFT_JIS.decode(bytes).0);
    strip_script_extension(decoded.trim()).to_lowercase()
}
fn strip_script_extension(name: &str) -> &str {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return name;
    };
    let is_extension = (1..=8).contains(&extension.len())
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric());
    if is_extension && !stem.is_empty() { stem } else { name }
}
fn canonical_resource_name(name: &str) -> String {
    let normalized = name.replace('\\', "/").to_lowercase();
    let path = Path::new(&normalized);
    let mut output = path.with_extension("png").to_string_lossy().replace('\\', "/");
    while output.starts_with("./") {
        output.drain(..2);
    }
    output
}
fn canonical_file_name(name: &str) -> String {
    let mut output = name.replace('\\', "/").to_lowercase();
    while output.starts_with("./") {
        output.drain(..2);
    }
    output
}
fn decode_png_bytes(bytes: &[u8], display: &str) -> Result<PresentationImage, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder
        .set_transformations(
            png::Transformations::EXPAND | png::Transformations::STRIP_16,
        );
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("failed to read PNG header {display}: {error}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("failed to decode PNG {display}: {error}"))?;
    let source = &buffer[..info.buffer_size()];
    let mut pixels = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(source),
        png::ColorType::Rgb => {
            for pixel in source.chunks_exact(3) {
                pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &value in source {
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in source.chunks_exact(2) {
                pixels.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Indexed => {
            return Err(format!("PNG palette expansion failed for {display}",));
        }
    }
    Ok(PresentationImage {
        width: info.width,
        height: info.height,
        pixels: Arc::new(pixels),
    })
}
pub(crate) fn localize_replay_messages(
    messages: &[Message],
    raw: &[u8],
) -> Option<Vec<LocalizedToken>> {
    let actual = source_markup(&tokenize(raw));
    let mut matches = messages
        .iter()
        .filter(|message| {
            message
                .source
                .as_deref()
                .is_some_and(|source| match_template(source, &actual).is_some())
        });
    let selected = matches.next()?;
    if matches.any(|candidate| candidate.text != selected.text) {
        return None;
    }
    localize_tokens(raw, selected)
}
pub(crate) fn localize_tokens(
    raw: &[u8],
    message: &Message,
) -> Option<Vec<LocalizedToken>> {
    let original = tokenize(raw);
    let actual = source_markup(&original);
    let captures = message
        .source
        .as_deref()
        .and_then(|source| match_template(source, &actual))
        .unwrap_or_default();
    let target = substitute(&message.text, &captures)?;
    if !target.contains("/>") {
        let mut inserted = false;
        let mut output = Vec::new();
        for token in original {
            match token {
                TextToken::Text(_) if !inserted => {
                    output.push(LocalizedToken::Text(target.clone()));
                    inserted = true;
                }
                TextToken::Text(_) => {}
                TextToken::Control(control) => {
                    output.push(LocalizedToken::Control(control))
                }
            }
        }
        if !inserted {
            output.insert(0, LocalizedToken::Text(target));
        }
        return Some(output);
    }
    let controls: Vec<_> = original
        .into_iter()
        .filter_map(|token| match token {
            TextToken::Control(control) => Some(control),
            TextToken::Text(_) => None,
        })
        .collect();
    parse_explicit_markup(&target, &controls)
}
fn source_markup(tokens: &[TextToken]) -> String {
    let mut output = String::new();
    for token in tokens {
        match token {
            TextToken::Text(bytes) => {
                let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
                let (decoded, _, _) = SHIFT_JIS.decode(bytes);
                output.push_str(&decoded);
            }
            TextToken::Control(control) => {
                output.push('<');
                output.push_str(control.kind.as_name());
                output.push_str("/>");
            }
        }
    }
    output
}
fn parse_explicit_markup(
    target: &str,
    controls: &[ControlCode],
) -> Option<Vec<LocalizedToken>> {
    let mut output = Vec::new();
    let mut used = vec![false; controls.len()];
    let mut cursor = 0;
    while let Some(relative) = target[cursor..].find('<') {
        let start = cursor + relative;
        if start > cursor {
            output.push(LocalizedToken::Text(target[cursor..start].to_owned()));
        }
        let close = target[start..].find("/>")? + start;
        let name = target[start + 1..close].trim();
        let kind = control_kind(name)?;
        let index = controls
            .iter()
            .enumerate()
            .find(|(index, control)| !used[*index] && control.kind == kind)
            .map(|(index, _)| index)?;
        used[index] = true;
        output.push(LocalizedToken::Control(controls[index].clone()));
        cursor = close + 2;
    }
    if cursor < target.len() {
        output.push(LocalizedToken::Text(target[cursor..].to_owned()));
    }
    if used.iter().all(|used| *used) { Some(output) } else { None }
}
fn control_kind(name: &str) -> Option<ControlCodeKind> {
    (0u8..=u8::MAX)
        .filter_map(ControlCodeKind::from_byte)
        .find(|kind| kind.as_name() == name)
}
pub fn localize_display_message(messages: &[Message], raw: &[u8]) -> Option<String> {
    let raw = raw.split(|byte| *byte == 0).next().unwrap_or(raw);
    let (decoded, _, had_errors) = SHIFT_JIS.decode(raw);
    if had_errors {
        return None;
    }
    let source = decoded.as_ref();
    let exact = |candidate: &str| {
        messages
            .iter()
            .find_map(|message| {
                let pattern = message.source.as_deref()?;
                if pattern.contains('{') || pattern != candidate {
                    return None;
                }
                Some(message.text.clone())
            })
    };
    if let Some(text) = exact(source) {
        return Some(text);
    }
    let trimmed = source.trim();
    if trimmed != source {
        if let Some(text) = exact(trimmed) {
            return Some(text);
        }
    }
    let templates = messages
        .iter()
        .filter(|message| {
            message.source.as_deref().is_some_and(|pattern| pattern.contains('{'))
        });
    for message in templates {
        let pattern = message.source.as_deref()?;
        let mut captures = match_template(pattern, source)?;
        for capture in captures.values_mut() {
            if let Some(translated) = exact(capture)
                .or_else(|| {
                    let trimmed = capture.trim();
                    (trimmed != capture.as_str())
                        .then(|| trimmed.to_owned())
                        .and_then(|t| exact(&t))
                })
            {
                *capture = translated;
            }
        }
        if let Some(text) = substitute(&message.text, &captures) {
            return Some(text);
        }
    }
    None
}
pub(crate) fn localize_trimmed_replay_message(
    messages: &[Message],
    raw: &[u8],
) -> Option<String> {
    let raw = raw.split(|byte| *byte == 0).next().unwrap_or(raw);
    let (decoded, _, had_errors) = SHIFT_JIS.decode(raw);
    if had_errors {
        return None;
    }
    let candidate = decoded.trim();
    messages
        .iter()
        .find_map(|message| {
            let source = message.source.as_deref()?;
            let trimmed = source.trim();
            (trimmed != source && !trimmed.contains('{') && trimmed == candidate)
                .then(|| message.text.trim().to_owned())
        })
}
fn match_template(pattern: &str, actual: &str) -> Option<HashMap<String, String>> {
    let mut captures = HashMap::new();
    let mut pattern_cursor = 0;
    let mut actual_cursor = 0;
    while let Some(open_rel) = pattern[pattern_cursor..].find('{') {
        let open = pattern_cursor + open_rel;
        let close = pattern[open + 1..].find('}')? + open + 1;
        let literal = &pattern[pattern_cursor..open];
        if !actual[actual_cursor..].starts_with(literal) {
            return None;
        }
        actual_cursor += literal.len();
        let name = pattern[open + 1..close].trim();
        if name.is_empty() {
            return None;
        }
        pattern_cursor = close + 1;
        let next_open = pattern[pattern_cursor..]
            .find('{')
            .map(|offset| pattern_cursor + offset)
            .unwrap_or(pattern.len());
        let next_literal = &pattern[pattern_cursor..next_open];
        let capture_end = if next_literal.is_empty() {
            actual.len()
        } else {
            actual[actual_cursor..].find(next_literal)? + actual_cursor
        };
        captures.insert(name.to_owned(), actual[actual_cursor..capture_end].to_owned());
        actual_cursor = capture_end;
    }
    let tail = &pattern[pattern_cursor..];
    (actual[actual_cursor..] == *tail).then_some(captures)
}
fn substitute(target: &str, captures: &HashMap<String, String>) -> Option<String> {
    let mut output = String::new();
    let mut cursor = 0;
    while let Some(open_rel) = target[cursor..].find('{') {
        let open = cursor + open_rel;
        let close = target[open + 1..].find('}')? + open + 1;
        output.push_str(&target[cursor..open]);
        let name = target[open + 1..close].trim();
        output.push_str(captures.get(name)?);
        cursor = close + 1;
    }
    output.push_str(&target[cursor..]);
    Some(output)
}

