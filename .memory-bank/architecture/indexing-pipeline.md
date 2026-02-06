---
description: "Пайплайн индексирования: от FileWalker до SqliteIndex."
---

# Indexing Pipeline (Architecture)

Пайплайн индексации построен вокруг `index_directory` в `src/cli/commands.rs` и компонентов `indexer`.

## Основная цепочка
1. File discovery: `FileWalker::new(LanguageRegistry)` собирает поддерживаемые файлы.
2. Progress init: `IndexingProgress::start(files.len())` — shared atomic state для tracking.
3. Parsing: `Parser::parse_file` строит AST через tree-sitter (rayon `map_init`: parser/extractor создаются один раз на worker thread).
4. Extraction: `SymbolExtractor::extract_all` извлекает symbols, references, imports. Queries берутся из cache (`cached_*_query`) при наличии.
5. Persist: `SqliteIndex::add_extraction_results_batch_with_durability` сохраняет данные в SQLite (`--durability fast|safe` для bulk index).
6. Finish: `progress.finish()` — финализация прогресса.

CLI использует `indicatif::ProgressBar` для визуализации. MCP предоставляет `get_indexing_status` tool для polling.

## Watch mode
- `FileWatcher` отслеживает изменения.
- На Modified/Created: удаление старых данных по файлу и повторная индексация.
- На Deleted: удаление файла из индекса.

## Дополнительные анализаторы
- `ScopeBuilder` строит дерево scopes на базе AST.
- `ImportResolver` резолвит import пути для разных языков.
- `CallAnalyzer` классифицирует уверенность call graph.
Эти компоненты предназначены для scope/import/call graph функций и могут использоваться в расширенных сценариях анализа.

## Связанные материалы
[.memory-bank/guides/indexing-pipeline.md](../guides/indexing-pipeline.md): сценарии индексирования и параметры CLI.
