use serde::ser::{Serialize, Serializer};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

macro_rules! impl_from {
    ($variant:ident, $ty:ty) => {
        impl From<$ty> for AppError {
            fn from(value: $ty) -> Self {
                Self::$variant(value.to_string())
            }
        }
    };
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("JSON error: {0}")]
    Json(String),
    #[error("Invalid data: {0}")]
    Invalid(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Command error: {0}")]
    Command(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl_from!(Io, std::io::Error);
impl_from!(Json, serde_json::Error);
impl_from!(Network, reqwest::Error);
impl_from!(Invalid, url::ParseError);
impl_from!(Command, tauri::Error);
impl_from!(Invalid, zip::result::ZipError);
