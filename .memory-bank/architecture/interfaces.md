---
description: "Поверхности CLI и MCP tools, их назначение и границы."
---

# Interfaces (Architecture)

## CLI Surface
- `index` — индексация директории, поддерживает `--watch` и `--deep-deps`.
- `serve` — запуск MCP server.
- `symbols` — список символов с фильтрами.
- `definition` — поиск определений.
- `references` — поиск usages и callers.
- `call-graph` — анализ графа вызовов.
- `outline` — структура файла.
- `imports` — импорты файла.
- `changed` — символы по git diff.
- `stats` — статистика индекса.
- `clear` — очистка индекса.
- `deps` — операции с зависимостями.
- `query` — legacy namespace (deprecated).

## MCP Surface (23 tools)
- `index_workspace` — индексация проекта. Params: `path`, `watch`, `include_deps`.
- `update_files` — virtual documents. Params: `files[]` с `path`, `content`, `version`.
- `list_symbols` — список символов. Params: `kind`, `language`, `file`, `pattern`, `limit`, `format`.
- `search_symbols` — поиск, включая fuzzy/regex. Params: `query`, `fuzzy`, `fuzzy_threshold`, `regex`, `module`, `limit`.
- `get_symbol` — получение по ID или позиции. Params: `id`, `ids[]`, `file`, `line`, `column`.
- `find_definitions` — определения. Params: `name`, `include_deps`, `dependency`.
- `find_references` — usages. Params: `name`, `include_callers`, `include_importers`, `kind`, `depth`.
- `analyze_call_graph` — граф вызовов. Params: `function`, `direction`, `depth`, `confidence`.
- `get_file_outline` — структура файла. Params: `file`, `start_line`, `end_line`, `include_scopes`.
- `get_imports` — импорты файла. Params: `file`, `resolve`.
- `get_diagnostics` — dead code и метрики. Params: `kind`, `file`, `include_metrics`, `target`.
- `get_stats` — статистика индекса. Params: `detailed`, `include_workspace`, `include_deps`.
- `manage_tags` — управление tag inference rules. Params: `action`, `pattern`, `tags`, `confidence`, `file`, `path`.
- `get_indexing_status` — прогресс текущей индексации (files_processed, progress_pct, eta_ms). Без параметров.

## Связанные материалы
[.memory-bank/guides/interfaces.md](../guides/interfaces.md): практическое использование CLI и MCP.
