use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}: JSON 형식 오류: {source}")]
    Json { path: PathBuf, source: serde_json::Error },
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

impl CoreError {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io { path: path.to_path_buf(), source }
    }

    pub fn json(path: &Path, source: serde_json::Error) -> Self {
        Self::Json { path: path.to_path_buf(), source }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
