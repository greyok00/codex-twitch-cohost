use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication error: {0}")]
    Auth(String),
    #[error("twitch error: {0}")]
    Twitch(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("search error: {0}")]
    Search(String),
    #[error("voice error: {0}")]
    Voice(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("secret-store error: {0}")]
    SecretStore(String),
    #[error("url error: {0}")]
    Url(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        Self::Network(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<sled::Error> for AppError {
    fn from(value: sled::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for AppError {
    fn from(value: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Twitch(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Internal(value.to_string())
    }
}
