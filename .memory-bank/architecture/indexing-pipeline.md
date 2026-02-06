---
description: "Пайплайн индексирования: от FileWalker до SqliteIndex."
---

# Indexing Pipeline (Architecture)

Пайплайн индексации построен вокруг `index_directory` в `src/cli/commands.rs` и компонентов `indexer`.

## Основная цепочка
1. File discovery: `FileWalker::new(LanguageRegistry)` собирает поддерживаемые файлы.
2. Incremental precheck: для каждого файла вычисляется `content_hash`; unchanged файлы пропускаются через `file_needs_reindex`.
3. Stale cleanup: из индекса удаляются tracked-файлы, которых больше нет в workspace (`remove_files_batch`).
4. Progress init: `IndexingProgress::start(files_to_index.len())` — shared atomic state для tracking.
5. Parsing: `Parser::parse_file` строит AST через tree-sitter (rayon `map_init`: parser/extractor создаются один раз на worker thread).
   - Параллелизм и тепловой профиль задаются через `index --profile eco|balanced|max`, ручной override `--threads N`, дополнительный мягкий throttling `--throttle-ms`.
6. Extraction: `SymbolExtractor::extract_all` извлекает symbols, references, imports. Queries берутся из cache (`cached_*_query`) при наличии.
7. Persist: сначала удаляются старые записи для changed-файлов, затем `SqliteIndex::add_extraction_results_batch_with_durability` сохраняет новые символы (`--durability fast|safe` для bulk index).
8. File tracking persist: `upsert_file_records_batch` обновляет `files(path, language, symbol_count, content_hash)` для следующего incremental-run.
9. Finish: `progress.finish()` — финализация прогресса.

CLI использует `indicatif::ProgressBar` для визуализации. MCP предоставляет `get_indexing_status` tool для polling.

## Watch mode
- `FileWatcher` отслеживает изменения.
- На Modified/Created: удаление старых данных по файлу и повторная индексация.
- Для changed-файла обновляется `content_hash` в `files` через `upsert_file_records_batch`.
- На Deleted: удаление файла из индекса.

## Дополнительные анализаторы
- `ScopeBuilder` строит дерево scopes на базе AST.
- `ImportResolver` резолвит import пути для разных языков.
- `CallAnalyzer` классифицирует уверенность call graph.
Эти компоненты предназначены для scope/import/call graph функций и могут использоваться в расширенных сценариях анализа.

## Связанные материалы
[.memory-bank/guides/indexing-pipeline.md](../guides/indexing-pipeline.md): сценарии индексирования и параметры CLI.
