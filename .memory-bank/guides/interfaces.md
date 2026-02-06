---
description: "Практическое использование CLI и MCP tools."
---

# Interfaces (Guide)

## CLI примеры
```bash
code-indexer index
code-indexer index --profile eco --throttle-ms 8
code-indexer prepare-context "where is auth token validated?" --task-hint debugging
code-indexer symbols "UserService" --kind function
code-indexer definition "UserRepository"
code-indexer references "handleRequest" --callers --depth 3
code-indexer call-graph "main" --direction out --depth 3
code-indexer stats
```

## MCP server
```bash
code-indexer serve
```

## MCP tools ориентиры
- Используйте `list_symbols` и `search_symbols` для навигации по symbols.
- Для single-shot подготовки контекста используйте `prepare_context` (или `get_context_bundle`, если нужен полный контракт input/budget).
- `find_definitions` и `find_references` покрывают go-to-definition и usages.
- `get_file_outline` и `get_imports` помогают при анализе файла.
- `get_diagnostics` и `get_stats` дают обзор качества и состояния индекса.

## Связанные материалы
[.memory-bank/architecture/interfaces.md](../architecture/interfaces.md): архитектурное описание поверхностей.
