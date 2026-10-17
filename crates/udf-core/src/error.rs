use thiserror::Error;

#[derive(Debug, Error)]
pub enum UdfError {
    #[error("zip hatası: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("G/Ç hatası: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML hatası: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("arşivde zorunlu dosya yok: {0}")]
    Missing(&'static str),
    #[error("geçersiz içerik: {0}")]
    Invalid(String),
}
