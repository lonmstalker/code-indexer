---
description: "Обслуживание SQLite индекса и сценарии in-memory."
---

# Data Store (Guide)

## Путь к БД
- По умолчанию используется `.code-index.db`.
- Флаг `--db` задаёт альтернативный путь.

## Обслуживание
```bash
code-indexer stats
code-indexer clear
```

## In-memory (для тестов)
- В тестах используется `SqliteIndex::in_memory()`.
- Это полезно для быстрых unit-test без файлового SQLite.

## Связанные материалы
[.memory-bank/architecture/data-store.md](../architecture/data-store.md): архитектура хранилища.
