#!/usr/bin/env python3
import copy
from curly_expand import expand_or_literal, cartesian
# Copyright (c) 2026 sal
# SPDX-License-Identifier: MIT
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
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path
from typing import Iterable

from byok_core import (
    LLM_DEPENDENCY_PATTERNS,
    Evidence,
    ProjectReport,
    discover_projects,
    grep_project,
    is_likely_project,
)


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


def main() -> int:
    parser = argparse.ArgumentParser(description="Find BYOK LLM usage in projects")
    parser.add_argument("projects", nargs="*", help="Project directories to scan")
    parser.add_argument("--home", type=Path, help="Scan all immediate subdirectories of HOME")
    parser.add_argument("--output", type=Path, default=Path("byok-report.json"), help="Output JSON file")
    args = parser.parse_args()

    __curly_projects = cartesian([expand_or_literal(str(t)) for t in (args.projects or [''])])
    for __curly_v_projects in __curly_projects:
        args = copy.copy(args)
        args.projects = __curly_v_projects
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
