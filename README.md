# Code-Indexer

CLI-инструмент и MCP-сервер для индексации и семантического анализа кода с использованием tree-sitter.

## Возможности

- **12 консолидированных MCP tools** для AI-агентов (Claude, GPT и др.)
- **17 языков программирования** с полной поддержкой синтаксиса
- **Semantic analysis** — scope resolution, import resolution, FQDN computation
- **Call graph с confidence** — различие между certain и possible вызовами
- **Fuzzy search** с терпимостью к опечаткам
- **Git integration** — отслеживание изменённых символов
- **Компактные форматы вывода** для экономии токенов
- **Dead code detection** — поиск неиспользуемого кода
- **Кросс-языковой анализ** — связи между Java и Kotlin
- **Workspace support** — мультимодульные проекты (Cargo, Maven, Gradle, npm)
- **Dependency indexing** — индексация зависимостей
- **Virtual documents** — поддержка несохранённых изменений (LSP overlay)

## Производительность (бенчмарк)

Тестирование на проекте JavaTgBots (2160 файлов, 18944 символа):

| Операция | code-indexer | grep | Ускорение |
|----------|--------------|------|-----------|
| Поиск определения класса | 0.007 сек | 0.539 сек | **77x** |
| Поиск реализаций интерфейса | 0.007 сек | 0.538 сек | **77x** |
| Граф вызовов метода | 0.007 сек | 0.380 сек | **54x** |
| Cross-module поиск | 0.011 сек | 0.363 сек | **33x** |
| Fuzzy поиск с опечаткой | 0.089 сек | не найдено | **∞** |

**Время индексации**: 3.5 сек для 2160 файлов

## Поддерживаемые языки (17)

| # | Язык | Расширения | Tree-sitter | Функции | Типы | Импорты | Ссылки |
|---|------|-----------|-------------|:-------:|:----:|:-------:|:------:|
| 1 | Rust | `.rs` | tree-sitter-rust 0.24 | ✅ | ✅ | ✅ | ✅ |
| 2 | Java | `.java` | tree-sitter-java 0.23 | ✅ | ✅ | ✅ | ✅ |
| 3 | Kotlin | `.kt`, `.kts` | tree-sitter-kotlin-ng 1.1 | ✅ | ✅ | ✅ | ✅ |
| 4 | TypeScript | `.ts`, `.tsx`, `.js`, `.jsx` | tree-sitter-typescript 0.23 | ✅ | ✅ | ✅ | ✅ |
| 5 | Python | `.py`, `.pyi` | tree-sitter-python 0.23 | ✅ | ✅ | ✅ | ✅ |
| 6 | Go | `.go` | tree-sitter-go 0.23 | ✅ | ✅ | ✅ | ✅ |
| 7 | C# | `.cs` | tree-sitter-c-sharp 0.23 | ✅ | ✅ | ✅ | ✅ |
| 8 | C++ | `.cpp`, `.cc`, `.hpp`, `.h` | tree-sitter-cpp 0.23 | ✅ | ✅ | ✅ | ✅ |
| 9 | SQL | `.sql` | tree-sitter-sequel 0.3 | ✅ | ✅ | — | ✅ |
| 10 | Bash | `.sh`, `.bash` | tree-sitter-bash 0.23 | ✅ | — | ✅ | ✅ |
| 11 | Lua | `.lua` | tree-sitter-lua 0.4 | ✅ | — | ✅ | ✅ |
| 12 | Swift | `.swift` | tree-sitter-swift 0.7 | ✅ | ✅ | ✅ | ✅ |
| 13 | Haskell | `.hs`, `.lhs` | tree-sitter-haskell 0.23 | ✅ | ✅ | ✅ | ✅ |
| 14 | Elixir | `.ex`, `.exs` | tree-sitter-elixir 0.3 | ✅ | ✅ | ✅ | ✅ |
| 15 | YAML | `.yml`, `.yaml` | tree-sitter-yaml 0.7 | — | ✅ | — | ✅ |
| 16 | TOML | `.toml` | tree-sitter-toml-ng 0.7 | — | ✅ | — | ✅ |
| 17 | HCL | `.tf`, `.hcl`, `.tfvars` | tree-sitter-hcl 1.1 | ✅ | ✅ | — | ✅ |

## Быстрый старт

### Установка

```bash
# Клонирование и сборка
git clone https://github.com/your-repo/code-indexer
cd code-indexer
cargo build --release

# Или установка через cargo
cargo install --path .
```

### Индексация проекта

```bash
# Индексация текущей директории
code-indexer index

# Индексация конкретного пути
code-indexer index ./src

# Индексация с отслеживанием изменений
code-indexer index --watch

# Индексация с зависимостями
code-indexer index --deep-deps
```

### Поиск символов

```bash
# Поиск символов
code-indexer symbols "MyFunction"

# Fuzzy поиск (терпимость к опечаткам)
code-indexer symbols "MyFuncton" --fuzzy

# Найти только функции
code-indexer symbols --kind function --limit 50

# Найти только типы
code-indexer symbols --kind type --language rust

# Найти определение
code-indexer definition "SymbolName"

# Найти ссылки на символ
code-indexer references "UserService" --callers
```

