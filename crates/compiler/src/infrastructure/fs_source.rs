use crate::application::ports::source::SourceProvider;
use std::path::{Path, PathBuf};

/// Production adapter: delegates to the real filesystem.
pub struct FsSourceProvider;

impl SourceProvider for FsSourceProvider {
    fn read_source(&self, path: &Path) -> Result<String, std::io::Error> {
        std::fs::read_to_string(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, std::io::Error> {
        path.canonicalize()
    }
}
