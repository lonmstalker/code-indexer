# Code-Indexer

CLI-инструмент и MCP-сервер для индексации и семантического анализа кода с использованием tree-sitter.

## Возможности

- **24 MCP tools** для AI-агентов (Claude, GPT, Codex и др.)
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
- **File Tags & Intent Layer** — метаданные файлов через sidecar `.code-indexer.yml`
- **Incremental indexing by default** — skip unchanged файлов по `content_hash` + cleanup stale файлов

## Производительность (честный speed-check v2)

В репозитории разделены два типа benchmark:

- **Capability checks (legacy)** — функциональные сравнения в `benches/results/*.md` (historical snapshot).
- **Honest speed checks (v2)** — воспроизводимые speed-замеры `code-indexer` vs `rg` через `benches/speed/run_speed_bench.py`.

Методология speed-check v2:

- только definition-like кейсы из `benches/speed/cases.json`;
- strict precheck: `code_indexer_count == rg_count`;
- метрики: `median`, `p95`, `cv%`;
- режимы: `query-only` и `first-run`.

Подробности и команды запуска: [benches/README.md](benches/README.md) и [benches/results/speed/README.md](benches/results/speed/README.md).

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
# Быстрая установка из GitHub Release (latest)
curl -fsSL https://raw.githubusercontent.com/lonmstalker/code-indexer/master/scripts/install.sh | sh

# Установка конкретной версии
curl -fsSL https://raw.githubusercontent.com/lonmstalker/code-indexer/master/scripts/install.sh | CODE_INDEXER_VERSION=v0.1.0 sh

# Установка в кастомную директорию
curl -fsSL https://raw.githubusercontent.com/lonmstalker/code-indexer/master/scripts/install.sh | INSTALL_DIR="$HOME/bin" sh

# Сборка из исходников
git clone https://github.com/lonmstalker/code-indexer
cd code-indexer
cargo build --release

# Локальная установка через cargo
cargo install --path .
```

Release assets:
- `code-indexer-aarch64-apple-darwin.tar.gz`
- `code-indexer-x86_64-apple-darwin.tar.gz`
- `checksums-v0.1.0.txt`

Примечание: в `v0.1.0` опубликованы только macOS артефакты.

### Docker

```bash
# Pull готового образа
docker pull lonmstalkerd/code-indexer:latest

# Или собрать локально из Dockerfile
docker build -t code-indexer:local .

# Использование CLI через контейнер (рекомендуется фиксировать --db в /workspace)
docker run --rm -v "$PWD:/workspace" -w /workspace \
  lonmstalkerd/code-indexer:latest --db /workspace/.code-index.db index

docker run --rm -v "$PWD:/workspace" -w /workspace \
  lonmstalkerd/code-indexer:latest --db /workspace/.code-index.db stats
