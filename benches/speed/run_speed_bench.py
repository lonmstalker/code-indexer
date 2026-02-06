#!/usr/bin/env python3
"""Honest speed benchmark runner for code-indexer vs rg.

Methodology:
- definition-like cases only
- exact count parity precheck (code-indexer vs rg)
- query-only mode: one index build, then query timings
- first-run mode: (rm db + index + query) for code-indexer, query-only for rg
"""

from __future__ import annotations

import argparse
import json
import math
import platform
import re
import shlex
import socket
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    script_dir = Path(__file__).resolve().parent
    project_root = script_dir.parent.parent

    parser = argparse.ArgumentParser(description="Run honest speed benchmark for code-indexer vs rg")
    parser.add_argument(
        "--repos",
        default="all",
        help="Comma-separated repo names from cases.json, or 'all' (default: all)",
    )
    parser.add_argument(
        "--mode",
        choices=("query-only", "first-run", "both"),
        default="both",
        help="Benchmark mode (default: both)",
    )
    parser.add_argument("--runs", type=int, default=10, help="Measured runs per side (default: 10)")
    parser.add_argument("--warmup", type=int, default=3, help="Warmup runs per side (default: 3)")
    parser.add_argument(
        "--cases",
        default=str(script_dir / "cases.json"),
        help="Path to cases.json",
    )
    parser.add_argument(
        "--repos-dir",
        default=str(project_root / "benches" / "repos"),
        help="Path to benchmark repositories",
    )
    parser.add_argument(
        "--binary",
        default=str(project_root / "target" / "release" / "code-indexer"),
        help="Path to code-indexer binary",
    )
    parser.add_argument(
        "--out-json",
        default=str(project_root / "benches" / "results" / "speed" / "latest.json"),
        help="Output JSON path",
    )
    parser.add_argument(
        "--out-md",
        default=str(project_root / "benches" / "results" / "speed" / "latest.md"),
        help="Output Markdown path",
    )
    parser.add_argument(
        "--require-valid",
        action="store_true",
        help="Exit non-zero if any selected repo has zero valid cases",
    )
    return parser.parse_args()


