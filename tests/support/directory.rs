use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub struct TestDirectory {
    root: PathBuf,
    path: PathBuf,
}
impl TestDirectory {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = root.join(format!(
            "rust-port-fixture-{}-{stamp}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self { root, path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        if self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("rust-port-fixture-"))
            && self.path.canonicalize().is_ok_and(|path| path == self.path)
            && let Err(error) = std::fs::remove_dir_all(&self.path)
        {
            eprintln!("fixture cleanup failed: {error}");
        }
    }
}
