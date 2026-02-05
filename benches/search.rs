//! Benchmarks for search operations.
//!
//! Run with: `cargo bench --bench search`
//!
//! This benchmark requires pre-indexed repositories.
//! Run `./benches/download_repos.sh` and then index them first.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::time::Duration;

use code_indexer::{
    indexer::{FileWalker, Parser, SymbolExtractor},
    CodeIndex, LanguageRegistry, SearchOptions, SqliteIndex,
};
use rayon::prelude::*;

/// Search query patterns for benchmarking
struct SearchPattern {
    name: &'static str,
    query: &'static str,
    description: &'static str,
}

const SEARCH_PATTERNS: &[SearchPattern] = &[
    SearchPattern {
        name: "short_exact",
        query: "new",
        description: "Short common function name",
    },
    SearchPattern {
        name: "medium_exact",
        query: "parse",
        description: "Medium common function name",
    },
    SearchPattern {
        name: "long_exact",
        query: "initialize",
        description: "Longer function name",
    },
    SearchPattern {
        name: "camelCase",
        query: "getValue",
        description: "CamelCase pattern",
    },
    SearchPattern {
        name: "snake_case",
        query: "get_value",
        description: "Snake_case pattern",
    },
    SearchPattern {
        name: "prefix",
        query: "parse",
        description: "Prefix search (uses FTS)",
    },
    SearchPattern {
        name: "type_name",
        query: "Config",
        description: "Common type name",
    },
    SearchPattern {
        name: "interface",
        query: "Handler",
        description: "Interface/trait pattern",
    },
];

/// Set up an index with a pre-indexed project
fn setup_index(project_path: &Path) -> Option<SqliteIndex> {
    if !project_path.exists() {
        return None;
    }

    let temp_db = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_db.path().to_path_buf();

    // Keep the temp file alive by leaking it (benchmark will clean up on exit)
    std::mem::forget(temp_db);

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let index = SqliteIndex::new(&db_path).expect("Failed to create index");

    let files = walker.walk(project_path).expect("Failed to walk directory");

    let results: Vec<_> = files
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

    index
        .add_extraction_results_batch(results)
        .expect("Failed to add results");

    Some(index)
}

fn bench_search_exact(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_exact");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    // Use ripgrep as the test project (medium size, fast to index)
    let project_path = Path::new("benches/repos/ripgrep");
    let index = match setup_index(project_path) {
        Some(idx) => idx,
        None => {
            eprintln!("Skipping search benchmarks - ripgrep not found");
            return;
        }
    };

    for pattern in SEARCH_PATTERNS {
        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new(pattern.name, pattern.description),
            &(pattern.query, &options),
            |b, (query, opts)| {
                b.iter(|| {
                    let results = index.search(query, opts).expect("Search failed");
                    black_box(results.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_search_fuzzy(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_fuzzy");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    let project_path = Path::new("benches/repos/ripgrep");
    let index = match setup_index(project_path) {
        Some(idx) => idx,
        None => {
            eprintln!("Skipping fuzzy search benchmarks - ripgrep not found");
            return;
        }
    };

    // Fuzzy search patterns (with typos)
    let fuzzy_patterns = [
        ("typo_1", "pars", "parse with 1 char missing"),
        ("typo_2", "prse", "parse with typo"),
        ("typo_swap", "apres", "parse with swapped chars"),
        ("partial", "conf", "config partial"),
    ];

    for (name, query, description) in fuzzy_patterns {
        let options = SearchOptions {
            limit: Some(100),
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new(name, description),
            &(query, &options),
            |b, (query, opts)| {
                b.iter(|| {
                    let results = index.search_fuzzy(query, opts).expect("Fuzzy search failed");
                    black_box(results.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_find_definition(c: &mut Criterion) {
    let mut group = c.benchmark_group("find_definition");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    let project_path = Path::new("benches/repos/ripgrep");
    let index = match setup_index(project_path) {
        Some(idx) => idx,
        None => {
            eprintln!("Skipping find_definition benchmarks - ripgrep not found");
            return;
        }
    };

    let definition_queries = [
        ("common_fn", "new"),
        ("type", "Config"),
        ("trait_impl", "run"),
        ("nested", "Builder"),
    ];

    for (name, query) in definition_queries {
        group.bench_with_input(BenchmarkId::from_parameter(name), &query, |b, query| {
            b.iter(|| {
                let results = index.find_definition(query).expect("Find definition failed");
                black_box(results.len())
            });
        });
    }

    group.finish();
}

fn bench_search_limits(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_limits");
    group.sample_size(50);

    let project_path = Path::new("benches/repos/ripgrep");
    let index = match setup_index(project_path) {
        Some(idx) => idx,
        None => {
            eprintln!("Skipping search limits benchmarks - ripgrep not found");
            return;
        }
    };

    let limits = [10, 50, 100, 500, 1000];
    let query = "new"; // Common symbol name

    for limit in limits {
        let options = SearchOptions {
            limit: Some(limit),
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("limit_{}", limit)),
            &options,
            |b, opts| {
                b.iter(|| {
                    let results = index.search(query, opts).expect("Search failed");
                    black_box(results.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_search_with_file_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_with_filter");
    group.sample_size(50);

    let project_path = Path::new("benches/repos/ripgrep");
    let index = match setup_index(project_path) {
        Some(idx) => idx,
        None => {
            eprintln!("Skipping search filter benchmarks - ripgrep not found");
            return;
        }
    };

    let query = "new";

    // Without file filter
    let options_no_filter = SearchOptions {
        limit: Some(100),
        ..Default::default()
    };

    group.bench_function("no_filter", |b| {
        b.iter(|| {
            let results = index.search(query, &options_no_filter).expect("Search failed");
            black_box(results.len())
        });
    });

    // With current file context
    let options_with_file = SearchOptions {
        limit: Some(100),
        current_file: Some("src/main.rs".to_string()),
        ..Default::default()
    };

    group.bench_function("with_file_context", |b| {
        b.iter(|| {
            let results = index.search(query, &options_with_file).expect("Search failed");
            black_box(results.len())
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_search_exact,
    bench_search_fuzzy,
    bench_find_definition,
    bench_search_limits,
    bench_search_with_file_filter
);
criterion_main!(benches);
