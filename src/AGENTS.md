# AGENTS — local (Codex)

Этот файл дополняет корневой `AGENTS.md`.
Сначала прочитай `../AGENTS.md` или `../../AGENTS.md` (см. ссылку ниже).

Ссылка на корень: ../AGENTS.md

Локальный контекст:
Здесь находится основная реализация на Rust. Ключевые модули: `cli`, `indexer`, `index`, `mcp`, `languages`, `workspace`, `dependencies`, `memory`, `session`, `compass`, `docs`, `git`.

Важные подмодули:
- `indexer/sidecar.rs` — парсинг `.code-indexer.yml`, extract_file_meta, extract_file_tags, staleness detection.
- `index/migrations.rs` — миграции V1-V6, включая tag_dictionary, file_meta, file_tags.
- `index/models.rs` — FileMeta, FileTag, TagDictionary, Stability, MetaSource.
