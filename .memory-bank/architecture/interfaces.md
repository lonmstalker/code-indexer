---
description: "Поверхности CLI и MCP tools, их назначение и границы."
---

# Interfaces (Architecture)

## CLI Surface
- `index` — индексация директории, поддерживает `--watch`, `--deep-deps`, `--durability fast|safe`, `--profile eco|balanced|max`, `--threads N`, `--throttle-ms`; по умолчанию работает инкрементально (skip unchanged + cleanup stale files по `content_hash`/tracked files), использует single-read precheck, global parser/walker и batch sidecar/meta persist.
- `prepare-context` — agent-only оркестрация сбора контекста из NL-запроса; поддерживает `--file`, `--task-hint`, budget-флаги и лимиты оркестрации (`--agent-timeout-sec`, `--agent-max-steps`, `--agent-include-trace`); routing читается из корневого `.code-indexer.yml` (`agent.provider/model/endpoint/api_key[_env]`). Без валидного agent-конфига команда завершается ошибкой.
- `serve` — запуск MCP server (`--transport stdio|unix`, `--socket <path>` для unix daemon).
- `symbols` — список символов с фильтрами, поддерживает `--remote <unix-socket>`.
- `definition` — поиск определений, поддерживает `--remote <unix-socket>`.
- `references` — поиск usages и callers, поддерживает `--remote <unix-socket>`.
- `call-graph` — анализ графа вызовов, поддерживает `--remote <unix-socket>`.
- `outline` — структура файла, поддерживает `--remote <unix-socket>`.
- `imports` — импорты файла, поддерживает `--remote <unix-socket>`.
- `changed` — символы по git diff.
- `stats` — статистика индекса, поддерживает `--remote <unix-socket>`.
- `clear` — очистка индекса.
- `deps` — операции с зависимостями.
- `query` — legacy namespace (deprecated).

## MCP Surface (24 tools)
- `index_workspace` — индексация проекта. Params: `path`, `watch`, `include_deps`.
  - heavy scan/parse path выполняется в `spawn_blocking`, writes идут через serialized write queue (если включена), persist — batch.
- `update_files` — virtual documents. Params: `files[]` с `path`, `content`, `version`.
- `list_symbols` — список символов. Params: `kind`, `language`, `file`, `pattern`, `limit`, `format`.
- `search_symbols` — поиск, включая fuzzy/regex. Params: `query`, `fuzzy`, `fuzzy_threshold`, `regex`, `module`, `limit`.
  - `include_file_meta=true` использует batch retrieval (`get_file_meta_with_tags_many`) вместо per-file запросов.
- `get_symbol` — получение по ID или позиции. Params: `id`, `ids[]`, `file`, `line`, `column`.
- `find_definitions` — определения. Params: `name`, `include_deps`, `dependency`.
- `find_references` — usages. Params: `name`, `include_callers`, `include_importers`, `kind`, `depth`.
- `analyze_call_graph` — граф вызовов. Params: `function`, `direction`, `depth`, `confidence`.
- `get_file_outline` — структура файла. Params: `file`, `start_line`, `end_line`, `include_scopes`.
- `get_imports` — импорты файла. Params: `file`, `resolve`.
- `get_diagnostics` — dead code и метрики. Params: `kind`, `file`, `include_metrics`, `target`.
- `get_stats` — статистика индекса. Params: `detailed`, `include_workspace`, `include_deps`.
- `prepare_context` — agent-only entrypoint для context collection. Params: `query`, `file`, `task_hint`, `max_items`, `approx_tokens`, `agent_timeout_ms`, `agent_max_steps`, `include_trace`, `agent` (optional override; по умолчанию routing из `.code-indexer.yml`, включая token env fallback).
- Для `provider: local` auth может быть опциональным (если gateway не требует bearer token).
- Возвращает контекст без agent-plan/summary: `task_context`, `coverage`, `gaps`, `collection_meta` (+ стандартный envelope/next/warnings).
- Для deterministic/non-agent подготовки контекста используется отдельный `get_context_bundle`.
- `manage_tags` — управление tag inference rules. Params: `action`, `pattern`, `tags`, `confidence`, `file`, `path`.
- `get_indexing_status` — прогресс текущей индексации (files_processed, progress_pct, eta_ms). Без параметров.

## Связанные материалы
[.memory-bank/guides/interfaces.md](../guides/interfaces.md): практическое использование CLI и MCP.
