// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::{providers::ProviderError, store::StoreError};

mod body;
mod codes;

pub use body::ErrorBody;

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
