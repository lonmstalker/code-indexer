# Benchmarks

Инфраструктура бенчмарков и quality-тестов для code-indexer.

## Подготовка

```bash
# Скачать тестовые репозитории (~depth 1)
./benches/download_repos.sh
```

Репозитории сохраняются в `benches/repos/`:

| Репо | Язык | Описание |
|------|------|----------|
| ripgrep | Rust | CLI поиск (BurntSushi/ripgrep) |
| tokio | Rust | Async runtime (tokio-rs/tokio) |
| excalidraw | TypeScript | Whiteboard app (excalidraw/excalidraw) |
| guava | Java | Core libraries (google/guava) |
| prometheus | Go | Monitoring system (prometheus/prometheus) |
| django | Python | Web framework (django/django) |
| kotlin | Kotlin | Kotlin compiler (JetBrains/kotlin) |

---

## 1. Performance Benchmarks (Criterion)

### Запуск

```bash
# Все бенчмарки
cargo bench

# Только индексирование
cargo bench --bench indexing

# Только поиск
cargo bench --bench search
```

### 1.1 Indexing (`benches/indexing.rs`)

Три группы бенчмарков, запускаются на всех 7 репо:

| Группа | Что измеряет | Sample size | Measurement time |
|--------|-------------|-------------|-----------------|
| `indexing` | Полный цикл: walk + parse + extract + SQLite insert | 10 | 30s |
| `file_walking` | Только обход файлов (FileWalker) | 20 | default |
| `parsing` | Только парсинг tree-sitter (до 500 файлов, rayon) | 10 | 20s |

### 1.2 Search (`benches/search.rs`)

Пять групп бенчмарков, все на ripgrep:

| Группа | Что измеряет | Паттерны |
|--------|-------------|----------|
| `search_exact` | Точный поиск символов | `new`, `parse`, `initialize`, `getValue`, `get_value`, `Config`, `Handler` |
| `search_fuzzy` | Fuzzy поиск с опечатками | `pars`, `prse`, `apres`, `conf` |
| `find_definition` | Поиск определений | `new`, `Config`, `run`, `Builder` |
| `search_limits` | Влияние LIMIT на скорость | 10, 50, 100, 500, 1000 |
| `search_with_filter` | Поиск с/без файлового контекста | `no_filter` vs `with_file_context` |

---

## 2. Quality Benchmarks (`tests/quality_benchmarks.rs`)

Проверяют корректность извлечения символов, языковые фичи и полноту API.

### Запуск

```bash
# Компиляция (без запуска)
cargo test --test quality_benchmarks --no-run

# Запуск (тесты #[ignore], требуют скачанных репо)
cargo test --test quality_benchmarks -- --ignored

# Отдельный тест
cargo test --test quality_benchmarks rust_rg_traits_extracted -- --ignored
```

### 2.1 API Coverage тесты (6 тестов на каждое репо)

Каждое репо тестируется через 6 shared-хелперов, покрывающих все методы `CodeIndex`:

| Helper | Тестируемые API методы |
|--------|----------------------|
| `check_stats_and_files` | `get_stats`, `get_indexed_files`, `get_all_config_digests` |
| `check_search_and_fuzzy` | `search`, `search_fuzzy` |
| `check_definitions` | `find_definition`, `find_definition_by_parent`, `list_types`, `list_functions` |
| `check_file_operations` | `get_file_symbols`, `get_file_imports`, `get_file_importers`, `get_file_metrics` |
| `check_references_callers_callees` | `find_references`, `find_callers`, `find_callees`, `list_functions` |
| `check_graph_analysis_metrics` | `get_call_graph`, `find_dead_code`, `get_function_metrics`, `get_symbol_members`, `find_implementations`, `list_types` |

#### Матрица API coverage по репо

| Тест | ripgrep | tokio | excalidraw | guava | prometheus | django | kotlin |
|------|:-------:|:-----:|:----------:|:-----:|:----------:|:------:|:------:|
| `*_stats_and_files` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |
| `*_search_and_fuzzy` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |
| `*_definitions` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |
| `*_file_operations` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |
| `*_references_callers_callees` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |
| `*_graph_analysis_metrics` | `rust_rg_` | `rust_tokio_` | `ts_` | `java_` | `go_` | `py_` | `kt_` |

### 2.2 Language-specific тесты

