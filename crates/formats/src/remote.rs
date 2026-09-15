pub trait RemoteReader: Send + Sync {
    fn read_at(&self, name: &str, offset: u64, size: u32) -> Result<Vec<u8>, String>;
    fn size_of(&self, name: &str) -> Result<u64, String>;
}

