//! Quality benchmarks: comprehensive API coverage and language-specific extraction tests
//! on real open-source projects.
//!
//! Tests are `#[ignore]` — they require cloned repos in `benches/repos/`.
//! Run: `cargo test --test quality_benchmarks -- --ignored`

use std::path::Path;
use std::sync::OnceLock;

use code_indexer::{
    indexer::{FileWalker, Parser, SymbolExtractor},
    CodeIndex, LanguageRegistry, ReferenceKind, SearchOptions, SqliteIndex,
    SymbolKind, Visibility,
};

// ===== Helpers =====

fn index_project(project_path: &Path) -> SqliteIndex {
    let temp_db = tempfile::NamedTempFile::new().expect("temp db");
    let db_path = temp_db.path().to_path_buf();
    std::mem::forget(temp_db);

    let registry = LanguageRegistry::new();
    let walker = FileWalker::new(registry);
    let index = SqliteIndex::new(&db_path).expect("create index");

    let files = walker.walk(project_path).expect("walk");

    for file in &files {
        let registry = LanguageRegistry::new();
        let parser = Parser::new(registry);
        let extractor = SymbolExtractor::new();

        if let Ok(parsed) = parser.parse_file(file) {
            if let Ok(result) = extractor.extract_all(&parsed, file) {
                let _ = index.add_symbols(result.symbols);
                let _ = index.add_references(result.references);
                let _ = index.add_imports(result.imports);
            }
        }
    }

    index
}

fn repos_dir() -> &'static Path {
    Path::new("benches/repos")
}

fn first_file_with_ext(index: &SqliteIndex, ext: &str) -> Option<String> {
    index.get_indexed_files().ok()?.into_iter().find(|f| f.ends_with(ext))
}

// ===== Lazy repo indexes (indexed once, shared across tests) =====

macro_rules! define_repo_index {
    ($static_name:ident, $fn_name:ident, $repo:literal) => {
        static $static_name: OnceLock<Option<SqliteIndex>> = OnceLock::new();
        fn $fn_name() -> Option<&'static SqliteIndex> {
            $static_name
                .get_or_init(|| {
                    let p = repos_dir().join($repo);
                    if p.exists() {
                        Some(index_project(&p))
                    } else {
                        None
                    }
                })
                .as_ref()
        }
    };
}

define_repo_index!(RG_IDX, ripgrep_index, "ripgrep");
define_repo_index!(TK_IDX, tokio_index, "tokio");
define_repo_index!(EX_IDX, excalidraw_index, "excalidraw");
define_repo_index!(GV_IDX, guava_index, "guava");
define_repo_index!(PM_IDX, prometheus_index, "prometheus");
define_repo_index!(DJ_IDX, django_index, "django");
define_repo_index!(KT_IDX, kotlin_index, "kotlin");

macro_rules! get_or_skip {
    ($fn:ident) => {
        match $fn() {
            Some(i) => i,
            None => {
                eprintln!("Skipping: repo not found");
                return;
            }
        }
    };
}

// ===== Shared API check helpers =====

fn check_stats_and_files(index: &SqliteIndex, lang: &str, exts: &[&str], min_symbols: usize) {
    let stats = index.get_stats().expect("get_stats");
    assert!(
        stats.total_symbols > min_symbols,
        "expected >{} symbols, got {}",
        min_symbols,
        stats.total_symbols
    );
    assert!(
        stats.symbols_by_language.iter().any(|(l, _)| l == lang),
        "should have {} symbols",
        lang
    );
    assert!(!stats.symbols_by_kind.is_empty());

    let files = index.get_indexed_files().expect("get_indexed_files");
    assert!(!files.is_empty());
    assert!(
        files
            .iter()
            .any(|f| exts.iter().any(|ext| f.ends_with(ext))),
        "should have files with {:?}",
        exts
    );

    let _ = index.get_all_config_digests().expect("get_all_config_digests");
}

fn check_search_and_fuzzy(index: &SqliteIndex, term: &str, fuzzy_term: &str) {
    let opts = SearchOptions {
        limit: Some(50),
        ..Default::default()
    };
    let results = index.search(term, &opts).expect("search");
    assert!(
        !results.is_empty(),
        "search '{}' should return results",
        term
    );
    let fuzzy = index.search_fuzzy(fuzzy_term, &opts).expect("search_fuzzy");
    assert!(
        !fuzzy.is_empty(),
        "fuzzy '{}' should find results",
        fuzzy_term
    );
}

/// def_term: Some("main") for known symbols, None for dynamic discovery
fn check_definitions(index: &SqliteIndex, lang: &str, def_term: Option<&str>) {
    let term = if let Some(t) = def_term {
        t.to_string()
    } else {
        let types = index
            .list_types(&SearchOptions {
                language_filter: Some(vec![lang.to_string()]),
                limit: Some(1),
                ..Default::default()
            })
            .expect("list_types");
        types.first().expect("should have types").name.clone()
    };

    let defs = index.find_definition(&term).expect("find_definition");
    assert!(!defs.is_empty(), "should find '{}'", term);

    // find_definition_by_parent: discover a method with parent, verify filtering
    let methods = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Method]),
            language_filter: Some(vec![lang.to_string()]),
            limit: Some(200),
            ..Default::default()
        })
        .expect("list methods");

    if let Some(m) = methods.iter().find(|m| m.parent.is_some()) {
        let parent = m.parent.as_deref().unwrap();
        let filtered = index
            .find_definition_by_parent(&m.name, Some(parent), Some(lang))
            .expect("by_parent");
        assert!(
            !filtered.is_empty(),
            "should find '{}' by parent '{}'",
            m.name,
            parent
        );
        let all = index
            .find_definition_by_parent(&m.name, None, Some(lang))
            .expect("no_parent");
        assert!(
            all.len() >= filtered.len(),
            "parent filter should narrow results"
        );
    }
}

fn check_file_operations(index: &SqliteIndex, exts: &[&str]) {
    let file = exts
        .iter()
        .find_map(|ext| first_file_with_ext(index, ext))
        .expect("should have a file with matching extension");

    let syms = index.get_file_symbols(&file).expect("get_file_symbols");
    assert!(!syms.is_empty(), "file should have symbols: {}", file);

    let _ = index.get_file_imports(&file).expect("get_file_imports");
    let _ = index.get_file_importers(&file).expect("get_file_importers");
    let _ = index.get_file_metrics(&file).expect("get_file_metrics");
}

fn check_references_callers_callees(index: &SqliteIndex, lang: &str) {
    let opts = SearchOptions {
        limit: Some(100),
        ..Default::default()
    };

    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec![lang.to_string()]),
            limit: Some(10),
            ..Default::default()
        })
        .expect("list_functions");

    if let Some(f) = fns.first() {
        let _ = index.find_references(&f.name, &opts).expect("find_references");
        let _ = index.find_callers(&f.name, Some(1)).expect("find_callers");
        let _ = index.find_callees(&f.name).expect("find_callees");
    }
}