#### ripgrep (Rust) — 6 тестов

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `rust_rg_traits_extracted` | Извлечение `Trait` символов | `list_types(kind=Trait)` непусто |
| `rust_rg_impl_methods_have_self` | `is_self=true` у методов impl | `params.iter().any(is_self)` |
| `rust_rg_generic_params_with_bounds` | Generic params с bounds (`T: Clone`) | `generic_params[].bounds` непусто |
| `rust_rg_visibility_extracted` | Visibility = Public | `visibility == Some(Public)` |
| `rust_rg_return_types_captured` | Return types извлечены | `return_type.is_some()` |
| `rust_rg_doc_comments_extracted` | Doc comments (`///`) извлечены | `doc_comment.is_some()` |

#### tokio (Rust) — 3 теста

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `rust_tokio_structs_and_enums` | Struct и Enum символы | `symbols_by_kind` содержит оба |
| `rust_tokio_trait_implementations` | Impl для `Future` | `find_implementations("Future")` непусто |
| `rust_tokio_references_to_spawn` | Call-ссылки на `spawn` | `find_references("spawn")` непусто |

#### excalidraw (TypeScript) — 5 тестов

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `ts_interfaces_extracted` | Interface символы | `list_types(kind=Interface)` непусто |
| `ts_arrow_functions_found` | Arrow functions как Function | `list_functions(kind=Function)` > 0 |
| `ts_type_aliases_found` | TypeAlias символы | `list_types(kind=TypeAlias)` непусто |
| `ts_generic_types` | Типы с generic_params | `generic_params` непусто |
| `ts_return_types_captured` | Return types извлечены | `return_type.is_some()` |

#### guava (Java) — 5 тестов

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `java_classes_and_interfaces` | Class и Interface | `symbols_by_kind` содержит оба |
| `java_generic_bounds` | Generic params с bounds (extends) | `ImmutableList.generic_params[].bounds` непусто |
| `java_varargs_params` | Spread params (`is_variadic=true`) | `params.iter().any(is_variadic)` |
| `java_visibility_public_private` | Public и Private методы | Оба visibility присутствуют |
| `java_doc_comments_extracted` | Javadoc (`/** */`) извлечены | `doc_comment.is_some()` |

#### prometheus (Go) — 5 тестов

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `go_interfaces_extracted` | Interface символы | `list_types(kind=Interface)` непусто |
| `go_struct_methods` | Method символы | `list_functions(kind=Method)` непусто |
| `go_return_types` | Return types извлечены | `return_type.is_some()` |
| `go_imports_captured` | Импорты .go файлов | `get_file_imports()` непусто |
| `go_generic_params` | Generic params (Go 1.18+) | `generic_params` (soft check) |

#### django (Python) — 6 тестов

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `py_classes_extracted` | Class символы | `list_types(kind=Class)` непусто |
| `py_methods_have_self` | `self` param с `is_self=true` | `params.iter().any(is_self)` |
| `py_type_hints_captured` | Type annotations в params | `type_annotation.is_some()` |
| `py_inheritance_references` | `ReferenceKind::Extend` ссылки | `find_references("Model")` с Extend |
| `py_return_types_captured` | Return type annotations (`->`) | `return_type.is_some()` |
| `py_doc_comments_extracted` | Docstrings извлечены | `doc_comment.is_some()` |

#### kotlin — 4 теста

| Тест | Что проверяет | Ключевая проверка |
|------|--------------|-------------------|
| `kt_classes_and_objects` | Type символы (classes/objects) | `list_types()` непусто |
| `kt_type_aliases` | TypeAlias символы | `list_types(kind=TypeAlias)` непусто |
| `kt_generic_params` | Типы с generic_params | `generic_params` непусто |
| `kt_vararg_params` | `vararg` параметры | `is_variadic=true` |

### 2.3 Agent Tool Comparison (code-indexer vs rg/grep) — 15 тестов

Сравнивают возможности code-indexer с типичными агентскими инструментами (rg, grep, wc).
Требуют установленный `rg` (ripgrep) — пропускаются если не найден.

Запуск с выводом метрик:
```bash
cargo test --test quality_benchmarks compare_ -- --ignored --nocapture
```

