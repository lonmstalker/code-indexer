# AGENTS — local (Codex)

Этот файл дополняет корневой `AGENTS.md`.
Сначала прочитай `../AGENTS.md` или `../../AGENTS.md` (см. ссылку ниже).

Ссылка на корень: ../AGENTS.md

Локальный контекст:
Интеграционные тесты CLI и MCP. Файлы `tests/*.rs` описывают контрактное поведение; обновлять при изменении интерфейсов.
Также проверяй регрессии для persisted incremental indexing (hash skip unchanged, cleanup удалённых файлов).

Важные тестовые файлы:
- `file_tags_integration.rs` — тесты File Tags + Intent Layer (sidecar parsing, tag CRUD, FTS search, staleness detection).
- `mcp_tools.rs` — тесты MCP tools (search_symbols с tag, get_file_outline с include_file_meta).
