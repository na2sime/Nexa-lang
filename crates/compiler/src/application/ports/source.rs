use std::path::{Path, PathBuf};

/// Port: abstract file-system operations used by the Resolver.
///
/// Production code uses `FsSourceProvider`; tests use `MemSourceProvider`.
pub trait SourceProvider: Send + Sync {
    fn read_source(&self, path: &Path) -> Result<String, std::io::Error>;
    fn exists(&self, path: &Path) -> bool;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, std::io::Error>;
}
