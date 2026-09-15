use engine::storage::SaveStore;
use std::cell::RefCell;
use std::collections::BTreeMap;
struct Entry {
    data: Vec<u8>,
    modified: web_time::SystemTime,
}
pub(crate) struct WebSaveStore {
    files: RefCell<BTreeMap<String, Entry>>,
    persist: js_sys::Function,
    persist_this: wasm_bindgen::JsValue,
    imported_mss: RefCell<std::collections::HashSet<String>>,
}
unsafe impl Send for WebSaveStore {}
unsafe impl Sync for WebSaveStore {}
impl WebSaveStore {
    pub(crate) fn new() -> Self {
        let global = js_sys::global();
        let persist = js_sys::Reflect::get(&global, &"__savePersist".into())
            .ok()
            .filter(|value| value.is_function())
            .unwrap_or_else(wasm_bindgen::JsValue::undefined);
        Self {
            files: RefCell::new(BTreeMap::new()),
            imported_mss: RefCell::new(std::collections::HashSet::new()),
            persist_this: global.into(),
            persist: js_sys::Function::from(persist),
        }
    }
    pub(crate) fn add_file(&self, name: String, data: Vec<u8>) {
        self.files
            .borrow_mut()
            .insert(
                name,
                Entry {
                    data,
                    modified: web_time::SystemTime::now(),
                },
            );
    }
    pub(crate) fn is_save_name(name: &str) -> bool {
        let lower = name.to_lowercase();
        let bare = lower.rsplit('/').next().unwrap_or(&lower);
        (bare.starts_with("majiro_") && bare.ends_with(".sav"))
            || bare == "majiro_system.mss" || bare == "majiro_readmark.mss"
            || bare.ends_with("_config.dat")
    }
    pub(crate) fn export_files(&self) -> js_sys::Array {
        let files = self.files.borrow();
        let array = js_sys::Array::new();
        for (name, entry) in files.iter() {
            if !Self::is_save_name(name) {
                continue;
            }
            let pair = js_sys::Array::new();
            pair.push(&wasm_bindgen::JsValue::from_str(name));
            pair.push(&js_sys::Uint8Array::from(entry.data.as_slice()));
            array.push(&pair);
        }
        array
    }
    pub(crate) fn import_file(&self, name: &str, data: Vec<u8>) {
        let _ = self.write(name, &data);
        if name == "majiro_system.mss" || name == "majiro_readmark.mss" {
            self.imported_mss.borrow_mut().insert(name.to_string());
        }
    }
    pub(crate) fn has_imported_mss(&self) -> bool {
        !self.imported_mss.borrow().is_empty()
    }
}
impl SaveStore for WebSaveStore {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.files.borrow().get(name).map(|entry| entry.data.clone()))
    }
    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        if name == "majiro_system.mss" || name == "majiro_readmark.mss" {
            self.imported_mss.borrow_mut().remove(name);
        }
        self.files
            .borrow_mut()
            .insert(
                name.to_string(),
                Entry {
                    data: data.to_vec(),
                    modified: web_time::SystemTime::now(),
                },
            );
        self.forward(name, Some(data));
        Ok(())
    }
    fn delete(&self, name: &str) {
        if name == "majiro_system.mss" || name == "majiro_readmark.mss" {
            self.imported_mss.borrow_mut().remove(name);
        }
        if self.files.borrow_mut().remove(name).is_some() {
            self.forward(name, None);
        }
    }
    fn copy(&self, src: &str, dst: &str) -> Result<(), String> {
        let data = self.files.borrow().get(src).map(|entry| entry.data.clone());
        match data {
            Some(data) => self.write(dst, &data),
            None => Ok(()),
        }
    }
    fn last_modified(&self, name: &str) -> Option<web_time::SystemTime> {
        self.files.borrow().get(name).map(|entry| entry.modified)
    }
}
impl WebSaveStore {
    fn forward(&self, name: &str, data: Option<&[u8]>) {
        let argument = match data {
            Some(bytes) => js_sys::Uint8Array::from(bytes).into(),
            None => wasm_bindgen::JsValue::NULL,
        };
        let _ = self
            .persist
            .call2(
                &self.persist_this,
                &wasm_bindgen::JsValue::from_str(name),
                &argument,
            );
    }
}