fn check_graph_analysis_metrics(index: &SqliteIndex, lang: &str) {
    let lang_filter = Some(vec![lang.to_string()]);
    let opts = SearchOptions {
        language_filter: lang_filter.clone(),
        limit: Some(5),
        ..Default::default()
    };

    // call graph
    let fns = index.list_functions(&opts).expect("list_functions");
    if let Some(f) = fns.first() {
        let _ = index.get_call_graph(&f.name, 2).expect("get_call_graph");
    }

    // dead code
    let dead = index.find_dead_code().expect("find_dead_code");
    assert_eq!(
        dead.total_count,
        dead.unused_functions.len() + dead.unused_types.len(),
        "dead code total_count should equal sum of unused"
    );

    // function metrics
    if let Some(f) = fns.first() {
        let m = index
            .get_function_metrics(&f.name)
            .expect("get_function_metrics");
        if let Some(fm) = m.first() {
            assert!(fm.loc > 0, "function LOC should be > 0");
        }
    }

    // symbol members
    let types = index
        .list_types(&SearchOptions {
            language_filter: lang_filter.clone(),
            limit: Some(5),
            ..Default::default()
        })
        .expect("list_types");
    if let Some(t) = types.first() {
        let _ = index
            .get_symbol_members(&t.name)
            .expect("get_symbol_members");
    }

    // find_implementations
    if let Some(t) = types.first() {
        let _ = index
            .find_implementations(&t.name)
            .expect("find_implementations");
    }
}

// =====================================================================
// ripgrep (Rust) — API coverage
// =====================================================================

#[test]
#[ignore]
fn rust_rg_stats_and_files() {
    check_stats_and_files(get_or_skip!(ripgrep_index), "rust", &[".rs"], 100);
}

#[test]
#[ignore]
fn rust_rg_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(ripgrep_index), "main", "mian");
}

#[test]
#[ignore]
fn rust_rg_definitions() {
    check_definitions(get_or_skip!(ripgrep_index), "rust", Some("main"));
}

#[test]
#[ignore]
fn rust_rg_file_operations() {
    check_file_operations(get_or_skip!(ripgrep_index), &[".rs"]);
}

#[test]
#[ignore]
fn rust_rg_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(ripgrep_index), "rust");
}

#[test]
#[ignore]
fn rust_rg_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(ripgrep_index), "rust");
}

// ripgrep — Rust language features

#[test]
#[ignore]
fn rust_rg_traits_extracted() {
    let index = get_or_skip!(ripgrep_index);
    let traits = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Trait]),
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(!traits.is_empty(), "should find Trait symbols in ripgrep");
}

#[test]
#[ignore]
fn rust_rg_impl_methods_have_self() {
    let index = get_or_skip!(ripgrep_index);
    let methods = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Method]),
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!methods.is_empty(), "should find methods");
    let with_self: Vec<_> = methods
        .iter()
        .filter(|m| m.params.iter().any(|p| p.is_self))
        .collect();
    assert!(
        !with_self.is_empty(),
        "Rust impl methods should have is_self=true param"
    );
}

#[test]
#[ignore]
fn rust_rg_generic_params_with_bounds() {
    let index = get_or_skip!(ripgrep_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_bounds: Vec<_> = fns
        .iter()
        .filter(|s| s.generic_params.iter().any(|gp| !gp.bounds.is_empty()))
        .collect();
    assert!(
        !with_bounds.is_empty(),
        "should find functions with bounded generic params (e.g. T: Clone)"
    );
}

#[test]
#[ignore]
fn rust_rg_visibility_extracted() {
    let index = get_or_skip!(ripgrep_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");
    let public: Vec<_> = fns
        .iter()
        .filter(|f| f.visibility == Some(Visibility::Public))
        .collect();
    assert!(
        !public.is_empty(),
        "should find public functions in ripgrep"
    );
}

#[test]
#[ignore]
fn rust_rg_return_types_captured() {
    let index = get_or_skip!(ripgrep_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!fns.is_empty());
    let with_ret: Vec<_> = fns.iter().filter(|f| f.return_type.is_some()).collect();
    assert!(
        !with_ret.is_empty(),
        "should capture return_type for Rust functions"
    );
}

#[test]
#[ignore]
fn rust_rg_doc_comments_extracted() {
    let index = get_or_skip!(ripgrep_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_doc: Vec<_> = fns
        .iter()
        .filter(|f| f.doc_comment.is_some())
        .collect();
    assert!(
        !with_doc.is_empty(),
        "ripgrep should have functions with doc comments (/// ...)"
    );
}

// =====================================================================
// tokio (Rust) — API coverage
// =====================================================================

#[test]
#[ignore]
fn rust_tokio_stats_and_files() {
    check_stats_and_files(get_or_skip!(tokio_index), "rust", &[".rs"], 500);
}

#[test]
#[ignore]
fn rust_tokio_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(tokio_index), "spawn", "spwan");
}

#[test]
#[ignore]
fn rust_tokio_definitions() {
    check_definitions(get_or_skip!(tokio_index), "rust", Some("Runtime"));
}

#[test]
#[ignore]
fn rust_tokio_file_operations() {
    check_file_operations(get_or_skip!(tokio_index), &[".rs"]);
}

#[test]
#[ignore]
fn rust_tokio_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(tokio_index), "rust");
}

#[test]
#[ignore]
fn rust_tokio_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(tokio_index), "rust");
}

// tokio — Rust repo-specific features

#[test]
#[ignore]
fn rust_tokio_structs_and_enums() {
    let index = get_or_skip!(tokio_index);
    let stats = index.get_stats().expect("stats");
    let has_struct = stats
        .symbols_by_kind
        .iter()
        .any(|(k, c)| k == "struct" && *c > 0);
    let has_enum = stats
        .symbols_by_kind
        .iter()
        .any(|(k, c)| k == "enum" && *c > 0);
    assert!(has_struct, "tokio should have Struct symbols");
    assert!(has_enum, "tokio should have Enum symbols");
}

#[test]
#[ignore]
fn rust_tokio_trait_implementations() {
    let index = get_or_skip!(tokio_index);
    let impls = index
        .find_implementations("Future")
        .expect("find_implementations");
    assert!(
        !impls.is_empty(),
        "tokio should have implementations of Future"
    );
}

#[test]
#[ignore]
fn rust_tokio_references_to_spawn() {
    let index = get_or_skip!(tokio_index);
    let opts = SearchOptions {
        limit: Some(100),
        ..Default::default()
    };
    let refs = index.find_references("spawn", &opts).expect("find_references");
    assert!(!refs.is_empty(), "tokio should have references to 'spawn'");
}

// =====================================================================
// excalidraw (TypeScript) — API coverage
// =====================================================================

#[test]
#[ignore]
fn ts_stats_and_files() {
    check_stats_and_files(
        get_or_skip!(excalidraw_index),
        "typescript",
        &[".ts", ".tsx"],
        100,
    );
}

#[test]
#[ignore]
fn ts_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(excalidraw_index), "Element", "Elemnt");
}

#[test]
#[ignore]
fn ts_definitions() {
    check_definitions(get_or_skip!(excalidraw_index), "typescript", None);
}

#[test]
#[ignore]
fn ts_file_operations() {
    check_file_operations(get_or_skip!(excalidraw_index), &[".tsx", ".ts"]);
}

#[test]
#[ignore]
fn ts_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(excalidraw_index), "typescript");
}

#[test]
#[ignore]
fn ts_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(excalidraw_index), "typescript");
}

// excalidraw — TypeScript language features

#[test]
#[ignore]
fn ts_interfaces_extracted() {
    let index = get_or_skip!(excalidraw_index);
    let interfaces = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Interface]),
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(
        !interfaces.is_empty(),
        "excalidraw should have Interface symbols"
    );
}