| Тест | code-indexer | rg/grep эквивалент | Что демонстрирует |
|------|-------------|-------------------|-------------------|
| `compare_definition_precision` | `find_definition("new")` — только определения | `rg "fn\s+new\b"` — все текстовые совпадения | Снижение шума: Nx меньше результатов |
| `compare_kind_filtering` | `list_types(kind=Trait)` — структурированные символы | `rg "trait\s+\w+"` — текстовые строки | Фильтрация по виду символа + метаданные |
| `compare_reference_classification` | `find_references("Model")` — Call/Extend/TypeUse | `rg "\bModel\b"` — все упоминания без разделения | Классификация ссылок по типу |
| `compare_call_graph_navigation` | `find_callers` + `find_callees` + `get_call_graph` | **НЕВОЗМОЖНО** с rg | Граф вызовов с глубиной |
| `compare_dead_code_detection` | `find_dead_code()` — unused functions/types | **НЕВОЗМОЖНО** с rg | Анализ мертвого кода |
| `compare_structured_symbol_info` | `find_definition("ImmutableList")` — kind, generics, params, visibility | `rg "class ImmutableList"` — одна строка текста | Структурированные метаданные |
| `compare_cross_language_unified_api` | Один API для Rust/TS/Java/Python | Разные regex per language | Единый интерфейс для всех языков |
| `compare_fuzzy_search_quality` | `search_fuzzy("mian")` → находит "main" | `rg "\bmian\b"` → 0 результатов | Толерантность к опечаткам |
| `compare_import_analysis` | `get_file_imports` + `get_file_importers` | `rg "import"` — без структуры, без reverse lookup | Обратный поиск импортов |
| `compare_function_metrics` | `get_function_metrics` — LOC, params, line range per function | `wc -l` — только общее количество строк | Метрики на уровне функций |
| `compare_symbol_members_listing` | `get_symbol_members(type)` — методы/поля типа | `rg "class X"` — не может перечислить members | Структурный доступ к членам типа |
| `compare_search_with_kind_filter` | `list_functions(kind=Function)` vs `list_types(kind=Struct)` | `rg "fn"` + `rg "struct"` — нет разделения | Фильтрация по виду символа |
| `compare_search_with_language_filter` | `list_functions(language=typescript)` | `rg --glob *.ts` — по расширению | Семантическая фильтрация по языку |
| `compare_scoped_definition_lookup` | `find_definition_by_parent(method, Class)` | **НЕВОЗМОЖНО** с rg | Поиск метода в контексте класса |
| `compare_file_outline_generation` | `get_file_symbols(file)` — иерархия с parent | `rg "fn\|struct"` — плоский список | Иерархический outline файла |

#### Категории преимуществ

| Категория | Что code-indexer делает, а rg/grep не может |
|-----------|---------------------------------------------|
| **Семантическая точность** | Возвращает только определения/символы, а не все текстовые совпадения |
| **Классификация** | Различает Call, Extend, TypeUse, FieldAccess ссылки |
| **Структура** | Каждый символ: kind, visibility, generics, params, return_type, parent |
| **Граф вызовов** | `find_callers` → `find_callees` → `get_call_graph` с глубиной обхода |
| **Анализ** | Dead code detection, function metrics, reverse imports |
| **Fuzzy поиск** | Находит символы несмотря на опечатки в запросе |
| **Кросс-языковость** | Один API (`SearchOptions`) для 9+ языков |

### 2.4 Cross-project тесты — 8 тестов

| Тест | Что проверяет | Репо |
|------|--------------|------|
| `fuzzy_search_tolerates_typos` | Fuzzy search "mian" → "main" | ripgrep |
| `cross_language_stats_consistency` | `total_symbols == sum(symbols_by_kind)` | все 7 |
| `cross_language_dead_code_valid` | `total_count == unused_functions + unused_types` | все 7 |
| `cross_get_symbol_by_id` | `get_symbol(id)` возвращает корректный символ | все 7 |
| `rust_rg_search_options_coverage` | `current_file`, `use_advanced_ranking`, `fuzzy`, `fuzzy_threshold` | ripgrep |
| `cross_all_symbol_kinds_present` | Все основные SymbolKind встречаются | все 7 |
| `cross_reference_kinds_coverage` | Все основные ReferenceKind встречаются | все 7 |
| `java_visibility_protected` | `Visibility::Protected` в guava | guava |

---

## 3. Сводка по количеству тестов

