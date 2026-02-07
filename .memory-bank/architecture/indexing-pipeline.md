---
description: "Пайплайн индексирования: от FileWalker до SqliteIndex."
---

# Indexing Pipeline (Architecture)

Пайплайн индексации построен вокруг `index_directory` в `src/cli/commands.rs` и компонентов `indexer`.

## Основная цепочка
1. File discovery: `FileWalker::global()` собирает поддерживаемые файлы.
2. Stale cleanup: из индекса удаляются tracked-файлы, которых больше нет в workspace (`remove_files_batch`).
3. Run mode split:
   - `cold-run`: `tracked_files.is_empty() && tracked_hashes.is_empty()`.
   - `incremental`: есть tracked state в `files`.
4. Precheck (только incremental): параллельный `rayon`-проход с single-read (`fs::read_to_string`) и preloaded map `get_tracked_file_hashes`; unchanged-файлы отбрасываются сразу, changed-файлы несут `{path, content, content_hash}` в parse phase.
5. Progress init: `IndexingProgress::start(files_to_index.len())` — shared atomic state для tracking.
6. Parsing:
   - cold-run: `Parser::parse_file` (без `ParseCache`).
   - incremental: `ParseCache::parse_source_cached(&content, ...)` (без повторного чтения файла с диска).
   - Параллелизм и тепловой профиль задаются через `index --profile eco|balanced|max`, ручной override `--threads N`, дополнительный мягкий throttling `--throttle-ms`.
7. Extraction: `SymbolExtractor::extract_all` извлекает symbols, references, imports. Queries берутся из cache (`cached_*_query`) при наличии.
8. Persist:
   - stale cleanup уже выполнен на старте.
   - incremental: перед insert удаляются старые записи для changed-файлов (`remove_files_batch`, chunked).
   - cold-run: per-changed cleanup не выполняется (для пустой БД это лишний I/O).
   - batch insert идёт через `SqliteIndex::add_extraction_results_batch_with_mode(results, fast_mode, cold_run)`.
9. File tracking persist: `upsert_file_records_batch` обновляет `files(path, language, symbol_count, content_hash)` для следующего incremental-run.
10. Finish: `progress.finish()` — финализация прогресса.

Sidecar metadata/tags в CLI обрабатываются батчами (`upsert_file_meta_batch` / `add_file_tags_batch`), а `exported_hash` обновляется через batch retrieval/update (`get_file_meta_many` + `upsert_file_meta_batch`).

CLI использует `indicatif::ProgressBar` для визуализации. MCP предоставляет `get_indexing_status` tool для polling и использует тот же split `cold-run/incremental` в `index_workspace`.

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
