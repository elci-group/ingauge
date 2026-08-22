# Copyright (c) 2026 sal
# SPDX-License-Identifier: MIT
"""Project discovery and source scanning primitives for find-byok."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

LLM_DEPENDENCY_PATTERNS = [
    re.compile(r"\bopenai\b", re.I),
    re.compile(r"\banthropic\b", re.I),
    re.compile(r"\bclaude\b", re.I),
    re.compile(r"\bgroq\b", re.I),
    re.compile(r"\bollama\b", re.I),
    re.compile(r"\b@anthropic-ai/sdk\b", re.I),
    re.compile(r"\bopenai-node\b", re.I),
]

API_KEY_PATTERNS = [
    re.compile(r"\bOPENAI_API_KEY\b"),
    re.compile(r"\bOPENAI_KEY\b"),
    re.compile(r"\bANTHROPIC_API_KEY\b"),
    re.compile(r"\bCLAUDE_API_KEY\b"),
    re.compile(r"\bGROQ_API_KEY\b"),
    re.compile(r"\bGPT_OSS_API_KEY\b"),
    re.compile(r"\bGEMINI_API_KEY\b"),
    re.compile(r"\bGOOGLE_API_KEY\b"),
    re.compile(r"\bDEEPSEEK_API_KEY\b"),
]

LLM_HOST_PATTERNS = [
    re.compile(r"api\.openai\.com", re.I),
    re.compile(r"api\.anthropic\.com", re.I),
    re.compile(r"api\.groq\.com", re.I),
    re.compile(r"api\.gpt-oss\.com", re.I),
    re.compile(r"generativelanguage\.googleapis\.com", re.I),
    re.compile(r"api\.deepseek\.com", re.I),
    re.compile(r"openrouter\.ai", re.I),
]

SOURCE_EXTENSIONS = {".rs", ".ts", ".js", ".tsx", ".jsx", ".py", ".go", ".java", ".kt"}
SKIPPED_DIRECTORIES = {
    "target",
    "node_modules",
    ".git",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
}


@dataclass
class Evidence:
    type: str
    file: str
    line: int
    detail: str
    confidence: str


@dataclass
class ProjectReport:
    path: str
    chakra_analyzed: bool = False
    evidence: list[Evidence] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)


def grep_project(project: Path) -> Iterable[Evidence]:
    for root, _, files in os.walk(project):
        if any(part in SKIPPED_DIRECTORIES for part in Path(root).parts):
            continue
        for filename in files:
            path = Path(root) / filename
            if path.suffix not in SOURCE_EXTENSIONS:
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            yield from evidence_in_text(project, path, text)


def evidence_in_text(project: Path, path: Path, text: str) -> Iterable[Evidence]:
    for lineno, raw_line in enumerate(text.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        for pattern in API_KEY_PATTERNS:
            if pattern.search(line):
                yield Evidence(
                    type="api_key_env",
                    file=str(path.relative_to(project)),
                    line=lineno,
                    detail=line[:160],
                    confidence="high",
                )
        for pattern in LLM_HOST_PATTERNS:
            if pattern.search(line):
                yield Evidence(
                    type="llm_endpoint",
                    file=str(path.relative_to(project)),
                    line=lineno,
                    detail=line[:160],
                    confidence="medium",
                )


def is_likely_project(path: Path) -> bool:
    markers = ("Cargo.toml", "package.json", "pyproject.toml", "setup.py", "go.mod")
    return any((path / marker).exists() for marker in markers)


def discover_projects(home: Path) -> list[Path]:
    return sorted(
        entry
        for entry in home.iterdir()
        if entry.is_dir() and not entry.name.startswith(".") and is_likely_project(entry)
    )
