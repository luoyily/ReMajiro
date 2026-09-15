use super::*;
impl EngineHost {
    pub(super) fn file_exists_impl(&mut self, name: &[u8]) -> bool {
        resource_candidates(name)
            .into_iter()
            .any(|candidate| self.vfs.exists(&candidate))
    }
    pub(super) fn file_open_impl(&mut self, name: &[u8]) -> u32 {
        let mut name_str = sjis_to_string(name);
        if name_str.is_empty() {
            name_str = "sysdata.cfg".to_string();
        }
        if let Some(data) = self
            .patch
            .as_ref()
            .and_then(|patch| patch.file_override(&name_str))
        {
            let handle = self.alloc_handle();
            eprintln!(
                "[PATCH] file override {:?} → handle {} ({} bytes)", name_str, handle,
                data.len()
            );
            self.text_files
                .insert(
                    handle,
                    TextFile {
                        data,
                        cursor: 0,
                        pending_tail: None,
                    },
                );
            return handle;
        }
        let mut located = self.vfs.find(&name_str).map(|data| (name_str.clone(), data));
        if located.is_none() {
            for candidate in resource_candidates(name) {
                if candidate == name_str {
                    continue;
                }
                if let Some(data) = self.vfs.find(&candidate) {
                    located = Some((candidate, data));
                    break;
                }
            }
        }
        match located {
            Some((resolved, data)) => {
                let handle = self.alloc_handle();
                eprintln!(
                    "[FILE] opened {:?} → handle {} ({} bytes)", resolved, handle, data
                    .len()
                );
                self.text_files
                    .insert(
                        handle,
                        TextFile {
                            data,
                            cursor: 0,
                            pending_tail: None,
                        },
                    );
                handle
            }
            None => {
                eprintln!("[FILE] not found {:?}", name_str);
                0
            }
        }
    }
    pub(super) fn file_readline_impl(&mut self, handle: u32) -> Option<Vec<u8>> {
        let file = self.text_files.get_mut(&handle)?;
        loop {
            let line = match file.pending_tail.take() {
                Some(tail) => tail,
                None => read_next_line(&file.data, &mut file.cursor)?,
            };
            let token = parse_line_token(&line);
            if token.head.is_empty() && token.tail.is_none() {
                continue;
            }
            file.pending_tail = token.tail;
            return Some(token.head);
        }
    }
    pub(super) fn file_readline_raw_impl(&mut self, handle: u32) -> Option<Vec<u8>> {
        let file = self.text_files.get_mut(&handle)?;
        loop {
            let line = match file.pending_tail.take() {
                Some(tail) => tail,
                None => read_next_line(&file.data, &mut file.cursor)?,
            };
            let line = parse_raw_line(&line);
            if !line.is_empty() {
                return Some(line);
            }
        }
    }
    pub(super) fn file_close_impl(&mut self, handle: u32) {
        if self.text_files.remove(&handle).is_some() {
            eprintln!("[FILE] closed handle {}", handle);
        }
    }
}
