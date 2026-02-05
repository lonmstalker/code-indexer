# AGENTS — code-indexer (Codex)

## Назначение
Этот файл описывает правила и контекст для Codex при работе с репозиторием `code-indexer`.

## Короткий контекст
`code-indexer` — CLI и MCP server для индексации и семантического анализа кода на базе `tree-sitter`.

## Где искать архитектуру и гайды
Основной источник архитектурного контекста: `.memory-bank/index.md`.
Если меняются архитектура, интерфейсы или пайплайн — обновить `.memory-bank/`.

## Основные зоны кода
- `src/cli` — CLI команды и аргументы.
- `src/indexer` — индексирование и извлечение символов.
  - `sidecar.rs` — парсинг `.code-indexer.yml`, staleness detection.
  - `progress.rs` — shared progress state (atomic counters) для CLI progress bar и MCP `get_indexing_status`.
- `src/index` — SQLite слой и схема (12 таблиц, включая tag_dictionary, file_meta, file_tags).
- `src/mcp` — MCP server и tools (tag, include_file_meta параметры).
- `src/languages` — registry и tree-sitter грамматики.
- `src/workspace` и `src/dependencies` — workspace и deps indexing.
- `tests/` — интеграционные тесты и сценарии MCP/CLI.
  - `file_tags_integration.rs` — тесты File Tags и Intent Layer.
  - `quality_benchmarks.rs` — 99 quality-тестов на 7 open-source репо (API coverage, языковые фичи, сравнение с rg).
- `benches/` — Criterion performance бенчмарки (indexing, search) + `download_repos.sh`.
- `.github/workflows/benchmarks.yml` — CI для бенчмарков (quality + performance).
- `examples/*` — учебные проекты разных экосистем.
- `.code-indexer.yml` — sidecar-файлы с метаданными (doc1, purpose, tags).

## Команды (базовые)
- Сборка: `cargo build --release`
- Тесты: `cargo test`
- Quality benchmarks: `cargo test --test quality_benchmarks -- --ignored`
- Performance benchmarks: `cargo bench`

## Принципы работы
- Сначала изучить `.memory-bank/index.md`.
- Не менять `target/` и артефакты, если не попросили.
- Не трогать примерные проекты в `examples/*`, если задача не про них.