def run_checked(
    cmd: list[str],
    *,
    allowed_returncodes: tuple[int, ...] = (0,),
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        cmd,
        text=True,
        capture_output=capture_output,
    )
    if completed.returncode not in allowed_returncodes:
        joined = " ".join(shlex.quote(part) for part in cmd)
        raise RuntimeError(
            f"Command failed: {joined}\n"
            f"returncode={completed.returncode}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def run_quiet(cmd: list[str], *, allowed_returncodes: tuple[int, ...] = (0,)) -> None:
    completed = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if completed.returncode not in allowed_returncodes:
        joined = " ".join(shlex.quote(part) for part in cmd)
        raise RuntimeError(f"Command failed: {joined} (returncode={completed.returncode})")


def percentile(values: list[float], p: float) -> float:
    if not values:
        raise ValueError("Percentile requested for empty list")
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * (p / 100.0)
    lower = int(math.floor(rank))
    upper = int(math.ceil(rank))
    if lower == upper:
        return ordered[lower]
    weight = rank - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * weight


def summarize_samples(samples: list[float]) -> dict[str, Any]:
    if not samples:
        return {
            "samples_ms": [],
            "median_ms": None,
            "p95_ms": None,
            "cv_pct": None,
        }

    mean_s = statistics.mean(samples)
    stddev_s = statistics.pstdev(samples) if len(samples) > 1 else 0.0
    cv_pct = (stddev_s / mean_s * 100.0) if mean_s > 0 else 0.0

    return {
        "samples_ms": [round(v * 1000.0, 6) for v in samples],
        "median_ms": round(statistics.median(samples) * 1000.0, 6),
        "p95_ms": round(percentile(samples, 95.0) * 1000.0, 6),
        "cv_pct": round(cv_pct, 6),
    }


def measure_callable(fn, *, warmup: int, runs: int) -> list[float]:
    for _ in range(warmup):
        fn()

    measured: list[float] = []
    for _ in range(runs):
        t0 = time.perf_counter()
        fn()
        measured.append(time.perf_counter() - t0)
    return measured


def parse_repos(raw: str) -> list[str] | None:
    if raw.strip().lower() == "all":
        return None
    repos = [part.strip() for part in raw.split(",") if part.strip()]
    if not repos:
        raise ValueError("--repos must be 'all' or a comma-separated list")
    return repos


def load_cases(path: Path, selected_repos: list[str] | None) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    if not path.exists():
        raise FileNotFoundError(f"cases file not found: {path}")
    payload = json.loads(path.read_text(encoding="utf-8"))

    if "cases" not in payload or not isinstance(payload["cases"], list):
        raise ValueError("cases.json must contain an array field 'cases'")

    cases: list[dict[str, Any]] = []
    for case in payload["cases"]:
        required = ("id", "repo", "symbol", "rg_pattern")
        missing = [key for key in required if key not in case]
        if missing:
            raise ValueError(f"Case is missing required keys {missing}: {case}")
        if selected_repos is not None and case["repo"] not in selected_repos:
            continue
        cases.append(case)

    if not cases:
        raise ValueError("No cases selected. Check --repos and cases.json")

    return payload, cases


def code_indexer_definition_command(binary: Path, db_path: Path, symbol: str) -> list[str]:
    return [str(binary), "--db", str(db_path), "definition", symbol]


def rg_definition_command(repo_path: Path, rg_pattern: str, rg_glob: str | None) -> list[str]:
    cmd = ["rg", "--line-number", "--no-heading", rg_pattern]
    if rg_glob:
        cmd.extend(["--glob", rg_glob])
    cmd.append(str(repo_path))
    return cmd


def count_code_indexer_definitions(binary: Path, db_path: Path, symbol: str) -> tuple[int, list[str]]:
    cmd = code_indexer_definition_command(binary, db_path, symbol)
    completed = run_checked(cmd)
    pattern = re.compile(rf"^{re.escape(symbol)}\s+\(")
    count = sum(1 for line in completed.stdout.splitlines() if pattern.match(line))
    return count, cmd


def count_rg_matches(repo_path: Path, rg_pattern: str, rg_glob: str | None) -> tuple[int, list[str]]:
    cmd = rg_definition_command(repo_path, rg_pattern, rg_glob)
    completed = run_checked(cmd, allowed_returncodes=(0, 1))
    if completed.returncode == 1:
        return 0, cmd
    count = sum(1 for line in completed.stdout.splitlines() if line.strip())
    return count, cmd


def ensure_index(binary: Path, db_path: Path, repo_path: Path) -> None:
    if db_path.exists():
        db_path.unlink()
    run_checked([str(binary), "--db", str(db_path), "index", str(repo_path)], capture_output=True)


def format_cmd(cmd: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in cmd)


def build_report(
    *,
    args: argparse.Namespace,
    selected_cases: list[dict[str, Any]],
    results: list[dict[str, Any]],
    project_root: Path,
) -> dict[str, Any]:
    git_commit = "unknown"
    try:
        completed = run_checked(["git", "rev-parse", "HEAD"], capture_output=True)
        git_commit = completed.stdout.strip()
    except Exception:
        git_commit = "unknown"

    by_mode: dict[str, dict[str, Any]] = {}
    for mode in sorted({item["mode"] for item in results}):
        mode_items = [item for item in results if item["mode"] == mode]
        valid_items = [item for item in mode_items if item["validity"]["is_valid"]]
        speedups = [
            item["speedup"]["median_ratio_rg_over_code_indexer"]
            for item in valid_items
            if item["speedup"]["median_ratio_rg_over_code_indexer"] is not None
        ]
        by_mode[mode] = {
            "total_cases": len(mode_items),
            "valid_cases": len(valid_items),
            "invalid_cases": len(mode_items) - len(valid_items),
            "median_speedup_rg_over_code_indexer": round(statistics.median(speedups), 6)
            if speedups
            else None,
        }

    return {
        "meta": {
            "generated_at_utc": datetime.now(timezone.utc).isoformat(),
            "project_root": str(project_root),
            "code_indexer_git_commit": git_commit,
            "mode": args.mode,
            "runs": args.runs,
            "warmup": args.warmup,
            "repos": args.repos,
            "machine": {
                "hostname": socket.gethostname(),
                "platform": platform.platform(),
                "python": platform.python_version(),
            },
            "limitations": {
                "page_cache_flush": "not performed",
                "coldness": "process-cold only, not guaranteed disk-cold",
            },
        },
        "cases": selected_cases,
        "results": results,
        "summary": {
            "by_mode": by_mode,
            "selected_case_count": len(selected_cases),
        },
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines: list[str] = []
    meta = report["meta"]
    lines.append("# Honest Speed Benchmark Report")
    lines.append("")
    lines.append(f"Generated at (UTC): `{meta['generated_at_utc']}`")
    lines.append(f"Git commit: `{meta['code_indexer_git_commit']}`")
    lines.append(f"Mode: `{meta['mode']}`, runs: `{meta['runs']}`, warmup: `{meta['warmup']}`")
    lines.append("")
    lines.append(
        "Ограничение: page-cache flush не выполняется; результаты соответствуют process-cold, а не guaranteed disk-cold benchmark."
    )
    lines.append("")

    lines.append("## Summary")
    lines.append("")
    lines.append("| mode | total | valid | invalid | median speedup (rg/code-indexer) |")
    lines.append("|------|------:|------:|--------:|----------------------------------:|")
    for mode, stats in sorted(report["summary"]["by_mode"].items()):
        speedup = stats["median_speedup_rg_over_code_indexer"]
        speedup_cell = f"{speedup:.3f}x" if isinstance(speedup, (float, int)) else "n/a"
        lines.append(
            f"| {mode} | {stats['total_cases']} | {stats['valid_cases']} | {stats['invalid_cases']} | {speedup_cell} |"
        )

    for mode in sorted({item["mode"] for item in report["results"]}):
        lines.append("")
        lines.append(f"## Details: {mode}")
        lines.append("")
        lines.append("| repo | case | symbol | validity | counts (code/rg) | median ms (code/rg) | p95 ms (code/rg) | cv% (code/rg) | speedup |")
        lines.append("|------|------|--------|----------|------------------|---------------------|------------------|----------------|---------|")
        mode_items = [item for item in report["results"] if item["mode"] == mode]
        for item in mode_items:
            validity = item["validity"]
            if validity["is_valid"]:
                validity_cell = "valid"
            else:
                validity_cell = f"invalid ({validity['reason']})"

            counts = item["counts"]
            counts_cell = f"{counts['code_indexer']}/{counts['rg']}"

            code_stats = item["timings"]["code_indexer"]
            rg_stats = item["timings"]["rg"]

            def pair(metric: str) -> str:
                left = code_stats.get(metric)
                right = rg_stats.get(metric)
                if left is None or right is None:
                    return "n/a"
                return f"{left:.3f}/{right:.3f}"

            speedup = item["speedup"]["median_ratio_rg_over_code_indexer"]
            speedup_cell = f"{speedup:.3f}x" if isinstance(speedup, (float, int)) else "n/a"

            lines.append(
                "| "
                + f"{item['repo']}"
                + " | "
                + f"{item['case_id']}"
                + " | "
                + f"{item['symbol']}"
                + " | "
                + f"{validity_cell}"
                + " | "
                + f"{counts_cell}"
                + " | "
                + f"{pair('median_ms')}"
                + " | "
                + f"{pair('p95_ms')}"
                + " | "
                + f"{pair('cv_pct')}"
                + " | "
                + f"{speedup_cell}"
                + " |"
            )

    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()

    if args.runs <= 0:
        raise ValueError("--runs must be > 0")
    if args.warmup < 0:
        raise ValueError("--warmup must be >= 0")

    selected_repo_names = parse_repos(args.repos)

    cases_payload, selected_cases = load_cases(Path(args.cases), selected_repo_names)
    del cases_payload

    repos_dir = Path(args.repos_dir)
    binary = Path(args.binary)
    project_root = Path(__file__).resolve().parent.parent.parent

    if not binary.exists():
        raise FileNotFoundError(
            f"code-indexer binary not found: {binary}. Build it with 'cargo build --release'."
        )

    repo_to_cases: dict[str, list[dict[str, Any]]] = {}
    for case in selected_cases:
        repo_to_cases.setdefault(case["repo"], []).append(case)

    requested_modes = ["query-only", "first-run"] if args.mode == "both" else [args.mode]

    results: list[dict[str, Any]] = []
    repos_without_valid: list[str] = []

    for repo_name in sorted(repo_to_cases.keys()):
        repo_path = repos_dir / repo_name
        if not repo_path.exists():
            raise FileNotFoundError(
                f"Repository not found: {repo_path}. Run './benches/download_repos.sh --repos {repo_name}'."
            )

        print(f"[bench] repo={repo_name} preparing index for parity precheck", file=sys.stderr)
        db_path = repo_path / ".code-index.db"
        ensure_index(binary, db_path, repo_path)

        repo_has_valid = False
        for case in repo_to_cases[repo_name]:
            symbol = case["symbol"]
            rg_pattern = case["rg_pattern"]
            rg_glob = case.get("rg_glob")

            code_count, code_cmd = count_code_indexer_definitions(binary, db_path, symbol)
            rg_count, rg_cmd = count_rg_matches(repo_path, rg_pattern, rg_glob)

            is_valid = code_count == rg_count
            reason = None
            if not is_valid:
                reason = f"count mismatch: code_indexer={code_count}, rg={rg_count}"
            else:
                repo_has_valid = True

            for mode in requested_modes:
                result: dict[str, Any] = {
                    "repo": repo_name,
                    "mode": mode,
                    "case_id": case["id"],
                    "symbol": symbol,
                    "counts": {
                        "code_indexer": code_count,
                        "rg": rg_count,
                    },
                    "validity": {
                        "is_valid": is_valid,
                        "reason": reason,
                    },
                    "commands": {
                        "code_indexer": format_cmd(code_cmd),
                        "rg": format_cmd(rg_cmd),
                    },
                    "timings": {
                        "code_indexer": {
                            "samples_ms": [],
                            "median_ms": None,
                            "p95_ms": None,
                            "cv_pct": None,
                        },
                        "rg": {
                            "samples_ms": [],
                            "median_ms": None,
                            "p95_ms": None,
                            "cv_pct": None,
                        },
                    },
                    "speedup": {
                        "median_ratio_rg_over_code_indexer": None,
                    },
                }

                if is_valid:
                    if mode == "query-only":
                        code_query_cmd = code_indexer_definition_command(binary, db_path, symbol)
                        rg_query_cmd = rg_definition_command(repo_path, rg_pattern, rg_glob)

                        code_samples = measure_callable(
                            lambda: run_quiet(code_query_cmd),
                            warmup=args.warmup,
                            runs=args.runs,
                        )
                        rg_samples = measure_callable(
                            lambda: run_quiet(rg_query_cmd, allowed_returncodes=(0, 1)),
                            warmup=args.warmup,
                            runs=args.runs,
                        )

                    elif mode == "first-run":
                        rg_query_cmd = rg_definition_command(repo_path, rg_pattern, rg_glob)

                        def code_first_run_once() -> None:
                            if db_path.exists():
                                db_path.unlink()
                            run_quiet([str(binary), "--db", str(db_path), "index", str(repo_path)])
                            run_quiet(code_indexer_definition_command(binary, db_path, symbol))

                        code_samples = measure_callable(
                            code_first_run_once,
                            warmup=args.warmup,
                            runs=args.runs,
                        )
                        rg_samples = measure_callable(
                            lambda: run_quiet(rg_query_cmd, allowed_returncodes=(0, 1)),
                            warmup=args.warmup,
                            runs=args.runs,
                        )
                    else:
                        raise AssertionError(f"Unsupported mode: {mode}")

                    code_stats = summarize_samples(code_samples)
                    rg_stats = summarize_samples(rg_samples)
                    result["timings"]["code_indexer"] = code_stats
                    result["timings"]["rg"] = rg_stats

                    code_median = code_stats["median_ms"]
                    rg_median = rg_stats["median_ms"]
                    if code_median and code_median > 0 and rg_median is not None:
                        result["speedup"]["median_ratio_rg_over_code_indexer"] = round(
                            rg_median / code_median,
                            6,
                        )

                results.append(result)

        if not repo_has_valid:
            repos_without_valid.append(repo_name)

    report = build_report(
        args=args,
        selected_cases=selected_cases,
        results=results,
        project_root=project_root,
    )

    out_json = Path(args.out_json)
    out_md = Path(args.out_md)
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_md.parent.mkdir(parents=True, exist_ok=True)

    out_json.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    out_md.write_text(render_markdown(report), encoding="utf-8")

    print(f"[bench] JSON report: {out_json}", file=sys.stderr)
    print(f"[bench] Markdown report: {out_md}", file=sys.stderr)

    if args.require_valid and repos_without_valid:
        print(
            "[bench] ERROR: no valid parity cases for repos: " + ", ".join(sorted(repos_without_valid)),
            file=sys.stderr,
        )
        return 2

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # noqa: BLE001
        print(f"[bench] ERROR: {exc}", file=sys.stderr)
        raise
