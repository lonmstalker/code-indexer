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
- `files` — метаданные файлов, hash и counts (`content_hash`, `last_size`, `last_mtime_ns` для incremental prefilter).
- `file_meta`, `file_tags`, `tag_dictionary` — Intent Layer (doc1/purpose/capabilities + теги).
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

Batch/scale-path:
- `remove_files_batch` использует `TEMP TABLE` + set-based delete (вместо больших IN-списков и промежуточных `symbol_ids` в памяти).
- Для Intent Layer есть batch API: `upsert_file_meta_batch`, `get_file_meta_many`, `get_file_meta_with_tags_many`, `add_file_tags_batch`.
- `exported_hash` в `file_meta` пишется из индексатора селективно: inferred-мета не создаётся для файлов без sidecar и без exported symbols; это снижает рост таблицы `file_meta` и объём лишних апдейтов.
- Для symbol ingest используется `add_extraction_results_batch_with_mode(results, fast_mode, cold_run)`:
  - default/safe profile: базовые PRAGMA (`WAL/NORMAL/-64000`).
  - `fast_mode` profile: `synchronous=OFF`, `cache_size=-128000`.
  - `fast_mode + cold_run` profile: агрессивный one-shot bulk (`journal_mode=MEMORY`, `locking_mode=EXCLUSIVE`, `synchronous=OFF`, `temp_store=MEMORY`, `cache_size=-256000`) с обязательным restore к дефолтному профилю после batch insert.
  - если cold-fast профиль не может быть включён из-за SQLite lock/busy, ingest автоматически fallback на обычный fast profile, чтобы избежать hard-fail индексации.
  - bulk-write использует `busy_timeout` + bounded retry/backoff на `SQLITE_BUSY/LOCKED`, чтобы снизить флапы на больших cold-run.
- `content_hash` вычисляется стабильным быстрым `xxh3_64` (вместо process-dependent `DefaultHasher`).
- Для no-op incremental есть metadata-refresh API: `update_file_tracking_metadata_batch` (обновляет `last_size/last_mtime_ns` без reindex symbols).

## Связанные материалы
[.memory-bank/guides/data-store.md](../guides/data-store.md): эксплуатация БД и команды обслуживания.
