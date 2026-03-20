use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Analysis error: {0}")]
    Analysis(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        let (kind, message) = match self {
            AppError::Database(msg) => ("database", msg.as_str()),
            AppError::Parse(msg) => ("parse", msg.as_str()),
            AppError::Io(msg) => ("io", msg.as_str()),
            AppError::Analysis(msg) => ("analysis", msg.as_str()),
            AppError::Network(msg) => ("network", msg.as_str()),
            AppError::NotFound(msg) => ("not_found", msg.as_str()),
        };
        state.serialize_field("kind", kind)?;
        state.serialize_field("message", message)?;
        state.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<csv::Error> for AppError {
    fn from(err: csv::Error) -> Self {
        AppError::Parse(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Parse(err.to_string())
    }
}
