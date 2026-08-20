use crate::{providers::ProviderError, store::StoreError};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Configuration(String),
    #[error("{0}")]
    Provider(#[from] ProviderError),
    #[error("{0}")]
    Storage(#[from] StoreError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration_error",
            Self::Provider(_) => "provider_error",
            Self::Storage(_) => "storage_error",
            Self::Io(_) => "io_error",
            Self::Serialization(_) => "serialization_error",
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Configuration(_) => 3,
            Self::Provider(_) => 4,
            Self::Storage(_) | Self::Io(_) | Self::Serialization(_) => 5,
        }
    }

    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_stable_codes_bodies_and_exit_statuses() {
        let configuration = AppError::Configuration("bad input".into());
        assert_eq!(configuration.code(), "configuration_error");
        assert_eq!(configuration.exit_code(), 3);
        assert_eq!(configuration.body().message, "bad input");

        let timeout = AppError::Provider(ProviderError::Timeout);
        assert_eq!(timeout.code(), "provider_error");
        assert_eq!(timeout.exit_code(), 4);

        let io = AppError::Io(std::io::Error::other("disk"));
        assert_eq!(io.code(), "io_error");
        assert_eq!(io.exit_code(), 5);
    }
}