### Запуск MCP сервера

```bash
code-indexer serve
```

## CLI Commands

### Основные команды

| Команда | Описание | Ключевые флаги |
|---------|----------|----------------|
| `index [path]` | Индексация директории | `--watch`, `--deep-deps` |
| `serve` | Запуск MCP сервера | — |
| `symbols [query]` | Поиск и список символов | `--kind`, `--fuzzy`, `--format` |
| `definition <name>` | Найти определение | `--include-deps`, `--dep` |
| `references <name>` | Найти ссылки | `--callers`, `--depth` |
| `call-graph <func>` | Анализ графа вызовов | `--direction`, `--depth` |
| `outline <file>` | Структура файла | `--scopes`, `--start-line` |
| `imports <file>` | Импорты файла | `--resolve` |
| `changed` | Изменённые символы (git) | `--base`, `--staged` |
| `stats` | Статистика индекса | — |
| `clear` | Очистка индекса | — |
| `deps <subcmd>` | Работа с зависимостями | `list`, `index`, `find`, `info` |

### Глобальные опции

```bash
--db <path>    # Путь к базе данных (default: .code-index.db)
--help         # Справка
--version      # Версия
```

### symbols — Поиск и список символов

```bash
# Поиск по запросу
code-indexer symbols "UserService"
code-indexer symbols "User" --fuzzy --fuzzy-threshold 0.8

# Список всех символов
code-indexer symbols --kind all --limit 100

# Только функции
code-indexer symbols --kind function --language rust

# Только типы с паттерном
code-indexer symbols --kind type --pattern "User*" --format compact
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `query` | Поисковый запрос (опционально) | — |
| `--kind` | Тип: `function`, `type`, `all` | all |
| `--limit` | Максимум результатов | 100 |
| `--language` | Фильтр по языку | — |
| `--file` | Фильтр по файлу | — |
| `--pattern` | Паттерн имени (glob: `*`, `?`) | — |
| `--format` | Формат: `full`, `compact`, `minimal` | full |
| `--fuzzy` | Включить fuzzy поиск | false |
| `--fuzzy-threshold` | Порог совпадения (0.0-1.0) | 0.7 |

### definition — Найти определение

```bash
code-indexer definition "UserRepository"
code-indexer definition "HashMap" --include-deps
code-indexer definition "Serialize" --include-deps --dep "serde"
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--include-deps` | Искать в зависимостях | false |
| `--dep` | Фильтр по зависимости | — |

### references — Найти ссылки

```bash
code-indexer references "UserService"
code-indexer references "handleRequest" --callers --depth 3
code-indexer references "Config" --file "src/main.rs"
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--callers` | Включить вызывающие функции | false |
| `--depth` | Глубина поиска callers | 1 |
| `--file` | Фильтр по файлу | — |
| `--limit` | Максимум результатов | 50 |

### call-graph — Анализ графа вызовов

```bash
code-indexer call-graph "main" --depth 3
code-indexer call-graph "handleRequest" --direction both
code-indexer call-graph "processData" --direction in
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--direction` | Направление: `out`, `in`, `both` | out |
| `--depth` | Максимальная глубина | 3 |
| `--include-possible` | Включить uncertain вызовы | false |

### outline — Структура файла

```bash
code-indexer outline src/main.rs
code-indexer outline src/lib.rs --start-line 10 --end-line 50
code-indexer outline src/module.rs --scopes
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--start-line` | Начальная строка | — |
| `--end-line` | Конечная строка | — |
| `--scopes` | Включить scopes | false |

### imports — Импорты файла

```bash
code-indexer imports src/main.rs
code-indexer imports src/service.rs --resolve
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--resolve` | Разрешить импорты до определений | false |

### changed — Изменённые символы

```bash
code-indexer changed
code-indexer changed --base "main"
code-indexer changed --staged --format minimal
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--base` | Git reference для сравнения | HEAD |
| `--staged` | Только staged изменения | false |
| `--unstaged` | Только unstaged изменения | false |
| `--format` | Формат вывода | full |

### deps — Работа с зависимостями

```bash
# Список зависимостей
code-indexer deps list
code-indexer deps list --dev --format json

# Индексация зависимостей
code-indexer deps index
code-indexer deps index --name "serde"

# Поиск в зависимостях
code-indexer deps find "Serialize" --dep "serde"