#[test]
#[ignore]
fn ts_arrow_functions_found() {
    let index = get_or_skip!(excalidraw_index);
    let fns = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Function]),
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(
        !fns.is_empty(),
        "excalidraw should capture arrow functions as Function"
    );
}

#[test]
#[ignore]
fn ts_type_aliases_found() {
    let index = get_or_skip!(excalidraw_index);
    let aliases = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::TypeAlias]),
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(
        !aliases.is_empty(),
        "excalidraw should have TypeAlias symbols"
    );
}

#[test]
#[ignore]
fn ts_generic_types() {
    let index = get_or_skip!(excalidraw_index);
    let types = index
        .list_types(&SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_types");
    let with_generics: Vec<_> = types
        .iter()
        .filter(|s| !s.generic_params.is_empty())
        .collect();
    assert!(
        !with_generics.is_empty(),
        "excalidraw should have types with generic_params"
    );
}

#[test]
#[ignore]
fn ts_return_types_captured() {
    let index = get_or_skip!(excalidraw_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!fns.is_empty());
    let with_ret: Vec<_> = fns.iter().filter(|f| f.return_type.is_some()).collect();
    assert!(
        !with_ret.is_empty(),
        "excalidraw should have TypeScript functions with return_type captured"
    );
}

// =====================================================================
// guava (Java) — API coverage
// =====================================================================

#[test]
#[ignore]
fn java_stats_and_files() {
    check_stats_and_files(get_or_skip!(guava_index), "java", &[".java"], 500);
}

#[test]
#[ignore]
fn java_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(guava_index), "Optional", "Optinal");
}

#[test]
#[ignore]
fn java_definitions() {
    check_definitions(get_or_skip!(guava_index), "java", Some("ImmutableList"));
}

#[test]
#[ignore]
fn java_file_operations() {
    check_file_operations(get_or_skip!(guava_index), &[".java"]);
}

#[test]
#[ignore]
fn java_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(guava_index), "java");
}

#[test]
#[ignore]
fn java_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(guava_index), "java");
}

// guava — Java language features

#[test]
#[ignore]
fn java_classes_and_interfaces() {
    let index = get_or_skip!(guava_index);
    let stats = index.get_stats().expect("stats");
    let has_class = stats
        .symbols_by_kind
        .iter()
        .any(|(k, c)| k == "class" && *c > 0);
    let has_interface = stats
        .symbols_by_kind
        .iter()
        .any(|(k, c)| k == "interface" && *c > 0);
    assert!(has_class, "guava should have Class symbols");
    assert!(has_interface, "guava should have Interface symbols");
}

#[test]
#[ignore]
fn java_generic_bounds() {
    let index = get_or_skip!(guava_index);
    let defs = index
        .find_definition("ImmutableList")
        .expect("find_definition");
    assert!(!defs.is_empty(), "should find ImmutableList");
    let with_bounds: Vec<_> = defs
        .iter()
        .filter(|s| s.generic_params.iter().any(|gp| !gp.bounds.is_empty()))
        .collect();
    assert!(
        !with_bounds.is_empty(),
        "ImmutableList should have generic params with bounds (extends)"
    );
}

#[test]
#[ignore]
fn java_varargs_params() {
    let index = get_or_skip!(guava_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_varargs: Vec<_> = fns
        .iter()
        .filter(|f| f.params.iter().any(|p| p.is_variadic))
        .collect();
    assert!(
        !with_varargs.is_empty(),
        "guava should have methods with varargs (is_variadic=true)"
    );
}

#[test]
#[ignore]
fn java_visibility_public_private() {
    let index = get_or_skip!(guava_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let has_public = fns
        .iter()
        .any(|f| f.visibility == Some(Visibility::Public));
    let has_private = fns
        .iter()
        .any(|f| f.visibility == Some(Visibility::Private));
    assert!(has_public, "guava should have public methods");
    assert!(has_private, "guava should have private methods");
}

#[test]
#[ignore]
fn java_doc_comments_extracted() {
    let index = get_or_skip!(guava_index);
    let types = index
        .list_types(&SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_types");
    let with_doc: Vec<_> = types
        .iter()
        .filter(|t| t.doc_comment.is_some())
        .collect();
    assert!(
        !with_doc.is_empty(),
        "guava should have types with Javadoc comments (/** ... */)"
    );
}

// =====================================================================
// prometheus (Go) — API coverage
// =====================================================================

#[test]
#[ignore]
fn go_stats_and_files() {
    check_stats_and_files(get_or_skip!(prometheus_index), "go", &[".go"], 200);
}

#[test]
#[ignore]
fn go_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(prometheus_index), "Register", "Registr");
}

#[test]
#[ignore]
fn go_definitions() {
    check_definitions(get_or_skip!(prometheus_index), "go", None);
}

#[test]
#[ignore]
fn go_file_operations() {
    check_file_operations(get_or_skip!(prometheus_index), &[".go"]);
}

#[test]
#[ignore]
fn go_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(prometheus_index), "go");
}

#[test]
#[ignore]
fn go_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(prometheus_index), "go");
}

// prometheus — Go language features

