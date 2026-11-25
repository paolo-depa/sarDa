//! Test support shared by the unit tests (`mod testutil` in `lib.rs`) and the
//! integration tests (each includes this file with `#[path]`).

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A fresh, empty directory under the system temp dir, removed again when it
/// is dropped (also when a test fails), so test runs leave nothing behind.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("sarda-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