# Информация о зависимости
code-indexer deps info "serde"
```

## Форматы вывода

Все команды поиска поддерживают три формата:

### Full (default)

Полная информация:

```
UserService (class) - src/UserService.java:10 [score: 1.00]
```

### Compact

JSON для программной обработки:

```json
{"n":"UserService","k":"cls","f":"src/UserService.java","l":10,"s":1.0}
```

### Minimal

Однострочный формат для максимальной компактности:

```
UserService:cls@src/UserService.java:10
```

## MCP Tools (12 консолидированных)

| # | Tool | Описание | Ключевые параметры |
|---|------|----------|-------------------|
| 1 | `index_workspace` | Индексация проекта | `path`, `watch`, `include_deps` |
| 2 | `update_files` | Virtual documents (LSP) | `files[]` с `path`, `content`, `version` |
| 3 | `list_symbols` | Список символов | `kind`, `language`, `file`, `pattern`, `limit`, `format` |
| 4 | `search_symbols` | Поиск с fuzzy/regex | `query`, `fuzzy`, `fuzzy_threshold`, `regex`, `module` |
| 5 | `get_symbol` | Получить по ID/позиции | `id` или `ids[]` или `file`+`line`+`column` |
| 6 | `find_definitions` | Найти определения | `name`, `include_deps`, `dependency` |
| 7 | `find_references` | Найти использования | `name`, `include_callers`, `include_importers`, `kind`, `depth` |
| 8 | `analyze_call_graph` | Граф вызовов | `function`, `direction`, `depth`, `confidence` |
| 9 | `get_file_outline` | Структура файла | `file`, `start_line`, `end_line`, `include_scopes` |
| 10 | `get_imports` | Импорты файла | `file`, `resolve` |
| 11 | `get_diagnostics` | Dead code, метрики | `kind`, `file`, `include_metrics`, `target` |
| 12 | `get_stats` | Статистика индекса | `detailed`, `include_workspace`, `include_deps` |

### Backward Compatibility

Старые инструменты (20+ legacy tools) работают как deprecated aliases:

- `search_symbol` → `search_symbols`
- `list_functions`, `list_types` → `list_symbols`
- `find_definition`, `find_in_dependency` → `find_definitions`
- `find_callers`, `get_file_importers` → `find_references`
- `get_call_graph`, `find_callees` → `analyze_call_graph`
- `get_file_structure`, `find_symbols_in_range` → `get_file_outline`
- `find_dead_code`, `get_metrics` → `get_diagnostics`
- `index_stats` → `get_stats`

## MCP настройка для Claude Desktop

Добавьте в `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "code-indexer": {
      "command": "/path/to/code-indexer",
      "args": ["serve"],
      "cwd": "/path/to/your/project"
    }
  }
}
```

## Анализ кода

### Call Confidence

| Уровень | Описание | Примеры |
|---------|----------|---------|
| **Certain** | 100% уверенность | Прямые вызовы, статические методы |
| **Possible** | Неопределённость | Virtual dispatch, dynamic receiver, multiple candidates |

**UncertaintyReason**: `VirtualDispatch`, `DynamicReceiver`, `MultipleCandidates`, `ExternalLibrary`, `HigherOrderFunction`

### Метрики и ранжирование

| Метрика | Описание |
|---------|----------|
| **PageRank** | Важность в графе вызовов |
| **Git Recency** | Как недавно изменялся |
| **Incoming Refs** | Количество входящих ссылок |
| **Outgoing Refs** | Количество исходящих ссылок |
| **Visibility Score** | public > protected > private |

## Архитектура

### Модули

```
src/
├── cli/commands.rs      # CLI команды
├── mcp/
│   ├── server.rs        # MCP сервер (12 tools)
│   └── consolidated.rs  # Консолидированные ответы
├── index/
│   ├── models.rs        # Symbol, Scope, Reference, CallGraph
│   ├── sqlite.rs        # SQLite хранилище + FTS5
│   └── overlay.rs       # Virtual documents
├── indexer/
│   ├── extractor.rs     # Извлечение символов из AST
│   ├── call_analyzer.rs # Анализ confidence вызовов
│   ├── scope_builder.rs # Построение иерархии scopes
│   ├── resolver.rs      # Разрешение идентификаторов
│   ├── import_resolver.rs # Разрешение импортов
│   ├── walker.rs        # Обход файлов
│   └── watcher.rs       # File watching
├── languages/           # 17 языковых модулей
└── git/mod.rs           # Git интеграция
```

### Модели данных

```rust
Symbol {
    id, name, kind, location, language,
    visibility, signature, doc_comment,
    parent, scope_id, fqdn
}

Scope {
    id, file_path, parent_id, kind,
    name, start_offset, end_offset
}

SymbolReference {
    symbol_id, symbol_name, file_path,
    line, column, kind (Call/TypeUse/Import/Extend)
}

CallGraph {
    nodes: Vec<CallGraphNode>,
    edges: Vec<CallGraphEdge> // с confidence
}
```

### SQLite схема (9 таблиц)

- `symbols` — символы + FTS5 индекс
- `scopes` — иерархия областей видимости
- `symbol_references` — ссылки
- `call_edges` — граф вызовов с confidence
- `file_imports` — импорты
- `projects` — проекты
- `dependencies` — зависимости
- `dependency_symbols` — символы из зависимостей
- `symbol_metrics` — метрики (PageRank, git recency)

## Текущие ограничения

1. **Нет type inference** — для Python/JS вызовы часто Possible
2. **Нет межпроектных ссылок** — только внутри workspace
3. **Ограниченная поддержка generics** — template instantiation не отслеживается
4. **Нет инкрементального парсинга** — весь файл перепарсивается
5. **SQLite single-writer** — нет параллельной записи

## Лицензия

MIT License
