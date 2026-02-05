---
description: "Сценарии индексирования, watch, deps и db path."
---

# Indexing Pipeline (Guide)

## Базовая индексация
```bash
code-indexer index
code-indexer index ./src
```

## Watch mode
```bash
code-indexer index ./src --watch
```

## Индексация зависимостей
- Глубокая индексация вместе с проектом:
```bash
code-indexer index --deep-deps
```
- Отдельно по deps:
```bash
code-indexer deps index
code-indexer deps index --name "serde"
```

## Поиск в зависимостях
```bash
code-indexer deps find "Serialize" --dep "serde"
```

## База данных
- Глобальный флаг `--db` задаёт путь к SQLite файлу.
- При дефолтном `--db` индекс сохраняется в `.code-index.db` внутри индексируемого пути.

## Связанные материалы
[.memory-bank/architecture/indexing-pipeline.md](../architecture/indexing-pipeline.md): архитектура пайплайна и роли компонентов.
