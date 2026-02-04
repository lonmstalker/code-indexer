# Code Indexer

CLI-инструмент и MCP-сервер для индексации и поиска кода с использованием tree-sitter.

## Возможности

- **12 консолидированных MCP tools** для AI-агентов (Claude, GPT и др.)
- **8 языков программирования** с полной поддержкой синтаксиса
- **Semantic analysis** - scope resolution, import resolution
- **Call graph с confidence** - различие между certain и possible вызовами
- **Fuzzy search** с терпимостью к опечаткам
- **Git integration** - отслеживание изменённых символов
- **Компактные форматы вывода** для экономии токенов
- **Dead code detection** - поиск неиспользуемого кода
- **Кросс-языковой анализ** - связи между Java и Kotlin
- **Workspace support** - мультимодульные проекты
- **Dependency indexing** - индексация зависимостей
- **Virtual documents** - поддержка несохранённых изменений

## Поддерживаемые языки

| Язык | Расширения |
|------|-----------|
| Rust | `.rs` |
| Java | `.java` |
| Kotlin | `.kt`, `.kts` |
| TypeScript | `.ts`, `.tsx`, `.js`, `.jsx` |
| Python | `.py`, `.pyi` |
| Go | `.go` |
| C# | `.cs` |
| C++ | `.cpp`, `.cc`, `.hpp`, `.h` |

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

| Команда | Описание |
|---------|----------|
| `index [path]` | Индексация директории |
| `serve` | Запуск MCP сервера |
| `symbols [query]` | Поиск и список символов |
| `definition <name>` | Найти определение |
| `references <name>` | Найти ссылки |
| `call-graph <function>` | Анализ графа вызовов |
| `outline <file>` | Структура файла |
| `imports <file>` | Импорты файла |
| `changed` | Изменённые символы (git) |
| `stats` | Статистика индекса |
| `clear` | Очистка индекса |
| `deps <subcommand>` | Работа с зависимостями |

### Глобальные опции

```bash
--db <path>    # Путь к базе данных (default: .code-index.db)
--help         # Справка
--version      # Версия
```

### symbols - Поиск и список символов

Объединяет функции поиска, списка функций и типов.

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
| `query` | Поисковый запрос (опционально) | - |
| `--kind` | Тип: `function`, `type`, `all` | all |
| `--limit` | Максимум результатов | 100 |
| `--language` | Фильтр по языку | - |
| `--file` | Фильтр по файлу | - |
| `--pattern` | Паттерн имени (glob: `*`, `?`) | - |
| `--format` | Формат: `full`, `compact`, `minimal` | full |
| `--fuzzy` | Включить fuzzy поиск | false |
| `--fuzzy-threshold` | Порог совпадения (0.0-1.0) | 0.7 |

### definition - Найти определение

```bash
code-indexer definition "UserRepository"
code-indexer definition "HashMap" --include-deps
code-indexer definition "Serialize" --include-deps --dep "serde"
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--include-deps` | Искать в зависимостях | false |
| `--dep` | Фильтр по зависимости | - |

### references - Найти ссылки

```bash
code-indexer references "UserService"
code-indexer references "handleRequest" --callers --depth 3
code-indexer references "Config" --file "src/main.rs"
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--callers` | Включить вызывающие функции | false |
| `--depth` | Глубина поиска callers | 1 |
| `--file` | Фильтр по файлу | - |
| `--limit` | Максимум результатов | 50 |

### call-graph - Анализ графа вызовов

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

### outline - Структура файла

```bash
code-indexer outline src/main.rs
code-indexer outline src/lib.rs --start-line 10 --end-line 50
code-indexer outline src/module.rs --scopes
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--start-line` | Начальная строка | - |
| `--end-line` | Конечная строка | - |
| `--scopes` | Включить scopes | false |

### imports - Импорты файла

```bash
code-indexer imports src/main.rs
code-indexer imports src/service.rs --resolve
```

| Параметр | Описание | Default |
|----------|----------|---------|
| `--resolve` | Разрешить импорты до определений | false |

### changed - Изменённые символы

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

### deps - Работа с зависимостями

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

Все команды поиска поддерживают три формата вывода:

### Full (default)

Полная информация.

```
UserService (class) - src/UserService.java:10 [score: 1.00]
```

### Compact

JSON для программной обработки.

```json
{"n":"UserService","k":"cls","f":"src/UserService.java","l":10,"s":1.0}
```

### Minimal

Однострочный формат для максимальной компактности.

```
UserService:cls@src/UserService.java:10
```

## MCP Tools (12 консолидированных инструментов)

| # | Tool | Описание |
|---|------|----------|
| 1 | `index_workspace` | Индексация workspace с конфигом |
| 2 | `update_files` | Обновление файлов (virtual docs) |
| 3 | `list_symbols` | Список символов с фильтрами |
| 4 | `search_symbols` | Поиск символов (fuzzy, regex, ranking) |
| 5 | `get_symbol` | Получить символ по ID или позиции |
| 6 | `find_definitions` | Найти определения |
| 7 | `find_references` | Найти использования (включая callers) |
| 8 | `analyze_call_graph` | Call graph с confidence |
| 9 | `get_file_outline` | Структура файла |
| 10 | `get_imports` | Импорты файла |
| 11 | `get_diagnostics` | Диагностика (dead code и др.) |
| 12 | `get_stats` | Статистика индекса |

### Backward Compatibility

Старые инструменты (36 tools) работают как deprecated aliases:
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

## Примеры использования с AI агентами

### Поиск и навигация

```
Human: Найди определение функции handleRequest
AI: [использует find_definitions с name="handleRequest"]
```

### Анализ изменений

```
Human: Какие функции я изменил с последнего коммита?
AI: [использует get_stats с git changes]
```

### Рефакторинг

```
Human: Кто использует класс UserService?
AI: [использует find_references с name="UserService"]
```

### Архитектурный анализ

```
Human: Покажи граф вызовов функции main
AI: [использует analyze_call_graph с function="main"]
```

### Поиск мёртвого кода

```
Human: Есть ли неиспользуемые функции?
AI: [использует get_diagnostics с kind="dead_code"]
```

## Структура базы данных

База данных `.code-index.db` хранит:

- **symbols** - все символы (с scope_id и fqdn)
- **scopes** - иерархия scope (file, module, class, function, block)
- **files** - проиндексированные файлы
- **references** - ссылки между символами
- **call_edges** - граф вызовов с confidence
- **symbol_metrics** - метрики для ranking
- **projects** - информация о проектах
- **dependencies** - зависимости проектов
- **dependency_symbols** - символы из зависимостей

## Лицензия

MIT License
