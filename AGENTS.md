# AGENTS — code-indexer (Codex)

## Назначение
Этот файл описывает правила и контекст для Codex при работе с репозиторием `code-indexer`.

## Короткий контекст
`code-indexer` — CLI и MCP server для индексации и семантического анализа кода на базе `tree-sitter`.

## Где искать архитектуру и гайды
Основной источник архитектурного контекста: `.memory-bank/index.md`.
Если меняются архитектура, интерфейсы или пайплайн — обновить `.memory-bank/`.

## Основные зоны кода
- `src/cli` — CLI команды и аргументы.
- `src/indexer` — индексирование и извлечение символов.
- `src/index` — SQLite слой и схема.
- `src/mcp` — MCP server и tools.
- `src/languages` — registry и tree-sitter грамматики.
- `src/workspace` и `src/dependencies` — workspace и deps indexing.
- `tests/` — интеграционные тесты и сценарии MCP/CLI.
- `examples/*` — учебные проекты разных экосистем.

## Команды (базовые)
- Сборка: `cargo build --release`
- Тесты: `cargo test`

## Принципы работы
- Сначала изучить `.memory-bank/index.md`.
- Не менять `target/` и артефакты, если не попросили.
- Не трогать примерные проекты в `examples/*`, если задача не про них.
