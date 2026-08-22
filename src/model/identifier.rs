// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.is_empty() || value.len() > 128 {
                    tracing::warn!(
                        event = "identifier_rejected",
                        identifier_type = stringify!($name),
                        reason = "length",
                        "identifier validation failed"
                    );
                    return Err("identifier length must be between 1 and 128 bytes".into());
                }
                if !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
                {
                    tracing::warn!(
                        event = "identifier_rejected",
                        identifier_type = stringify!($name),
                        reason = "characters",
                        "identifier validation failed"
                    );
                    return Err("identifier contains unsupported characters".into());
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(ProviderId);
identifier!(ModelId);
