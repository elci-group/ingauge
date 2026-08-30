// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}