| Секция | Количество |
|--------|-----------|
| API coverage (7 репо x 6 тестов) | 42 |
| ripgrep language-specific | 6 |
| tokio language-specific | 3 |
| excalidraw language-specific | 5 |
| guava language-specific | 5 |
| prometheus language-specific | 5 |
| django language-specific | 6 |
| kotlin language-specific | 4 |
| Agent tool comparison | 15 |
| Cross-project | 8 |
| **Итого quality тестов** | **99** |

---

## 4. Таблицы для записи результатов

### 4.1 Performance: Indexing (полный цикл)

| Репо | Язык | Файлов | Символов | DB размер | Время (mean) | Время (std dev) |
|------|------|--------|----------|-----------|-------------|----------------|
| ripgrep | Rust | | | | | |
| tokio | Rust | | | | | |
| excalidraw | TypeScript | | | | | |
| guava | Java | | | | | |
| prometheus | Go | | | | | |
| django | Python | | | | | |
| kotlin | Kotlin | | | | | |

### 4.2 Performance: File Walking

| Репо | Язык | Файлов найдено | Время (mean) | Время (std dev) |
|------|------|---------------|-------------|----------------|
| ripgrep | Rust | | | |
| tokio | Rust | | | |
| excalidraw | TypeScript | | | |
| guava | Java | | | |
| prometheus | Go | | | |
| django | Python | | | |
| kotlin | Kotlin | | | |

### 4.3 Performance: Parsing (tree-sitter, до 500 файлов)

| Репо | Язык | Файлов parsed | Время (mean) | Время (std dev) |
|------|------|--------------|-------------|----------------|
| ripgrep | Rust | | | |
| tokio | Rust | | | |
| excalidraw | TypeScript | | | |
| guava | Java | | | |
| prometheus | Go | | | |
| django | Python | | | |
| kotlin | Kotlin | | | |

### 4.4 Performance: Search (на ripgrep)

#### Exact Search

| Паттерн | Запрос | Время (mean) | Время (std dev) |
|---------|--------|-------------|----------------|
| short_exact | `new` | | |
| medium_exact | `parse` | | |
| long_exact | `initialize` | | |
| camelCase | `getValue` | | |
| snake_case | `get_value` | | |
| prefix | `parse` | | |
| type_name | `Config` | | |
| interface | `Handler` | | |

#### Fuzzy Search

| Паттерн | Запрос | Описание | Время (mean) | Время (std dev) |
|---------|--------|----------|-------------|----------------|
| typo_1 | `pars` | 1 символ пропущен | | |
| typo_2 | `prse` | опечатка | | |
| typo_swap | `apres` | перестановка символов | | |
| partial | `conf` | частичный запрос | | |

#### Find Definition

| Паттерн | Запрос | Время (mean) | Время (std dev) |
|---------|--------|-------------|----------------|
| common_fn | `new` | | |
| type | `Config` | | |
| trait_impl | `run` | | |
| nested | `Builder` | | |

#### Search Limits

| LIMIT | Время (mean) | Время (std dev) |
|-------|-------------|----------------|
| 10 | | |
| 50 | | |
| 100 | | |
| 500 | | |
| 1000 | | |

#### Search с фильтром

| Вариант | Время (mean) | Время (std dev) |
|---------|-------------|----------------|
| no_filter | | |
| with_file_context | | |

### 4.5 Quality Benchmarks: Результаты

#### ripgrep (Rust)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| rust_rg_stats_and_files | | |
| rust_rg_search_and_fuzzy | | |
| rust_rg_definitions | | |
| rust_rg_file_operations | | |
| rust_rg_references_callers_callees | | |
| rust_rg_graph_analysis_metrics | | |
| rust_rg_traits_extracted | | |
| rust_rg_impl_methods_have_self | | |
| rust_rg_generic_params_with_bounds | | |
| rust_rg_visibility_extracted | | |
| rust_rg_return_types_captured | | |
| rust_rg_doc_comments_extracted | | |

#### tokio (Rust)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| rust_tokio_stats_and_files | | |
| rust_tokio_search_and_fuzzy | | |
| rust_tokio_definitions | | |
| rust_tokio_file_operations | | |
| rust_tokio_references_callers_callees | | |
| rust_tokio_graph_analysis_metrics | | |
| rust_tokio_structs_and_enums | | |
| rust_tokio_trait_implementations | | |
| rust_tokio_references_to_spawn | | |

