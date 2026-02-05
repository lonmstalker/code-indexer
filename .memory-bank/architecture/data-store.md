---
description: "SQLite схема индекса, FTS и параметры хранения."
---

# Data Store (Architecture)

Хранилище индекса реализовано в `SqliteIndex` (`src/index/sqlite.rs`).

## Основные таблицы
- `symbols` — definitions (name, kind, file, location, visibility, signature, doc_comment, parent).
- `symbol_references` — usages с типом ссылки (`reference_kind`).
- `file_imports` — imports и их тип (`import_type`).
- `call_edges` — ребра графа вызовов с `CallConfidence`.
- `scopes` — дерево scopes для scope-aware анализа.
- `files` — метаданные файлов, hash и counts.
- `projects` и `dependencies` — metadata по deps.

## Поиск
- `symbols_fts` — FTS5 виртуальная таблица для поиска по name/signature/doc_comment.
- Триггеры поддерживают синхронизацию `symbols` и `symbols_fts`.

## Производительность
PRAGMA настройки:
- `journal_mode = WAL`
- `synchronous = NORMAL`
- `cache_size = -64000`
- `temp_store = MEMORY`

## Связанные материалы
[.memory-bank/guides/data-store.md](../guides/data-store.md): эксплуатация БД и команды обслуживания.