```

### Индексация проекта

CLI показывает реалтайм progress bar с ETA:

```
⠋ [################>-----------------------] 847/2160 (39%) | 00:01:23 ETA 00:02:05 | indexing...
```

По умолчанию `index` работает инкрементально:
- unchanged файлы пропускаются (`content_hash` parity);
- удалённые из workspace файлы удаляются из индекса;
- полный rebuild можно принудительно сделать удалением `.code-index.db`.

```bash
# Индексация текущей директории
code-indexer index

# Индексация конкретного пути
code-indexer index ./src

# Индексация с отслеживанием изменений
code-indexer index --watch

# Индексация с зависимостями
code-indexer index --deep-deps

# Профиль durability для bulk-индексации
code-indexer index --durability fast
code-indexer index --durability safe

# Термопрофили (по умолчанию: balanced, до 4 потоков)
code-indexer index --profile eco
code-indexer index --profile balanced
code-indexer index --profile max

# Ручное ограничение CPU-параллелизма и мягкий throttle
code-indexer index --threads 2 --throttle-ms 8 --durability safe

# Подготовка AI-ready контекста одной командой
code-indexer prepare-context "where is auth token validated?" \
  --file src/auth/middleware.rs \
  --task-hint debugging \
  --agent-timeout-sec 60 \
  --agent-max-steps 6
# токен/провайдер берутся из .code-indexer.yml -> agent.*, токен можно через env
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

# Выполнить query через запущенный daemon
code-indexer definition "SymbolName" --remote /tmp/code-indexer.sock
```

### Запуск MCP сервера

```bash
code-indexer serve

# daemon на unix socket
code-indexer serve --transport unix --socket /tmp/code-indexer.sock
```

## CLI Commands

### Основные команды

| Команда | Описание | Ключевые флаги |
|---------|----------|----------------|
| `index [path]` | Индексация директории | `--watch`, `--deep-deps`, `--durability`, `--profile`, `--threads`, `--throttle-ms` |
| `prepare-context <query>` | Agent-only сбор контекста по задаче | `--file`, `--task-hint`, `--max-items`, `--agent-timeout-sec`, `--agent-max-steps`, `--agent-include-trace`, `--remote` |
| `serve` | Запуск MCP сервера | `--transport`, `--socket` |
| `symbols [query]` | Поиск и список символов | `--kind`, `--fuzzy`, `--format`, `--remote` |
| `definition <name>` | Найти определение | `--include-deps`, `--dep`, `--remote` |
| `references <name>` | Найти ссылки | `--callers`, `--depth`, `--remote` |
| `call-graph <func>` | Анализ графа вызовов | `--direction`, `--depth`, `--remote` |
| `outline <file>` | Структура файла | `--scopes`, `--start-line`, `--remote` |
| `imports <file>` | Импорты файла | `--resolve`, `--remote` |
| `changed` | Изменённые символы (git) | `--base`, `--staged` |
| `stats` | Статистика индекса | `--remote` |
| `clear` | Очистка индекса | — |
| `deps <subcmd>` | Работа с зависимостями | `list`, `index`, `find`, `info` |
| `tags <subcmd>` | Управление тегами | `add-rule`, `remove-rule`, `list-rules`, `preview`, `apply`, `stats` |

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

## MCP Tools (24)

| # | Tool | Описание | Ключевые параметры |
|---|------|----------|-------------------|
| 1 | `index_workspace` | Индексация проекта | `path`, `watch`, `include_deps` |
| 2 | `update_files` | Virtual documents (LSP) | `files[]` с `path`, `content`, `version` |
| 3 | `list_symbols` | Список символов | `kind`, `language`, `file`, `pattern`, `limit`, `format` |
| 4 | `search_symbols` | Поиск с fuzzy/regex | `query`, `fuzzy`, `fuzzy_threshold`, `regex`, `module`, `tag`, `include_file_meta` |
| 5 | `get_symbol` | Получить по ID/позиции | `id` или `ids[]` или `file`+`line`+`column` |
| 6 | `find_definitions` | Найти определения | `name`, `include_deps`, `dependency` |
| 7 | `find_references` | Найти использования | `name`, `include_callers`, `include_importers`, `kind`, `depth` |
| 8 | `analyze_call_graph` | Граф вызовов | `function`, `direction`, `depth`, `confidence` |
| 9 | `get_file_outline` | Структура файла | `file`, `start_line`, `end_line`, `include_scopes`, `include_file_meta` |
| 10 | `get_imports` | Импорты файла | `file`, `resolve` |
| 11 | `get_diagnostics` | Dead code, метрики | `kind`, `file`, `include_metrics`, `target` |
| 12 | `get_stats` | Статистика индекса | `detailed`, `include_workspace`, `include_deps` |
| 13 | `get_context_bundle` | Summary-first контекст | `input`, `budget`, `format`, `agent` |
| 14 | `prepare_context` | Agent-only orchestration context collection | `query`, `file`, `task_hint`, `max_items`, `agent_timeout_ms`, `agent_max_steps`, `include_trace`, `agent` |
| 22 | `manage_tags` | Управление tag inference | `action`, `pattern`, `tags`, `confidence`, `file`, `path` |
| 24 | `get_indexing_status` | Прогресс индексации | — (без параметров) |

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

Вариант через Docker image:

```json
{
  "mcpServers": {
    "code-indexer-docker": {
      "command": "docker",
      "args": [
        "run",
        "--rm",
        "-i",
        "-v",
        "/path/to/your/project:/workspace",
        "-w",
        "/workspace",
        "lonmstalkerd/code-indexer:latest",
        "--db",
        "/workspace/.code-index.db",
        "serve"
      ]
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
│   ├── server.rs        # MCP сервер (23 tools)
│   └── consolidated.rs  # Консолидированные параметры tools
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
│   ├── sidecar.rs       # Парсинг .code-indexer.yml и staleness detection
│   ├── progress.rs      # Shared progress state (atomic counters)
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

FileMeta {
    file_path, doc1, purpose, capabilities,
    invariants, non_goals, security_notes,
    owner, stability, exported_hash,
    source (Sidecar|Explicit|Inferred), confidence
}
```

### File Tags & Intent Layer

Sidecar-файлы `.code-indexer.yml` позволяют добавлять метаданные к файлам:

```yaml
directory_tags:
  - domain:auth
  - layer:service

files:
  service.rs:
    doc1: "Единая точка аутентификации с JWT и OAuth2"
    purpose: "Централизует логику выдачи и валидации токенов"
    capabilities: [jwt_generation, oauth2_flow]
    invariants: ["refresh_token хранится как hash"]
    stability: stable
    tags: [pattern:idempotency]
```

**Возможности:**
- **Tag search**: `search_symbols(tag=["domain:auth", "layer:service"])`
- **File meta**: `get_file_outline(include_file_meta=true)` — doc1, purpose, tags
- **Staleness detection**: предупреждение при изменении публичного API
- **Tag dictionary**: нормализация через синонимы (authn → auth)

### Tag Inference Rules

Автоматический вывод тегов на основе путей файлов через glob-паттерны.

**Формат в корневом `.code-indexer.yml`:**

```yaml
agent:
  provider: openrouter
  model: openrouter/auto
  endpoint: https://openrouter.ai/api/v1
  api_key_env: OPENROUTER_API_KEY
  mode: planner

tag_rules:
  - pattern: "**/auth/**"
    tags: [domain:auth]
    confidence: 0.8

  - pattern: "**/service/**"
    tags: [layer:service]

  - pattern: "**/*_test.*"
    tags: [infra:test]
    confidence: 0.9

  - pattern: "**/api/**/*.ts"
    tags: [layer:api, lang:typescript]

# Остальные поля sidecar
directory_tags: []
files: {}
```

`prepare-context` (CLI и MCP tool `prepare_context`) теперь работает только в agent-режиме и читает `agent.*` из корневого `.code-indexer.yml`.  
Если валидный агент-конфиг не найден, вызов завершится ошибкой.  
Детерминированный non-agent путь для контекста остаётся в отдельном tool `get_context_bundle`.

`prepare_context` возвращает только собранный контекст и связи:
- `task_context` (module/file/symbol/deps/docs слои)
- `coverage` и `gaps` (без silent truncation)
- `collection_meta` (шаги/время/usage, опционально trace)

План/summary от агента не формируется и не возвращается.

Токен аутентификации поддерживается двумя способами:
- `agent.api_key_env` (рекомендуется)
- `agent.api_key` (прямо в config, менее безопасно)

Для `provider: local` (включая vibeproxy-gateway) auth может быть опциональным, если endpoint не требует bearer token.

Если `api_key_env` не указан, используется provider-default:
- `openai` -> `OPENAI_API_KEY`
- `anthropic` -> `ANTHROPIC_API_KEY`
- `openrouter` -> `OPENROUTER_API_KEY`
- `local` -> `LOCAL_LLM_API_KEY`

### Примеры agent-конфига для провайдеров

```yaml
# OpenAI
agent:
  provider: openai
  model: gpt-4o-mini
  endpoint: https://api.openai.com/v1
  api_key_env: OPENAI_API_KEY
  mode: planner
```

```yaml
# Anthropic
agent:
  provider: anthropic
  model: claude-3-5-sonnet-latest
  endpoint: https://api.anthropic.com
  api_key_env: ANTHROPIC_API_KEY
  mode: planner
```

```yaml
# OpenRouter
agent:
  provider: openrouter
  model: openrouter/auto
  endpoint: https://openrouter.ai/api/v1
  api_key_env: OPENROUTER_API_KEY
  mode: planner
```

```yaml
# Local gateway (Ollama/vLLM/TGI/proxy)
agent:
  provider: local
  model: gpt-5.2
  endpoint: http://127.0.0.1:11434/v1
  api_key_env: LOCAL_LLM_API_KEY
  mode: planner
```

```yaml
# Vibeproxy (OpenAI-compatible routing)
agent:
  provider: local
  model: gpt-5.2
  endpoint: https://<your-vibeproxy-endpoint>/v1
  api_key_env: VIBEPROXY_API_KEY
  mode: planner
```

**CLI команды:**

```bash
# Добавить правило
code-indexer tags add-rule "domain:auth" --pattern "**/auth/**" --confidence 0.8

# Удалить правило
code-indexer tags remove-rule --pattern "**/auth/**"

# Список всех правил
code-indexer tags list-rules

# Preview: какие теги будут применены к файлу
code-indexer tags preview ./src/auth/service.rs

# Применить правила к индексу
code-indexer tags apply

# Статистика тегов
code-indexer tags stats
```

**MCP tool `manage_tags`:**

```json
{
  "action": "add_rule",
  "pattern": "**/auth/**",
  "tags": ["domain:auth"],
  "confidence": 0.8,
  "path": "."
}

{
  "action": "preview",
  "file": "src/auth/service.rs"
}

{
  "action": "apply"
}

{
  "action": "stats"
}
```

**Поддерживаемые actions**: `add_rule`, `remove_rule`, `list_rules`, `preview`, `apply`, `stats`

### Search Diversification

Параметр `max_per_directory` ограничивает число результатов из одной директории для разнообразия:

```json
{
  "query": "handler",
  "max_per_directory": 2,
  "limit": 20
}
```

Результаты будут включать максимум 2 символа из каждой директории.

### SQLite схема (12 таблиц)

- `symbols` — символы + FTS5 индекс
- `scopes` — иерархия областей видимости
- `symbol_references` — ссылки
- `call_edges` — граф вызовов с confidence
- `file_imports` — импорты
- `projects` — проекты
- `dependencies` — зависимости
- `dependency_symbols` — символы из зависимостей
- `symbol_metrics` — метрики (PageRank, git recency)
- `tag_dictionary` — словарь тегов с категориями и синонимами
- `file_meta` — метаданные файлов (doc1, purpose, capabilities, invariants)
- `file_tags` — связи файл-тег + FTS5 для поиска по doc1/purpose

## Текущие ограничения

1. **Нет межпроектных ссылок** — только внутри workspace (планируется: cross-project links)
2. **Template instantiation** — конкретные инстанциации generic типов не отслеживаются (планируется: generic resolver)

### Улучшения производительности

- **WriteQueue для SQLite** — сериализация записей через tokio mpsc channel, предотвращает SQLITE_BUSY ошибки при concurrent writes
- **Batch writes** — все символы, ссылки и импорты вставляются в одной транзакции (2-3x ускорение)
- **Batch deletes** — `remove_file()` использует IN clause (3 запроса вместо 3N), `remove_files_batch()` удаляет несколько файлов в одной транзакции
- **Batch updates** — `mark_dependencies_indexed_batch()` и `add_doc_digests_batch()` для групповых операций
- **Incremental parsing** — `ParseCache` переиспользует старые деревья tree-sitter (30-50% ускорение для watch mode)
- **Type-aware call resolution** — `filter_by_type()` использует аннотации типов для disambiguation вызовов методов

### Извлечение типов параметров (9 языков)

| Язык | Поддержка | Примечания |
|------|:---------:|-----------|
| Python | ✅ | typed_parameter, typed_default_parameter |
| TypeScript | ✅ | required_parameter, type_annotation |
| Rust | ✅ | self_parameter, parameter with type |
| Java | ✅ | formal_parameter, spread_parameter |
| Go | ✅ | parameter_declaration, variadic_parameter_declaration |
| C++ | ✅ | parameter_declaration, pointer_declarator |
| C# | ✅ | parameter, predefined_type, nullable_type |
| Swift | ✅ | parameter (direct children), user_type |
| Kotlin | ✅ | function_value_parameter, parameter |

### Извлечение generic параметров (6 языков)

| Язык | Поддержка | Примечания |
|------|:---------:|-----------|
| Rust | ✅ | type_parameters, constrained_type_parameter, trait_bounds |
| TypeScript | ✅ | type_parameters, constraint, default_type |
| Java | ✅ | type_parameters, type_bound |
| Go | ✅ | type_parameter_list, type_parameter_declaration |
| C# | ✅ | type_parameter_list |
| Kotlin | ✅ | type_parameters, type_parameter |

Generic параметры сохраняются в поле `generic_params` символа и хранятся в БД как JSON в колонке `generic_params_json`.

## Сравнение с CLI-инструментами

### Capability checks (legacy)

`benches/results/*.md` содержат historical capability snapshot (семантика, граф вызовов, fuzzy, outline).  
Эти файлы сохранены для функционального сравнения и не используются как источник честных speed-метрик.

| Репо | Язык | Legacy capability |
|------|------|-------------------|
| ripgrep | Rust | [benches/results/ripgrep.md](benches/results/ripgrep.md) |
| tokio | Rust | [benches/results/tokio.md](benches/results/tokio.md) |
| excalidraw | TypeScript | [benches/results/excalidraw.md](benches/results/excalidraw.md) |
| guava | Java | [benches/results/guava.md](benches/results/guava.md) |
| prometheus | Go | [benches/results/prometheus.md](benches/results/prometheus.md) |
| django | Python | [benches/results/django.md](benches/results/django.md) |
| kotlin | Kotlin | [benches/results/kotlin.md](benches/results/kotlin.md) |

### Honest speed checks (v2)

Честные speed-замеры находятся в `benches/speed/run_speed_bench.py` и публикуются в `benches/results/speed/`.

- baseline: только `rg`;
- strict parity по количеству результатов;
- speedup считается только для valid parity-case.

```bash
# Скачивание pinned репозиториев (все 7)
./benches/download_repos.sh

# Сборка
cargo build --release

# Honest speed benchmark (JSON + Markdown)
python3 benches/speed/run_speed_bench.py \
  --repos all \
  --mode both \
  --runs 10 \
  --warmup 3 \
  --out-json benches/results/speed/latest.json \
  --out-md benches/results/speed/latest.md
```

Smoke-набор (CI): `ripgrep,tokio`.

Подробнее: [benches/README.md](benches/README.md), [benches/results/speed/README.md](benches/results/speed/README.md)

## Лицензия

MIT License