#### excalidraw (TypeScript)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| ts_stats_and_files | | |
| ts_search_and_fuzzy | | |
| ts_definitions | | |
| ts_file_operations | | |
| ts_references_callers_callees | | |
| ts_graph_analysis_metrics | | |
| ts_interfaces_extracted | | |
| ts_arrow_functions_found | | |
| ts_type_aliases_found | | |
| ts_generic_types | | |
| ts_return_types_captured | | |

#### guava (Java)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| java_stats_and_files | | |
| java_search_and_fuzzy | | |
| java_definitions | | |
| java_file_operations | | |
| java_references_callers_callees | | |
| java_graph_analysis_metrics | | |
| java_classes_and_interfaces | | |
| java_generic_bounds | | |
| java_varargs_params | | |
| java_visibility_public_private | | |
| java_doc_comments_extracted | | |

#### prometheus (Go)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| go_stats_and_files | | |
| go_search_and_fuzzy | | |
| go_definitions | | |
| go_file_operations | | |
| go_references_callers_callees | | |
| go_graph_analysis_metrics | | |
| go_interfaces_extracted | | |
| go_struct_methods | | |
| go_return_types | | |
| go_imports_captured | | |
| go_generic_params | | |

#### django (Python)

| Тест | Результат | Примечания |
|------|-----------|-----------|
| py_stats_and_files | | |
| py_search_and_fuzzy | | |
| py_definitions | | |
| py_file_operations | | |
| py_references_callers_callees | | |
| py_graph_analysis_metrics | | |
| py_classes_extracted | | |
| py_methods_have_self | | |
| py_type_hints_captured | | |
| py_inheritance_references | | |
| py_return_types_captured | | |
| py_doc_comments_extracted | | |

#### kotlin

| Тест | Результат | Примечания |
|------|-----------|-----------|
| kt_stats_and_files | | |
| kt_search_and_fuzzy | | |
| kt_definitions | | |
| kt_file_operations | | |
| kt_references_callers_callees | | |
| kt_graph_analysis_metrics | | |
| kt_classes_and_objects | | |
| kt_type_aliases | | |
| kt_generic_params | | |
| kt_vararg_params | | |

#### Agent Tool Comparison

| Тест | Результат | code-indexer | rg/grep | Noise reduction |
|------|-----------|-------------|---------|----------------|
| compare_definition_precision | | | | |
| compare_kind_filtering | | | | |
| compare_reference_classification | | | | |
| compare_call_graph_navigation | | | N/A | |
| compare_dead_code_detection | | | N/A | |
| compare_structured_symbol_info | | | | |
| compare_cross_language_unified_api | | | | |
| compare_fuzzy_search_quality | | | | |
| compare_import_analysis | | | | |
| compare_function_metrics | | | N/A | |
| compare_symbol_members_listing | | | N/A | |
| compare_search_with_kind_filter | | | | |
| compare_search_with_language_filter | | | | |
| compare_scoped_definition_lookup | | | N/A | |
| compare_file_outline_generation | | | | |

#### Cross-project

| Тест | Результат | Примечания |
|------|-----------|-----------|
| fuzzy_search_tolerates_typos | | |
| cross_language_stats_consistency | | |
| cross_language_dead_code_valid | | |
| cross_get_symbol_by_id | | |
| rust_rg_search_options_coverage | | |
| cross_all_symbol_kinds_present | | |
| cross_reference_kinds_coverage | | |
| java_visibility_protected | | |

---

## 5. Матрица проверяемых языковых фич

| Фича | Rust (rg) | Rust (tokio) | TS | Java | Go | Python | Kotlin |
|------|:---------:|:------------:|:--:|:----:|:--:|:------:|:------:|
| Trait/Interface extraction | x | | x | x | x | | |
| Class extraction | | | | x | | x | x |
| Struct + Enum | | x | | | | | |
| TypeAlias | | | x | | | | x |
| Method is_self | x | | | | | x | |
| Generic params | x | | x | x | x | | x |
| Generic bounds | x | | | x | | | |
| Visibility (pub/priv/prot) | x | | | x | | | |
| Return types | x | | x | | x | x | |
| Varargs/variadic | | | | x | | | x |
| Type annotations | | | | | | x | |
| Imports | | | | | x | | |
| Struct methods | | | | | x | | |
| Arrow functions | | | x | | | | |
| Trait implementations | | x | | | | | |
| References (Call) | | x | | | | | |
| Inheritance (Extend) | | | | | | x | |
| Doc comments | x | | | x | | x | |