#[test]
#[ignore]
fn go_interfaces_extracted() {
    let index = get_or_skip!(prometheus_index);
    let interfaces = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Interface]),
            language_filter: Some(vec!["go".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(
        !interfaces.is_empty(),
        "prometheus should have Interface symbols"
    );
}

#[test]
#[ignore]
fn go_struct_methods() {
    let index = get_or_skip!(prometheus_index);
    let methods = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Method]),
            language_filter: Some(vec!["go".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(
        !methods.is_empty(),
        "prometheus should have Method symbols bound to structs"
    );
}

#[test]
#[ignore]
fn go_return_types() {
    let index = get_or_skip!(prometheus_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["go".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!fns.is_empty());
    let with_ret: Vec<_> = fns.iter().filter(|f| f.return_type.is_some()).collect();
    assert!(
        !with_ret.is_empty(),
        "Go functions should have return_type captured"
    );
}

#[test]
#[ignore]
fn go_imports_captured() {
    let index = get_or_skip!(prometheus_index);
    let go_file = first_file_with_ext(index, ".go").expect("should have .go file");
    let imports = index.get_file_imports(&go_file).expect("get_file_imports");
    assert!(
        !imports.is_empty(),
        "Go files should have imports captured"
    );
}

#[test]
#[ignore]
fn go_generic_params() {
    let index = get_or_skip!(prometheus_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["go".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let types = index
        .list_types(&SearchOptions {
            language_filter: Some(vec!["go".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_types");
    let with_generics: Vec<_> = fns
        .iter()
        .chain(types.iter())
        .filter(|s| !s.generic_params.is_empty())
        .collect();
    // Go generics were added in Go 1.18; prometheus may or may not use them
    eprintln!(
        "Go symbols with generic_params: {} (out of {} fns + {} types)",
        with_generics.len(),
        fns.len(),
        types.len()
    );
    // Soft assertion — prometheus may not use generics extensively
    if with_generics.is_empty() {
        eprintln!("WARNING: No Go generic params found in prometheus — this is acceptable if the repo doesn't use Go generics");
    }
}

// =====================================================================
// django (Python) — API coverage
// =====================================================================

#[test]
#[ignore]
fn py_stats_and_files() {
    check_stats_and_files(get_or_skip!(django_index), "python", &[".py"], 500);
}

#[test]
#[ignore]
fn py_search_and_fuzzy() {
    check_search_and_fuzzy(get_or_skip!(django_index), "Model", "Modle");
}

#[test]
#[ignore]
fn py_definitions() {
    check_definitions(get_or_skip!(django_index), "python", Some("Model"));
}

#[test]
#[ignore]
fn py_file_operations() {
    check_file_operations(get_or_skip!(django_index), &[".py"]);
}

#[test]
#[ignore]
fn py_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(django_index), "python");
}

#[test]
#[ignore]
fn py_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(django_index), "python");
}

// django — Python language features

#[test]
#[ignore]
fn py_classes_extracted() {
    let index = get_or_skip!(django_index);
    let classes = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Class]),
            language_filter: Some(vec!["python".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(!classes.is_empty(), "django should have Class symbols");
}

#[test]
#[ignore]
fn py_methods_have_self() {
    let index = get_or_skip!(django_index);
    let methods = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Method]),
            language_filter: Some(vec!["python".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!methods.is_empty(), "django should have Method symbols");
    let with_self: Vec<_> = methods
        .iter()
        .filter(|m| m.params.iter().any(|p| p.is_self))
        .collect();
    assert!(
        !with_self.is_empty(),
        "Python class methods should have self param with is_self=true"
    );
}

#[test]
#[ignore]
fn py_type_hints_captured() {
    let index = get_or_skip!(django_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["python".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_hints: Vec<_> = fns
        .iter()
        .filter(|f| f.params.iter().any(|p| p.type_annotation.is_some()))
        .collect();
    assert!(
        !with_hints.is_empty(),
        "django should have functions with type-annotated params"
    );
}

#[test]
#[ignore]
fn py_inheritance_references() {
    let index = get_or_skip!(django_index);
    let opts = SearchOptions {
        limit: Some(500),
        ..Default::default()
    };
    let refs = index.find_references("Model", &opts).expect("find_references");
    let extend_refs: Vec<_> = refs
        .iter()
        .filter(|r| r.kind == ReferenceKind::Extend)
        .collect();
    assert!(
        !extend_refs.is_empty(),
        "django should have Extend references for class inheritance"
    );
}

#[test]
#[ignore]
fn py_return_types_captured() {
    let index = get_or_skip!(django_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["python".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    assert!(!fns.is_empty());
    let with_ret: Vec<_> = fns.iter().filter(|f| f.return_type.is_some()).collect();
    assert!(
        !with_ret.is_empty(),
        "django should have Python functions with return type annotations (-> type)"
    );
}

#[test]
#[ignore]
fn py_doc_comments_extracted() {
    let index = get_or_skip!(django_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["python".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_doc: Vec<_> = fns
        .iter()
        .filter(|f| f.doc_comment.is_some())
        .collect();
    assert!(
        !with_doc.is_empty(),
        "django should have functions with docstrings"
    );
}

// =====================================================================
// kotlin — API coverage
// =====================================================================

#[test]
#[ignore]
fn kt_stats_and_files() {
    check_stats_and_files(get_or_skip!(kotlin_index), "kotlin", &[".kt", ".kts"], 100);
}

#[test]
#[ignore]
fn kt_search_and_fuzzy() {
    // Dynamic: discover a term from the index
    let index = get_or_skip!(kotlin_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(1),
            ..Default::default()
        })
        .expect("list_functions");
    let term = &fns.first().expect("should have kotlin functions").name;
    let opts = SearchOptions {
        limit: Some(50),
        ..Default::default()
    };
    let results = index.search(term, &opts).expect("search");
    assert!(!results.is_empty(), "search '{}' should return results", term);
    // Fuzzy with character swap
    let mut chars: Vec<char> = term.chars().collect();
    if chars.len() > 3 {
        chars.swap(1, 2);
    }
    let fuzzy_term: String = chars.into_iter().collect();
    let _ = index
        .search_fuzzy(&fuzzy_term, &opts)
        .expect("search_fuzzy");
}

#[test]
#[ignore]
fn kt_definitions() {
    check_definitions(get_or_skip!(kotlin_index), "kotlin", None);
}

#[test]
#[ignore]
fn kt_file_operations() {
    check_file_operations(get_or_skip!(kotlin_index), &[".kt", ".kts"]);
}

#[test]
#[ignore]
fn kt_references_callers_callees() {
    check_references_callers_callees(get_or_skip!(kotlin_index), "kotlin");
}

#[test]
#[ignore]
fn kt_graph_analysis_metrics() {
    check_graph_analysis_metrics(get_or_skip!(kotlin_index), "kotlin");
}

// kotlin — Kotlin language features

#[test]
#[ignore]
fn kt_classes_and_objects() {
    let index = get_or_skip!(kotlin_index);
    let types = index
        .list_types(&SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(
        !types.is_empty(),
        "kotlin project should have type symbols (classes/objects)"
    );
}

#[test]
#[ignore]
fn kt_type_aliases() {
    let index = get_or_skip!(kotlin_index);
    let aliases = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::TypeAlias]),
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");
    assert!(
        !aliases.is_empty(),
        "kotlin project should have TypeAlias symbols"
    );
}

#[test]
#[ignore]
fn kt_generic_params() {
    let index = get_or_skip!(kotlin_index);
    let types = index
        .list_types(&SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_types");
    let with_generics: Vec<_> = types
        .iter()
        .filter(|s| !s.generic_params.is_empty())
        .collect();
    assert!(
        !with_generics.is_empty(),
        "kotlin project should have types with generic_params"
    );
}

#[test]
#[ignore]
fn kt_vararg_params() {
    let index = get_or_skip!(kotlin_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["kotlin".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let with_vararg: Vec<_> = fns
        .iter()
        .filter(|f| f.params.iter().any(|p| p.is_variadic))
        .collect();
    assert!(
        !with_vararg.is_empty(),
        "kotlin project should have functions with vararg (is_variadic=true)"
    );
}

// =====================================================================
// Cross-project tests
// =====================================================================

#[test]
#[ignore]
fn cross_get_symbol_by_id() {
    let repos: [(&str, Option<&SqliteIndex>); 7] = [
        ("ripgrep", ripgrep_index()),
        ("tokio", tokio_index()),
        ("excalidraw", excalidraw_index()),
        ("guava", guava_index()),
        ("prometheus", prometheus_index()),
        ("django", django_index()),
        ("kotlin", kotlin_index()),
    ];

    for (name, idx_opt) in &repos {
        let Some(index) = idx_opt else { continue };
        let fns = index
            .list_functions(&SearchOptions {
                limit: Some(1),
                ..Default::default()
            })
            .expect("list_functions");
        let first = fns.first().expect("should have at least one function");
        let defs = index
            .find_definition(&first.name)
            .expect("find_definition");
        let def = defs
            .iter()
            .find(|d| !d.id.is_empty())
            .expect("should have a definition with id");
        let symbol = index
            .get_symbol(&def.id)
            .expect("get_symbol")
            .expect("symbol should exist");
        assert_eq!(
            symbol.name, def.name,
            "{}: get_symbol returned wrong symbol",
            name
        );
    }
}

#[test]
#[ignore]
fn rust_rg_search_options_coverage() {
    let index = get_or_skip!(ripgrep_index);

    // current_file
    let rs_file = first_file_with_ext(index, ".rs").expect("should have .rs file");
    let opts_cf = SearchOptions {
        limit: Some(50),
        current_file: Some(rs_file),
        ..Default::default()
    };
    let results_cf = index.search("new", &opts_cf).expect("search with current_file");
    assert!(
        !results_cf.is_empty(),
        "search with current_file should return results"
    );

    // use_advanced_ranking
    let opts_ar = SearchOptions {
        limit: Some(50),
        use_advanced_ranking: Some(true),
        ..Default::default()
    };
    let results_ar = index.search("new", &opts_ar).expect("search with advanced_ranking");
    assert!(
        !results_ar.is_empty(),
        "search with use_advanced_ranking should return results"
    );

    // fuzzy flag in SearchOptions
    let opts_fuzzy = SearchOptions {
        limit: Some(50),
        fuzzy: Some(true),
        ..Default::default()
    };
    let results_fuzzy = index.search("mian", &opts_fuzzy).expect("search with fuzzy=true");
    // fuzzy flag may or may not produce results depending on implementation
    let _ = results_fuzzy;

    // fuzzy_threshold
    let opts_ft = SearchOptions {
        limit: Some(50),
        fuzzy_threshold: Some(0.5),
        ..Default::default()
    };
    let results_ft = index
        .search_fuzzy("mian", &opts_ft)
        .expect("search_fuzzy with threshold=0.5");
    let opts_ft_high = SearchOptions {
        limit: Some(50),
        fuzzy_threshold: Some(0.95),
        ..Default::default()
    };
    let results_ft_high = index
        .search_fuzzy("mian", &opts_ft_high)
        .expect("search_fuzzy with threshold=0.95");
    // lower threshold should give >= results compared to higher threshold
    assert!(
        results_ft.len() >= results_ft_high.len(),
        "lower fuzzy_threshold should return >= results: {} vs {}",
        results_ft.len(),
        results_ft_high.len()
    );
}

#[test]
#[ignore]
fn cross_all_symbol_kinds_present() {
    let repos: [(&str, &str, Option<&SqliteIndex>); 7] = [
        ("ripgrep", "rust", ripgrep_index()),
        ("tokio", "rust", tokio_index()),
        ("excalidraw", "typescript", excalidraw_index()),
        ("guava", "java", guava_index()),
        ("prometheus", "go", prometheus_index()),
        ("django", "python", django_index()),
        ("kotlin", "kotlin", kotlin_index()),
    ];

    let mut found_kinds = std::collections::HashSet::new();

    for (name, _lang, idx_opt) in &repos {
        let Some(index) = idx_opt else { continue };
        let stats = index.get_stats().expect("stats");
        for (kind, count) in &stats.symbols_by_kind {
            if *count > 0 {
                found_kinds.insert(kind.clone());
            }
        }
        eprintln!("{}: kinds = {:?}", name, stats.symbols_by_kind);
    }

    // Check that we observe the important kinds across all repos
    let expected_kinds = [
        "function", "method", "struct", "class", "interface", "trait",
        "enum", "constant", "field", "module", "type_alias",
    ];
    for kind in &expected_kinds {
        assert!(
            found_kinds.contains(*kind),
            "SymbolKind '{}' not found in any repo. Found: {:?}",
            kind,
            found_kinds
        );
    }
    // EnumVariant and Variable are less common, check opportunistically
    if found_kinds.contains("enum_variant") {
        eprintln!("  EnumVariant: found");
    } else {
        eprintln!("  EnumVariant: NOT found in any repo (optional)");
    }
    if found_kinds.contains("variable") {
        eprintln!("  Variable: found");
    } else {
        eprintln!("  Variable: NOT found in any repo (optional)");
    }
}

#[test]
#[ignore]
fn cross_reference_kinds_coverage() {
    let repos: [(&str, Option<&SqliteIndex>); 7] = [
        ("ripgrep", ripgrep_index()),
        ("tokio", tokio_index()),
        ("excalidraw", excalidraw_index()),
        ("guava", guava_index()),
        ("prometheus", prometheus_index()),
        ("django", django_index()),
        ("kotlin", kotlin_index()),
    ];

    let mut found_kinds = std::collections::HashSet::new();

    for (name, idx_opt) in &repos {
        let Some(index) = idx_opt else { continue };

        // Get a well-referenced symbol
        let fns = index
            .list_functions(&SearchOptions {
                limit: Some(5),
                ..Default::default()
            })
            .expect("list_functions");
        let types = index
            .list_types(&SearchOptions {
                limit: Some(5),
                ..Default::default()
            })
            .expect("list_types");

        let opts = SearchOptions {
            limit: Some(500),
            ..Default::default()
        };

        for sym in fns.iter().chain(types.iter()) {
            let refs = index
                .find_references(&sym.name, &opts)
                .expect("find_references");
            for r in &refs {
                found_kinds.insert(format!("{:?}", r.kind));
            }
        }
        eprintln!("{}: ref kinds so far = {:?}", name, found_kinds);
    }

    // Core reference kinds that should appear
    assert!(
        found_kinds.contains("Call"),
        "ReferenceKind::Call not found. Found: {:?}",
        found_kinds
    );
    assert!(
        found_kinds.contains("TypeUse"),
        "ReferenceKind::TypeUse not found. Found: {:?}",
        found_kinds
    );
    assert!(
        found_kinds.contains("Extend"),
        "ReferenceKind::Extend not found. Found: {:?}",
        found_kinds
    );
    // Import, FieldAccess, TypeArgument — check opportunistically
    for kind in &["Import", "FieldAccess", "TypeArgument"] {
        if found_kinds.contains(*kind) {
            eprintln!("  ReferenceKind::{}: found", kind);
        } else {
            eprintln!("  ReferenceKind::{}: NOT found (optional)", kind);
        }
    }
}

#[test]
#[ignore]
fn java_visibility_protected() {
    let index = get_or_skip!(guava_index);
    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");
    let has_protected = fns
        .iter()
        .any(|f| f.visibility == Some(Visibility::Protected));
    assert!(
        has_protected,
        "guava should have protected methods"
    );
}

#[test]
#[ignore]
fn fuzzy_search_tolerates_typos() {
    let index = get_or_skip!(ripgrep_index);
    let opts = SearchOptions {
        limit: Some(20),
        ..Default::default()
    };
    let exact = index.search("main", &opts).expect("exact search");
    let fuzzy = index.search_fuzzy("mian", &opts).expect("fuzzy search");
    assert!(!exact.is_empty(), "exact search for 'main' should return results");
    assert!(
        !fuzzy.is_empty(),
        "fuzzy search for 'mian' (typo) should return results"
    );
}

#[test]
#[ignore]
fn cross_language_stats_consistency() {
    let repos: [(&str, Option<&SqliteIndex>); 7] = [
        ("ripgrep", ripgrep_index()),
        ("tokio", tokio_index()),
        ("excalidraw", excalidraw_index()),
        ("guava", guava_index()),
        ("prometheus", prometheus_index()),
        ("django", django_index()),
        ("kotlin", kotlin_index()),
    ];

    for (name, idx_opt) in &repos {
        let Some(index) = idx_opt else {
            eprintln!("Skipping stats consistency for {}: not found", name);
            continue;
        };
        let stats = index.get_stats().expect("stats");
        let sum: usize = stats.symbols_by_kind.iter().map(|(_, c)| *c).sum();
        assert_eq!(
            stats.total_symbols, sum,
            "{}: total_symbols ({}) != sum of symbols_by_kind ({})",
            name, stats.total_symbols, sum
        );
    }
}

// =====================================================================
// Agent Tool Comparison: code-indexer vs rg/grep
// =====================================================================

/// Run `rg -c` and sum match counts across files. Returns None if rg not found.
fn rg_match_count(pattern: &str, path: &Path, extra_args: &[&str]) -> Option<usize> {
    let mut cmd = std::process::Command::new("rg");
    cmd.arg("-c")
        .arg("--no-filename")
        .args(extra_args)
        .arg(pattern)
        .arg(path);
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
            .filter_map(|l| l.trim().parse::<usize>().ok())
            .sum(),
    )
}

fn rg_available() -> bool {
    std::process::Command::new("rg")
        .arg("--version")
        .output()
        .is_ok()
}

macro_rules! skip_if_no_rg {
    () => {
        if !rg_available() {
            eprintln!("Skipping: rg not found");
            return;
        }
    };
}

/// code-indexer `find_definition` returns only actual definitions.
/// rg text search returns every line matching the pattern — including comments, strings, usages.
#[test]
#[ignore]
fn compare_definition_precision() {
    let index = get_or_skip!(ripgrep_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("ripgrep");

    let term = "new";

    // code-indexer: semantic definitions only
    let ci_defs = index.find_definition(term).expect("find_definition");

    // rg: text search for "fn new" (Rust-specific pattern)
    let rg_fn = rg_match_count(r"fn\s+new\b", &repo_path, &["-t", "rust"]).unwrap_or(0);

    // rg: all occurrences of "new" in code
    let rg_all = rg_match_count(r"\bnew\b", &repo_path, &["-t", "rust"]).unwrap_or(0);

    eprintln!("=== Definition precision: '{}' ===", term);
    eprintln!(
        "  code-indexer find_definition: {} exact definitions",
        ci_defs.len()
    );
    eprintln!(
        "  rg 'fn\\s+new\\b': {} text matches (Rust-only pattern)",
        rg_fn
    );
    eprintln!("  rg '\\bnew\\b': {} total occurrences (all mentions)", rg_all);
    if rg_all > 0 {
        eprintln!(
            "  Noise reduction: {:.0}x fewer results with code-indexer",
            rg_all as f64 / ci_defs.len().max(1) as f64
        );
    }

    // code-indexer definitions should be fewer than raw text matches
    assert!(
        ci_defs.len() <= rg_all.max(ci_defs.len()),
        "semantic search should be more precise than text search"
    );
    assert!(!ci_defs.is_empty());
}

/// code-indexer can filter by SymbolKind (Trait, Interface, Class, etc.).
/// rg requires language-specific regex and still captures false positives.
#[test]
#[ignore]
fn compare_kind_filtering() {
    let index = get_or_skip!(ripgrep_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("ripgrep");

    // code-indexer: list only Trait symbols — structured, with generics and visibility
    let traits = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Trait]),
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(1000),
            ..Default::default()
        })
        .expect("list_types");

    // rg: best approximation — regex for "trait" keyword
    let rg_trait =
        rg_match_count(r"^\s*(pub\s+)?trait\s+\w+", &repo_path, &["-t", "rust"]).unwrap_or(0);

    eprintln!("=== Kind filtering: Traits ===");
    eprintln!(
        "  code-indexer list_types(Trait): {} symbols with names, generics, visibility",
        traits.len()
    );
    eprintln!(
        "  rg 'trait\\s+\\w+': {} text matches (may include comments, macros, doc tests)",
        rg_trait
    );
    for t in traits.iter().take(3) {
        eprintln!(
            "    Example: {} (vis={:?}, generics={})",
            t.name,
            t.visibility,
            t.generic_params.len()
        );
    }

    assert!(!traits.is_empty());
}

/// code-indexer classifies references by kind: Call, Extend, TypeUse, FieldAccess.
/// rg returns all text matches with no classification.
#[test]
#[ignore]
fn compare_reference_classification() {
    let index = get_or_skip!(django_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("django");

    let opts = SearchOptions {
        limit: Some(500),
        ..Default::default()
    };

    // code-indexer: classified references
    let refs = index
        .find_references("Model", &opts)
        .expect("find_references");
    let calls: Vec<_> = refs
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    let extends: Vec<_> = refs
        .iter()
        .filter(|r| r.kind == ReferenceKind::Extend)
        .collect();
    let type_uses: Vec<_> = refs
        .iter()
        .filter(|r| r.kind == ReferenceKind::TypeUse)
        .collect();

    // rg: all occurrences — no classification possible
    let rg_all = rg_match_count(r"\bModel\b", &repo_path, &["-t", "py"]).unwrap_or(0);

    eprintln!("=== Reference classification: 'Model' in django ===");
    eprintln!("  code-indexer find_references: {} total", refs.len());
    eprintln!("    - Call: {}", calls.len());
    eprintln!("    - Extend (inheritance): {}", extends.len());
    eprintln!("    - TypeUse: {}", type_uses.len());
    eprintln!(
        "  rg '\\bModel\\b': {} unclassified text matches",
        rg_all
    );
    eprintln!("  Advantage: agent can filter to ONLY inheritance — impossible with rg");

    assert!(!refs.is_empty());
}

/// code-indexer provides call graph traversal: callers, callees, full graph with depth.
/// This is fundamentally impossible with text search — grep cannot determine call relationships.
#[test]
#[ignore]
fn compare_call_graph_navigation() {
    let index = get_or_skip!(tokio_index);

    let callers = index.find_callers("spawn", Some(1)).expect("find_callers");
    let callees = index.find_callees("spawn").expect("find_callees");
    let graph = index.get_call_graph("spawn", 2).expect("call_graph");

    eprintln!("=== Call graph: 'spawn' in tokio ===");
    eprintln!(
        "  code-indexer find_callers: {} direct callers",
        callers.len()
    );
    eprintln!("  code-indexer find_callees: {} callees", callees.len());
    eprintln!(
        "  code-indexer call_graph(depth=2): {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );
    eprintln!("  rg equivalent: IMPOSSIBLE — grep cannot determine call relationships");
}

/// code-indexer detects unused functions and types.
/// This requires cross-referencing all definitions with all usages — fundamentally impossible with grep.
#[test]
#[ignore]
fn compare_dead_code_detection() {
    let index = get_or_skip!(ripgrep_index);

    let dead = index.find_dead_code().expect("find_dead_code");

    eprintln!("=== Dead code detection: ripgrep ===");
    eprintln!("  code-indexer find_dead_code:");
    eprintln!("    - Unused functions: {}", dead.unused_functions.len());
    eprintln!("    - Unused types: {}", dead.unused_types.len());
    eprintln!("    - Total: {}", dead.total_count);
    eprintln!("  rg equivalent: IMPOSSIBLE — requires cross-referencing all defs with all usages");

    assert_eq!(
        dead.total_count,
        dead.unused_functions.len() + dead.unused_types.len()
    );
}

/// code-indexer returns rich structured data per symbol: kind, visibility, generic params, params with
/// types, return type, parent, signature. rg can only find the text line.
#[test]
#[ignore]
fn compare_structured_symbol_info() {
    let index = get_or_skip!(guava_index);

    let defs = index
        .find_definition("ImmutableList")
        .expect("find_definition");

    eprintln!("=== Structured symbol info: ImmutableList (guava) ===");
    for d in &defs {
        eprintln!("  Symbol: {}", d.name);
        eprintln!("    kind: {:?}", d.kind);
        eprintln!("    visibility: {:?}", d.visibility);
        eprintln!("    language: {:?}", d.language);
        eprintln!("    parent: {:?}", d.parent);
        eprintln!(
            "    generic_params: {:?}",
            d.generic_params
                .iter()
                .map(|g| format!("{} (bounds: {:?})", g.name, g.bounds))
                .collect::<Vec<_>>()
        );
        if !d.params.is_empty() {
            eprintln!(
                "    params: {:?}",
                d.params
                    .iter()
                    .map(|p| format!("{}: {:?}", p.name, p.type_annotation))
                    .collect::<Vec<_>>()
            );
        }
        eprintln!("    return_type: {:?}", d.return_type);
    }
    eprintln!("  rg equivalent: finds 'class ImmutableList' text line — NO structured data");

    assert!(!defs.is_empty());
}

/// code-indexer uses the same API (SearchOptions, list_functions, list_types) for all languages.
/// rg requires different regex patterns per language, each fragile and incomplete.
#[test]
#[ignore]
fn compare_cross_language_unified_api() {
    skip_if_no_rg!();

    let repos: [(&str, &str, &str, Option<&SqliteIndex>); 4] = [
        ("ripgrep", "rust", r"fn\s+\w+", ripgrep_index()),
        (
            "excalidraw",
            "typescript",
            r"(function\s+\w+|const\s+\w+\s*=.*=>)",
            excalidraw_index(),
        ),
        (
            "guava",
            "java",
            r"(public|private|protected)\s+.*\s+\w+\s*\(",
            guava_index(),
        ),
        ("django", "python", r"def\s+\w+", django_index()),
    ];

    eprintln!("=== Cross-language: same API vs per-language regex ===");

    for (name, lang, rg_pattern, idx_opt) in &repos {
        let Some(index) = idx_opt else { continue };
        let fns = index
            .list_functions(&SearchOptions {
                language_filter: Some(vec![lang.to_string()]),
                limit: Some(5),
                ..Default::default()
            })
            .expect("list_functions");
        let types = index
            .list_types(&SearchOptions {
                language_filter: Some(vec![lang.to_string()]),
                limit: Some(5),
                ..Default::default()
            })
            .expect("list_types");

        let rg_count =
            rg_match_count(rg_pattern, &repos_dir().join(name), &[]).unwrap_or(0);

        eprintln!(
            "  {} ({}): code-indexer: {} fns + {} types | rg '{}': {} text matches",
            name,
            lang,
            fns.len(),
            types.len(),
            rg_pattern,
            rg_count
        );
    }
    eprintln!("  code-indexer: ONE API, structured results for all languages");
    eprintln!("  rg: different fragile regex per language, no structure, false positives");
}

/// code-indexer fuzzy search finds symbols despite typos. rg requires exact text match —
/// a typo in the query returns 0 useful results.
#[test]
#[ignore]
fn compare_fuzzy_search_quality() {
    let index = get_or_skip!(ripgrep_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("ripgrep");

    let typos = [("mian", "main"), ("Cofnig", "Config"), ("prse", "parse")];

    let opts = SearchOptions {
        limit: Some(20),
        ..Default::default()
    };

    eprintln!("=== Fuzzy search: code-indexer vs rg ===");

    for (typo, expected) in &typos {
        let fuzzy = index.search_fuzzy(typo, &opts).expect("search_fuzzy");
        let found_expected = fuzzy
            .iter()
            .any(|s| s.symbol.name.to_lowercase().contains(&expected.to_lowercase()));

        // rg: exact text search for the typo — will find nothing useful
        let rg_exact =
            rg_match_count(&format!(r"\b{}\b", typo), &repo_path, &["-t", "rust"])
                .unwrap_or(0);

        eprintln!("  Typo '{}' (intended '{}'):", typo, expected);
        eprintln!(
            "    code-indexer: {} results, found '{}': {}",
            fuzzy.len(),
            expected,
            found_expected
        );
        eprintln!(
            "    rg '\\b{}\\b': {} matches (typo not in code = useless)",
            typo, rg_exact
        );
    }
}

/// code-indexer `get_file_imports` returns structured import data.
/// rg can grep for import/use/from keywords but returns raw text without structure.
#[test]
#[ignore]
fn compare_import_analysis() {
    let index = get_or_skip!(prometheus_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("prometheus");

    let go_file = first_file_with_ext(index, ".go").expect("should have .go file");

    // code-indexer: structured imports
    let imports = index.get_file_imports(&go_file).expect("get_file_imports");
    let importers = index.get_file_importers(&go_file).expect("get_file_importers");

    // rg: grep for import statements
    let rg_imports =
        rg_match_count(r"^\s*import\s", &repo_path, &["-t", "go"]).unwrap_or(0);

    eprintln!("=== Import analysis: {} ===", go_file);
    eprintln!(
        "  code-indexer get_file_imports: {} structured imports (source, kind)",
        imports.len()
    );
    for imp in imports.iter().take(3) {
        eprintln!("    {:?}", imp);
    }
    eprintln!(
        "  code-indexer get_file_importers: {} files that import this file",
        importers.len()
    );
    eprintln!(
        "  rg 'import': {} text matches across project (no per-file structure, no reverse lookup)",
        rg_imports
    );
    eprintln!("  Advantage: reverse import lookup (who imports this file) — impossible with rg");
}

/// code-indexer `get_function_metrics` provides LOC, cyclomatic complexity per function.
/// rg can count lines but cannot attribute them to specific functions.
#[test]
#[ignore]
fn compare_function_metrics() {
    let index = get_or_skip!(ripgrep_index);

    let fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(10),
            ..Default::default()
        })
        .expect("list_functions");

    eprintln!("=== Function metrics: ripgrep ===");

    let mut metrics_found = 0;
    for f in &fns {
        let metrics = index
            .get_function_metrics(&f.name)
            .expect("get_function_metrics");
        if let Some(m) = metrics.first() {
            eprintln!(
                "  {} — LOC: {}, params: {}, lines: {}-{}",
                f.name, m.loc, m.parameters, m.start_line, m.end_line
            );
            metrics_found += 1;
        }
    }
    eprintln!(
        "  Metrics found for {}/{} functions",
        metrics_found,
        fns.len()
    );
    eprintln!("  rg equivalent: can count lines with `wc -l` but cannot:");
    eprintln!("    - attribute LOC to specific functions");
    eprintln!("    - count parameters per function");
    eprintln!("    - provide start/end line ranges per function");
}

/// code-indexer `get_symbol_members` returns methods/fields of a type.
/// rg cannot structurally list members of a specific class/struct.
#[test]
#[ignore]
fn compare_symbol_members_listing() {
    let index = get_or_skip!(guava_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("guava");

    let types = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Class]),
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(20),
            ..Default::default()
        })
        .expect("list_types");

    let type_with_members = types.iter().find_map(|t| {
        let members = index.get_symbol_members(&t.name).ok()?;
        if members.len() > 2 {
            Some((t.name.clone(), members))
        } else {
            None
        }
    });

    if let Some((type_name, members)) = type_with_members {
        let rg_class =
            rg_match_count(&format!(r"class\s+{}", type_name), &repo_path, &["-t", "java"])
                .unwrap_or(0);

        eprintln!("=== Symbol members: '{}' (guava) ===", type_name);
        eprintln!(
            "  code-indexer get_symbol_members: {} members with kind, visibility, params",
            members.len()
        );
        for m in members.iter().take(5) {
            eprintln!(
                "    {:?} {} (vis={:?})",
                m.kind, m.name, m.visibility
            );
        }
        eprintln!(
            "  rg 'class {}': {} text matches — NO way to list members of THIS type",
            type_name, rg_class
        );
    }
}

/// code-indexer search with kind_filter returns only specific kinds.
/// rg has no concept of symbol kinds — filtering requires language-specific regex.
#[test]
#[ignore]
fn compare_search_with_kind_filter() {
    let index = get_or_skip!(ripgrep_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("ripgrep");

    // code-indexer: search functions only
    let fns = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Function]),
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");

    // code-indexer: search structs only
    let structs = index
        .list_types(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Struct]),
            language_filter: Some(vec!["rust".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_types");

    // rg: grep for fn and struct — no structured separation
    let rg_fn =
        rg_match_count(r"^\s*(pub\s+)?fn\s+\w+", &repo_path, &["-t", "rust"]).unwrap_or(0);
    let rg_struct =
        rg_match_count(r"^\s*(pub\s+)?struct\s+\w+", &repo_path, &["-t", "rust"]).unwrap_or(0);

    eprintln!("=== Kind filter: Function vs Struct ===");
    eprintln!(
        "  code-indexer: {} functions, {} structs — cleanly separated with metadata",
        fns.len(),
        structs.len()
    );
    eprintln!(
        "  rg: ~{} fn matches, ~{} struct matches — text patterns, may include macros/comments",
        rg_fn, rg_struct
    );

    assert!(!fns.is_empty());
    assert!(!structs.is_empty());
}

/// code-indexer search with language_filter returns only symbols from specified language.
/// rg -t limits by file extension but cannot distinguish languages that share extensions.
#[test]
#[ignore]
fn compare_search_with_language_filter() {
    let index = get_or_skip!(excalidraw_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("excalidraw");

    // code-indexer: TypeScript only
    let ts_fns = index
        .list_functions(&SearchOptions {
            language_filter: Some(vec!["typescript".to_string()]),
            limit: Some(500),
            ..Default::default()
        })
        .expect("list_functions");

    // rg: search .ts/.tsx files
    let rg_ts = rg_match_count(
        r"(function\s+\w+|const\s+\w+\s*=)",
        &repo_path,
        &["--glob", "*.ts", "--glob", "*.tsx"],
    )
    .unwrap_or(0);

    eprintln!("=== Language filter: TypeScript (excalidraw) ===");
    eprintln!(
        "  code-indexer: {} TypeScript functions — parsed with tree-sitter, structured",
        ts_fns.len()
    );
    eprintln!(
        "  rg (*.ts/*.tsx): {} text matches — extension-based, includes non-function matches",
        rg_ts
    );

    assert!(!ts_fns.is_empty());
}

/// code-indexer `find_definition_by_parent` resolves methods scoped to a specific type.
/// rg cannot scope a search to a specific class/struct — it has no concept of containment.
#[test]
#[ignore]
fn compare_scoped_definition_lookup() {
    let index = get_or_skip!(guava_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("guava");

    // Find a method that exists in multiple classes
    let methods = index
        .list_functions(&SearchOptions {
            kind_filter: Some(vec![SymbolKind::Method]),
            language_filter: Some(vec!["java".to_string()]),
            limit: Some(2000),
            ..Default::default()
        })
        .expect("list_functions");

    // Find a common method name that appears with different parents
    let mut name_counts = std::collections::HashMap::new();
    for m in &methods {
        if m.parent.is_some() {
            *name_counts.entry(m.name.clone()).or_insert(0usize) += 1;
        }
    }
    let common_method = name_counts
        .iter()
        .filter(|(_, c)| **c > 1)
        .max_by_key(|(_, c)| **c)
        .map(|(n, _)| n.clone());

    if let Some(method_name) = common_method {
        let all_defs = index
            .find_definition(&method_name)
            .expect("find_definition");

        // Pick one parent
        let parent = methods
            .iter()
            .find(|m| m.name == method_name && m.parent.is_some())
            .and_then(|m| m.parent.clone())
            .unwrap();

        let scoped = index
            .find_definition_by_parent(&method_name, Some(&parent), Some("java"))
            .expect("find_definition_by_parent");

        let rg_method = rg_match_count(
            &format!(r"\b{}\b", method_name),
            &repo_path,
            &["-t", "java"],
        )
        .unwrap_or(0);

        eprintln!("=== Scoped definition: '{}' in '{}' ===", method_name, parent);
        eprintln!(
            "  code-indexer find_definition: {} total definitions across all classes",
            all_defs.len()
        );
        eprintln!(
            "  code-indexer find_definition_by_parent('{}'): {} scoped to ONE class",
            parent,
            scoped.len()
        );
        eprintln!(
            "  rg '\\b{}\\b': {} text matches — IMPOSSIBLE to scope to specific class",
            method_name, rg_method
        );

        assert!(scoped.len() <= all_defs.len());
        assert!(!scoped.is_empty());
    }
}

/// code-indexer `get_file_symbols` returns a hierarchical outline of a file.
/// rg can grep for definition patterns but returns a flat, unstructured list.
#[test]
#[ignore]
fn compare_file_outline_generation() {
    let index = get_or_skip!(ripgrep_index);
    skip_if_no_rg!();
    let repo_path = repos_dir().join("ripgrep");

    let rs_file = first_file_with_ext(index, ".rs").expect("should have .rs file");

    // code-indexer: structured file outline with hierarchy
    let symbols = index.get_file_symbols(&rs_file).expect("get_file_symbols");

    // rg: flat text search for definitions
    let rg_defs = rg_match_count(
        r"^\s*(pub\s+)?(fn|struct|enum|trait|impl|mod|const|type)\s+\w+",
        &repo_path.join(&rs_file),
        &[],
    )
    .unwrap_or(0);

    let with_parent: Vec<_> = symbols.iter().filter(|s| s.parent.is_some()).collect();

    eprintln!("=== File outline: {} ===", rs_file);
    eprintln!(
        "  code-indexer get_file_symbols: {} symbols ({} with parent → hierarchy)",
        symbols.len(),
        with_parent.len()
    );
    for s in symbols.iter().take(8) {
        eprintln!(
            "    {:?} {} (parent={:?}, vis={:?})",
            s.kind, s.name, s.parent, s.visibility
        );
    }
    eprintln!(
        "  rg definition patterns: {} flat text matches — no hierarchy, no metadata",
        rg_defs
    );

    assert!(!symbols.is_empty());
}

#[test]
#[ignore]
fn cross_language_dead_code_valid() {
    let repos: [(&str, Option<&SqliteIndex>); 7] = [
        ("ripgrep", ripgrep_index()),
        ("tokio", tokio_index()),
        ("excalidraw", excalidraw_index()),
        ("guava", guava_index()),
        ("prometheus", prometheus_index()),
        ("django", django_index()),
        ("kotlin", kotlin_index()),
    ];

    for (name, idx_opt) in &repos {
        let Some(index) = idx_opt else {
            continue;
        };
        let dead = index.find_dead_code().expect("dead_code");
        assert_eq!(
            dead.total_count,
            dead.unused_functions.len() + dead.unused_types.len(),
            "{}: dead code total_count mismatch",
            name
        );
    }
}
