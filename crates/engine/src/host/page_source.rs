use super::pixel_ops::replace_red_colorkey;
use super::text_files::{resource_candidates, sjis_to_string};
use crate::patch::PatchBundle;
use crate::render_model::PresentationImage;
use crate::vfs::Vfs;
use formats::image::parse_majiro_image_with_key;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct CachedPageTemplate {
    pub(crate) resolved_name: String,
    pub(crate) page: super::Page,
}
const PAGE_TEMPLATE_CACHE_MAX_ENTRIES: usize = 64;
const PAGE_TEMPLATE_CACHE_BUDGET_BYTES: u64 = 768 * 1024 * 1024;
const ALPHA_COMPOSITE_CACHE_MAX_ENTRIES: usize = 128;
const ALPHA_COMPOSITE_CACHE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;
fn cached_page_template_bytes(template: &CachedPageTemplate) -> u64 {
    let mut seen = HashSet::new();
    let mut bytes = 0u64;
    for pixels in [&template.page.pixels, &template.page.presentation_seed] {
        let identity = Arc::as_ptr(pixels) as usize;
        if seen.insert(identity) {
            bytes = bytes.saturating_add(pixels.len() as u64);
        }
    }
    if let Some(presentation) = &template.page.presentation {
        let identity = Arc::as_ptr(&presentation.pixels) as usize;
        if seen.insert(identity) {
            bytes = bytes.saturating_add(presentation.pixels.len() as u64);
        }
    }
    if let Some(samples) = &template.page.indexed_samples {
        let identity = Arc::as_ptr(samples) as usize;
        if seen.insert(identity) {
            bytes = bytes.saturating_add(samples.len() as u64);
        }
    }
    if let Some(palette) = &template.page.indexed_palette {
        bytes = bytes
            .saturating_add((palette.len() * std::mem::size_of::<[u8; 4]>()) as u64);
    }
    bytes
}
pub(crate) struct PageSource {
    pub(crate) page_cache: HashMap<String, Arc<CachedPageTemplate>>,
    page_cache_lru: VecDeque<usize>,
    pub(crate) alpha_composite_cache: HashMap<String, Arc<Vec<u8>>>,
    alpha_composite_cache_lru: VecDeque<String>,
}
impl PageSource {
    pub(crate) fn new() -> Self {
        Self {
            page_cache: HashMap::new(),
            page_cache_lru: VecDeque::new(),
            alpha_composite_cache: HashMap::new(),
            alpha_composite_cache_lru: VecDeque::new(),
        }
    }
    pub(crate) fn contains_template(&self, name: &str) -> bool {
        self.page_cache.contains_key(name)
    }
    pub(crate) fn cached_presentation(
        &self,
        resource_name: &str,
    ) -> Option<Arc<PresentationImage>> {
        self.page_cache
            .get(resource_name)
            .and_then(|template| template.page.presentation.clone())
    }
    pub(crate) fn touch_template(&mut self, template: &Arc<CachedPageTemplate>) {
        let identity = Arc::as_ptr(template) as usize;
        self.page_cache_lru.retain(|cached_identity| *cached_identity != identity);
        self.page_cache_lru.push_back(identity);
    }
    pub(crate) fn trim_template_cache(&mut self) {
        let mut unique = HashMap::<usize, u64>::new();
        for template in self.page_cache.values() {
            unique
                .entry(Arc::as_ptr(template) as usize)
                .or_insert_with(|| cached_page_template_bytes(template));
        }
        let mut resident_bytes = unique.values().copied().sum::<u64>();
        while unique.len() > PAGE_TEMPLATE_CACHE_MAX_ENTRIES
            || resident_bytes > PAGE_TEMPLATE_CACHE_BUDGET_BYTES
        {
            let Some(oldest) = self.page_cache_lru.pop_front() else {
                break;
            };
            let Some(bytes) = unique.remove(&oldest) else {
                continue;
            };
            self.page_cache
                .retain(|_, template| Arc::as_ptr(template) as usize != oldest);
            resident_bytes = resident_bytes.saturating_sub(bytes);
        }
    }
    pub(crate) fn alpha_composite_get(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        let pixels = self.alpha_composite_cache.get(key).cloned()?;
        self.alpha_composite_cache_lru.retain(|cached_key| cached_key != key);
        self.alpha_composite_cache_lru.push_back(key.to_owned());
        Some(pixels)
    }
    pub(crate) fn alpha_composite_insert(&mut self, key: String, pixels: Arc<Vec<u8>>) {
        self.alpha_composite_cache.insert(key.clone(), pixels);
        self.alpha_composite_cache_lru.retain(|cached_key| cached_key != &key);
        self.alpha_composite_cache_lru.push_back(key);
        let mut resident_bytes = self
            .alpha_composite_cache
            .values()
            .map(|pixels| pixels.len() as u64)
            .sum::<u64>();
        while self.alpha_composite_cache.len() > ALPHA_COMPOSITE_CACHE_MAX_ENTRIES
            || resident_bytes > ALPHA_COMPOSITE_CACHE_BUDGET_BYTES
        {
            let Some(oldest) = self.alpha_composite_cache_lru.pop_front() else {
                break;
            };
            if let Some(pixels) = self.alpha_composite_cache.remove(&oldest) {
                resident_bytes = resident_bytes.saturating_sub(pixels.len() as u64);
            }
        }
    }
    pub(crate) fn cached_template(
        &mut self,
        vfs: &Vfs,
        rct_key: &[u8; 1024],
        patch: Option<&PatchBundle>,
        filename: &[u8],
    ) -> Option<Arc<CachedPageTemplate>> {
        let name = sjis_to_string(filename);
        let name_lower = name.to_lowercase();
        if let Some(template) = self.page_cache.get(&name_lower).cloned() {
            self.touch_template(&template);
            return Some(template);
        }
        let template = Arc::new(decode_page_template(vfs, rct_key, patch, filename)?);
        self.page_cache.insert(name_lower, Arc::clone(&template));
        self.page_cache
            .insert(template.resolved_name.to_lowercase(), Arc::clone(&template));
        self.touch_template(&template);
        self.trim_template_cache();
        Some(template)
    }
}
pub(crate) fn decode_page_template(
    vfs: &Vfs,
    rct_key: &[u8; 1024],
    patch: Option<&PatchBundle>,
    filename: &[u8],
) -> Option<CachedPageTemplate> {
    let name = sjis_to_string(filename);
    let template_start = crate::Instant::now();
    eprintln!("[IMAGE-ACCESS] {:?}", name);
    let mut img = None;
    let mut last_error = None;
    let mut resolved_name: Option<String> = None;
    for candidate in resource_candidates(filename) {
        let Some(data) = vfs.find(&candidate) else {
            continue;
        };
        match parse_majiro_image_with_key(&data, Some(&candidate), rct_key) {
            Ok(parsed) => {
                resolved_name = Some(candidate);
                img = Some(parsed);
                break;
            }
            Err(e) => {
                eprintln!(
                    "[GFX] candidate {:?} failed to decode: {} (trying next extension)",
                    candidate, e
                );
                last_error = Some(e);
            }
        }
    }
    let Some(resolved_name) = resolved_name else {
        match last_error {
            Some(error) => eprintln!("[GFX] failed to decode {:?}: {}", name, error),
            None => eprintln!("[GFX] page file not found: {:?}", name),
        }
        return None;
    };
    let mut img = img?;
    if let Some(class_name) = img
        .class_name
        .as_deref()
        .map(|s| s.trim_end_matches('\0').trim().to_owned())
        .filter(|s| !s.is_empty())
    {
        match vfs.find(&class_name) {
            None => {
                eprintln!(
                    "[IMAGE-CLASS] failed image={:?} class={:?} reason=source_not_found",
                    resolved_name, class_name
                )
            }
            Some(source_data) => {
                match parse_majiro_image_with_key(
                    &source_data,
                    Some(&class_name),
                    rct_key,
                ) {
                    Err(error) => {
                        eprintln!(
                            "[IMAGE-CLASS] failed image={:?} class={:?} reason=decode_error error={}",
                            resolved_name, class_name, error
                        )
                    }
                    Ok(
                        source,
                    ) if source.width != img.width || source.height != img.height => {
                        eprintln!(
                            "[IMAGE-CLASS] failed image={:?} class={:?} reason=size_mismatch target={}x{} source={}x{}",
                            resolved_name, class_name, img.width, img.height, source
                            .width, source.height
                        );
                    }
                    Ok(
                        source,
                    ) if source
                        .class_name
                        .as_deref()
                        .is_some_and(|name| !name.is_empty()) => {
                        eprintln!(
                            "[IMAGE-CLASS] failed image={:?} class={:?} reason=nested_class_not_recovered",
                            resolved_name, class_name
                        );
                    }
                    Ok(source) => {
                        let replaced = replace_red_colorkey(
                            &mut img.pixels,
                            &source.pixels,
                        );
                        eprintln!(
                            "[IMAGE-CLASS] applied image={:?} class={:?} replaced={}",
                            resolved_name, class_name, replaced
                        );
                    }
                }
            }
        }
    }
    let presentation = patch
        .and_then(|patch| {
            match patch.presentation_image(&resolved_name, img.width, img.height) {
                Ok(Some(image)) => {
                    eprintln!(
                        "[PATCH] using presentation image {:?} ({}x{} for native {}x{})",
                        resolved_name, image.width, image.height, img.width, img.height
                    );
                    Some(image)
                }
                Ok(None) => None,
                Err(error) => {
                    eprintln!("[PATCH] {error}; using the native image");
                    None
                }
            }
        });
    let pixels = Arc::new(img.pixels);
    let template = CachedPageTemplate {
        resolved_name: resolved_name.clone(),
        page: super::Page {
            width: img.width,
            height: img.height,
            pixels: pixels.clone(),
            presentation_seed: pixels,
            presentation,
            indexed_samples: img.indexed_pixels.take().map(Arc::new),
            indexed_palette: img.indexed_palette.take().map(Arc::new),
            revision: 0,
            alpha_masked: false,
            draw_mode: 0,
        },
    };
    vm::text_trace!(
        "[PAGE] template {:?} as {:?} ({}x{}) presentation={}", name, resolved_name,
        template.page.width, template.page.height, template.page.presentation.is_some()
    );
    if vm::diag_log_enabled() && template_start.elapsed().as_millis() >= 100 {
        std::eprintln!(
            "[TIMING] page decode {:?} ({}x{}) took {:?}", name, template.page.width,
            template.page.height, template_start.elapsed()
        );
    }
    eprintln!(
        "[GFX] cached page {:?} as {:?} ({}x{})", name, resolved_name, img.width, img
        .height
    );
    Some(template)
}
