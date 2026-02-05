---
description: "Карта знаний проекта code-indexer: архитектура, интерфейсы, гайды."
---

# Memory Bank — code-indexer

Кратко: code-indexer — CLI и MCP server для индексации и семантического анализа кода на базе tree-sitter.

## Overview
[.memory-bank/architecture/overview.md](architecture/overview.md): системный контекст и ключевые контейнеры.
[.memory-bank/guides/overview.md](guides/overview.md): базовый запуск CLI и MCP server.

## Architecture
[.memory-bank/architecture/indexing-pipeline.md](architecture/indexing-pipeline.md): пайплайн индексирования и роль indexer.
[.memory-bank/architecture/interfaces.md](architecture/interfaces.md): поверхности CLI и MCP tools.
[.memory-bank/architecture/data-store.md](architecture/data-store.md): SQLite схема, FTS и хранение индекса.

## Guides
[.memory-bank/guides/indexing-pipeline.md](guides/indexing-pipeline.md): сценарии индексирования, watch, deps, db path.
[.memory-bank/guides/interfaces.md](guides/interfaces.md): как пользоваться CLI и MCP tool параметрами.
[.memory-bank/guides/data-store.md](guides/data-store.md): обслуживание БД, очистка, in-memory.

## Decisions
Пока нет.

## Open Questions
Пока нет.
