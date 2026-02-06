# CLAUDE — code-indexer (Claude)

## Назначение
Этот файл описывает правила и контекст для Claude при работе с репозиторием `code-indexer`.

## Короткий контекст
`code-indexer` — CLI и MCP server для индексации и семантического анализа кода на базе `tree-sitter`.

## Где искать архитектуру и гайды
Основной источник архитектурного контекста: `.memory-bank/index.md`.
Если меняются архитектура, интерфейсы или пайплайн — обновить `.memory-bank/`.

## Основные зоны кода
- `src/cli` — CLI команды и аргументы.
  - `index` по умолчанию инкрементальный: hash-based skip unchanged + cleanup stale files.
  - `index` имеет термопрофили ресурсов: `--profile eco|balanced|max` (default: `balanced`), override через `--threads`, дополнительный `--throttle-ms`.
  - `prepare-context` собирает AI-ready контекст из NL-запроса; поддерживает `--provider openai|anthropic|openrouter|local` + budget/file/task-hint.
  - `serve` поддерживает `--transport stdio|unix` и `--socket` для daemon режима.
- `src/indexer` — индексирование и извлечение символов.
  - `sidecar.rs` — парсинг `.code-indexer.yml`, staleness detection.
  - `progress.rs` — shared progress state (atomic counters) для CLI progress bar и MCP `get_indexing_status`.
- `src/index` — SQLite слой и схема (12 таблиц, включая tag_dictionary, file_meta, file_tags).
  - `sqlite.rs` — `files` tracking (`content_hash`) для persisted incremental indexing.
- `src/mcp` — MCP server и tools (tag, include_file_meta параметры).
  - `index_workspace` инкрементально пропускает unchanged файлы по hash.
  - `prepare_context`/`get_context_bundle` — единые entrypoints для агентного retrieval-пакета.
- `src/languages` — registry и tree-sitter грамматики.
- `src/workspace` и `src/dependencies` — workspace и deps indexing.
- `tests/` — интеграционные тесты и сценарии MCP/CLI.
  - `file_tags_integration.rs` — тесты File Tags и Intent Layer.
- `benches/` — MD-документация сравнений code-indexer vs CLI (rg, grep, wc, find) + `download_repos.sh`.
  - `benches/results/*.md` — шаблоны сравнений по каждому из 7 open-source репо.
- `examples/*` — учебные проекты разных экосистем.
- `.code-indexer.yml` — sidecar-файлы с метаданными (doc1, purpose, tags).

## Команды (базовые)
- Сборка: `cargo build --release`
- Тесты: `cargo test`
- Полный rebuild индекса: удалить `.code-index.db` и запустить `code-indexer index`.

## Принципы работы
- Сначала изучить `.memory-bank/index.md`.
- Не менять `target/` и артефакты, если не попросили.
- Не трогать примерные проекты в `examples/*`, если задача не про них.
