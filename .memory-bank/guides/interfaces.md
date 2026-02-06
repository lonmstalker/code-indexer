---
description: "Практическое использование CLI и MCP tools."
---

# Interfaces (Guide)

## CLI примеры
```bash
code-indexer index
code-indexer index --profile eco --throttle-ms 8
code-indexer prepare-context "where is auth token validated?" --task-hint debugging --agent-timeout-sec 60 --agent-max-steps 6
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
- Для single-shot подготовки контекста используйте `prepare_context` в agent-only режиме.  
  Routing провайдера/модели берётся из корневого `.code-indexer.yml` (`agent.*`), при необходимости `agent` можно передать в запросе явно.
  Без валидного agent-конфига вызов `prepare_context` завершится ошибкой.
  Для `provider: local` bearer-токен может быть необязателен, если gateway работает без auth.
  Для deterministic/non-agent контекста используйте `get_context_bundle`.
- `prepare_context` не возвращает agent-plan/summary: только контекстные слои (`task_context`) + `coverage/gaps` + `collection_meta`.
- `find_definitions` и `find_references` покрывают go-to-definition и usages.
- `get_file_outline` и `get_imports` помогают при анализе файла.
- `get_diagnostics` и `get_stats` дают обзор качества и состояния индекса.

## Связанные материалы
[.memory-bank/architecture/interfaces.md](../architecture/interfaces.md): архитектурное описание поверхностей.
