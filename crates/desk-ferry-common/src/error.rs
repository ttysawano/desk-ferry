use thiserror::Error;

pub type Result<T> = std::result::Result<T, DeskFerryError>;

#[derive(Debug, Error)]
pub enum DeskFerryError {
    #[error("config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("config validation error: {0}")]
    ConfigValidation(String),
    #[error("json parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolVersionMismatch { expected: u16, actual: u16 },
    #[error("invalid json lines frame")]
    InvalidJsonLine,
    #[error("invalid neighbor specification: {0}")]
    InvalidNeighborSpec(String),
}
