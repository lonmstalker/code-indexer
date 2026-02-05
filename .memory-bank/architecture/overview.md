---
description: "Системный контекст и ключевые контейнеры code-indexer."
---

# Overview (Architecture)

code-indexer — CLI и MCP server для индексации кода и семантического поиска символов на базе tree-sitter.

## System Context
- Пользователь или агент вызывает CLI команды (`code-indexer index`, `code-indexer symbols`, `code-indexer serve`).
- MCP server (`src/mcp/server.rs`) предоставляет 12 consolidated tools для AI-агентов через rmcp.

## Key Containers
- CLI слой: парсинг аргументов через `clap` и маршрутизация команд в `src/cli/commands.rs`.
- MCP слой: `McpServer` и параметры tools в `src/mcp/*`.
- Indexing engine: `indexer` (парсинг, извлечение symbols, references, imports).
- Storage: `SqliteIndex` в `src/index/sqlite.rs`.
- Language registry: `LanguageRegistry` и грамматики tree-sitter в `src/languages/*`.
- Workspace и deps: `workspace` и `dependencies` для multi-module и deps indexing.
- Memory context: `memory` модуль для автоматического извлечения ProjectContext.

## Key Entry Points
- `src/main.rs` — entrypoint CLI и запуск MCP server.
- `src/lib.rs` — публичные re-exports.
- `src/cli/commands.rs` — реализация CLI команд.
- `src/mcp/server.rs` — MCP server.
- `src/index/sqlite.rs` — SQLite схема и операции.

## External Dependencies
- `tree-sitter` и набор `tree-sitter-*` грамматик.
- `rusqlite` (bundled) для SQLite.
- `rmcp` для MCP server.
- `tokio` для async runtime.
- `clap`, `tracing`, `rayon` как базовые компоненты CLI.

## Связанные материалы
[.memory-bank/guides/overview.md](../guides/overview.md): запуск и базовое использование CLI/MCP.
