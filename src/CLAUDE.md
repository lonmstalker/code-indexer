# CLAUDE — local (Claude)

Этот файл дополняет корневой `CLAUDE.md`.
Сначала прочитай `../CLAUDE.md` или `../../CLAUDE.md` (см. ссылку ниже).

Ссылка на корень: ../CLAUDE.md

Локальный контекст:
Здесь находится основная реализация на Rust. Ключевые модули: `cli`, `indexer`, `index`, `mcp`, `languages`, `workspace`, `dependencies`, `memory`, `session`, `compass`, `docs`, `git`.

Важные подмодули:
- `indexer/sidecar.rs` — парсинг `.code-indexer.yml`, extract_file_meta, extract_file_tags, staleness detection.
- `indexer/progress.rs` — `IndexingProgress` (Arc + atomics) для CLI progress bar и MCP `get_indexing_status`.
- `index/migrations.rs` — миграции V1-V8, включая tag_dictionary, file_meta, file_tags, generic_params, `idx_symbols_def_lookup`.
- `index/models.rs` — FileMeta, FileTag, TagDictionary, Stability, MetaSource.
- `index/sqlite.rs` — `files` tracking (`content_hash`) и batch-upsert file records для persisted incremental indexing.
- `cli/commands.rs` — incremental `index` по умолчанию (skip unchanged + stale cleanup), термопрофили `--profile eco|balanced|max`, `--threads`, `--throttle-ms`, плюс `prepare-context` для AI-ready context bundle.
- `mcp/server.rs` + `mcp/consolidated.rs` — `prepare_context`/`get_context_bundle` и типы агентного routing (`provider/model/endpoint`).
