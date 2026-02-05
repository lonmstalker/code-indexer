---
description: "Базовый запуск code-indexer и MCP server."
---

# Overview (Guide)

## Быстрый запуск
```bash
cargo build --release
./target/release/code-indexer index
```

## Запуск MCP server
```bash
./target/release/code-indexer serve
```

## База данных индекса
- По умолчанию `--db` = `.code-index.db`.
- В `index_directory` при дефолтном `--db` файл БД помещается внутрь индексируемого пути.

## Логи
- Уровень логирования управляется через `RUST_LOG` (используется `tracing_subscriber::EnvFilter`).
- Дефолт: `code_indexer=info`.

## Связанные материалы
[.memory-bank/architecture/overview.md](../architecture/overview.md): системный контекст и ключевые контейнеры.
