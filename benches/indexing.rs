//! Benchmarks for indexing large open-source projects.
//!
//! Run with: `cargo bench --bench indexing`
//!
//! Before running, download test repositories:
//! ```bash
//! ./benches/download_repos.sh
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::time::Duration;

use code_indexer::{
    indexer::{ExtractionResult, FileWalker, Parser, SymbolExtractor},
    LanguageRegistry, SqliteIndex,
};
use rayon::prelude::*;

/// Project configuration for benchmarking
struct BenchProject {
    name: &'static str,
    path: &'static str,
    language: &'static str,
}

const PROJECTS: &[BenchProject] = &[
    // Rust projects
    BenchProject {
        name: "ripgrep",
        path: "benches/repos/ripgrep",
        language: "Rust",
    },
    BenchProject {
        name: "tokio",
        path: "benches/repos/tokio",
        language: "Rust",
    },
    // TypeScript projects
    BenchProject {
        name: "excalidraw",
        path: "benches/repos/excalidraw",
        language: "TypeScript",
    },
    // Java projects
    BenchProject {
        name: "guava",
        path: "benches/repos/guava",
        language: "Java",
    },
    // Go projects
    BenchProject {
        name: "prometheus",
        path: "benches/repos/prometheus",
        language: "Go",
    },
    // Python projects
    BenchProject {
        name: "django",
        path: "benches/repos/django",
        language: "Python",
    },
    // Kotlin projects
    BenchProject {
        name: "kotlin-stdlib",
        path: "benches/repos/kotlin",
        language: "Kotlin",
    },
];

/// Index a single project and return statistics
fn index_project(project_path: &Path, db_path: &Path) -> IndexStats {
    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let index = SqliteIndex::new(db_path).expect("Failed to create index");

    let files = walker.walk(project_path).expect("Failed to walk directory");
    let files_count = files.len();

    // Parallel parsing and extraction using rayon
    let results: Vec<ExtractionResult> = files
        .par_iter()
        .filter_map(|file| {
            let registry = LanguageRegistry::new();
            let parser = Parser::new(registry);
            let extractor = SymbolExtractor::new();

            match parser.parse_file(file) {
                Ok(parsed) => extractor.extract_all(&parsed, file).ok(),
                Err(_) => None,
            }
        })
        .collect();

    let symbols_count: usize = results.iter().map(|r| r.symbols.len()).sum();

    // Batch insert all results
    index
        .add_extraction_results_batch(results)
        .expect("Failed to add results");

    let index_size = std::fs::metadata(db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    IndexStats {
        files_count,
        symbols_count,
        index_size_bytes: index_size,
    }
}

#[derive(Debug)]
struct IndexStats {
    files_count: usize,
    symbols_count: usize,
    index_size_bytes: u64,
}

fn bench_index_projects(c: &mut Criterion) {
    let mut group = c.benchmark_group("indexing");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    for project in PROJECTS {
        let project_path = Path::new(project.path);
        if !project_path.exists() {
            eprintln!(
                "Skipping {} - not found at {}. Run ./benches/download_repos.sh first.",
                project.name, project.path
            );
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new(project.language, project.name),
            &project_path,
            |b, path| {
                b.iter_with_setup(
                    || {
                        // Create a fresh temp database for each iteration
                        tempfile::NamedTempFile::new().expect("Failed to create temp file")
                    },
                    |temp_db| {
                        let stats = index_project(path, temp_db.path());
                        black_box(stats)
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark file walking separately
fn bench_file_walking(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_walking");
    group.sample_size(20);

    for project in PROJECTS {
        let project_path = Path::new(project.path);
        if !project_path.exists() {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new(project.language, project.name),
            &project_path,
            |b, path| {
                b.iter(|| {
                    let registry = LanguageRegistry::new();
                    let walker = FileWalker::new(registry);
                    let files = walker.walk(path).expect("Failed to walk");
                    black_box(files.len())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark parsing only (without extraction)
fn bench_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    for project in PROJECTS {
        let project_path = Path::new(project.path);
        if !project_path.exists() {
            continue;
        }

        // Pre-collect files to avoid including walk time
        let registry = LanguageRegistry::new();
        let walker = FileWalker::new(registry);
        let files: Vec<_> = walker
            .walk(project_path)
            .expect("Failed to walk")
            .into_iter()
            .take(500) // Limit to first 500 files for parsing benchmark
            .collect();

        if files.is_empty() {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new(project.language, project.name),
            &files,
            |b, files| {
                b.iter(|| {
                    let parsed_count: usize = files
                        .par_iter()
                        .filter_map(|file| {
                            let registry = LanguageRegistry::new();
                            let parser = Parser::new(registry);
                            parser.parse_file(file).ok().map(|_| 1)
                        })
                        .sum();
                    black_box(parsed_count)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_index_projects,
    bench_file_walking,
    bench_parsing
);
criterion_main!(benches);
