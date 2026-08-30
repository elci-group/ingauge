// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::{AppError, ErrorBody};

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
