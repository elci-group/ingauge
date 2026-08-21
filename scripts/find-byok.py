#!/usr/bin/env python3
"""Discover BYOK LLM usage in local projects using chakra + heuristics.

Usage:
    python3 scripts/find-byok.py ~/goglz ~/litecode ...
    python3 scripts/find-byok.py --home /home/sal --output byok-report.json

Output is a JSON object mapping project paths to lists of evidence entries.
Each evidence entry has a type, file, line, detail, and confidence.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Iterable

# External dependency names (crates, npm packages, go modules) that strongly
# suggest LLM API usage.
LLM_DEPENDENCY_PATTERNS = [
    re.compile(r"\bopenai\b", re.I),
    re.compile(r"\banthropic\b", re.I),
    re.compile(r"\bclaude\b", re.I),
    re.compile(r"\bgroq\b", re.I),
    re.compile(r"\bollama\b", re.I),
    re.compile(r"\b@anthropic-ai/sdk\b", re.I),
    re.compile(r"\bopenai-node\b", re.I),
]

# Environment-variable names that indicate a user-supplied LLM API key.
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

# Known LLM provider hostnames in endpoint URLs.
LLM_HOST_PATTERNS = [
    re.compile(r"api\.openai\.com", re.I),
    re.compile(r"api\.anthropic\.com", re.I),
    re.compile(r"api\.groq\.com", re.I),
    re.compile(r"api\.gpt-oss\.com", re.I),
    re.compile(r"generativelanguage\.googleapis\.com", re.I),
    re.compile(r"api\.deepseek\.com", re.I),
    re.compile(r"openrouter\.ai", re.I),
]

# Source file extensions to grep.
SOURCE_EXTENSIONS = {".rs", ".ts", ".js", ".tsx", ".jsx", ".py", ".go", ".java", ".kt"}


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


def run_chakra(project: Path) -> dict | None:
    result = subprocess.run(
        ["chakra", str(project), "--json"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def analyze_chakra(project: Path, flow: dict) -> Iterable[Evidence]:
    nodes = {node.get("id", ""): node for node in flow.get("nodes", [])}
    evidence_ids = set()

    for edge in flow.get("flows", []):
        target_id = edge.get("target", "")
        target = nodes.get(target_id, {})
        if target.get("kind") != "external":
            continue
        name = target.get("name", "")
        for pattern in LLM_DEPENDENCY_PATTERNS:
            if pattern.search(name) and edge.get("id") not in evidence_ids:
                evidence_ids.add(edge.get("id"))
                yield Evidence(
                    type="dependency",
                    file="",
                    line=0,
                    detail=f"{name} via {edge.get('kind', 'import')}",
                    confidence="high",
                )
                break


def grep_project(project: Path) -> Iterable[Evidence]:
    for root, _, files in os.walk(project):
        # Skip dependency and build directories quickly.
        if any(part in {"target", "node_modules", ".git", ".venv", "venv", "__pycache__", ".cache"} for part in Path(root).parts):
            continue
        for filename in files:
            ext = Path(filename).suffix
            if ext not in SOURCE_EXTENSIONS:
                continue
            path = Path(root) / filename
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
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
    return (
        (path / "Cargo.toml").exists()
        or (path / "package.json").exists()
        or (path / "pyproject.toml").exists()
        or (path / "setup.py").exists()
        or (path / "go.mod").exists()
    )


def scan_project(project: Path) -> ProjectReport:
    report = ProjectReport(path=str(project.resolve()))
    if not is_likely_project(project):
        report.errors.append("not a recognised software project")
        return report

    flow = run_chakra(project)
    if flow is not None:
        report.chakra_analyzed = True
        report.evidence.extend(analyze_chakra(project, flow))
    else:
        report.errors.append("chakra analysis failed")

    report.evidence.extend(grep_project(project))

    # Deduplicate by (type, file, line, detail).
    seen = set()
    unique = []
    for ev in report.evidence:
        key = (ev.type, ev.file, ev.line, ev.detail)
        if key not in seen:
            seen.add(key)
            unique.append(ev)
    report.evidence = unique
    return report


def discover_projects(home: Path) -> list[Path]:
    projects = []
    for entry in home.iterdir():
        if entry.is_dir() and not entry.name.startswith("."):
            if is_likely_project(entry):
                projects.append(entry)
    return sorted(projects)


def main() -> int:
    parser = argparse.ArgumentParser(description="Find BYOK LLM usage in projects")
    parser.add_argument("projects", nargs="*", help="Project directories to scan")
    parser.add_argument("--home", type=Path, help="Scan all immediate subdirectories of HOME")
    parser.add_argument("--output", type=Path, default=Path("byok-report.json"), help="Output JSON file")
    args = parser.parse_args()

    if args.home:
        projects = discover_projects(args.home)
    elif args.projects:
        projects = [Path(p) for p in args.projects]
    else:
        parser.print_help()
        return 1

    reports = {}
    for project in projects:
        print(f"Scanning {project} ...", file=sys.stderr)
        report = scan_project(project)
        reports[report.path] = asdict(report)

    output = {
        "generated_at": subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"]).decode().strip(),
        "projects": reports,
    }

    args.output.write_text(json.dumps(output, indent=2))
    print(f"Report written to {args.output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
