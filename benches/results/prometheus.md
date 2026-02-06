# prometheus (Go)

> [!WARNING]
> Legacy speed data: этот файл сохранён как historical capability snapshot и не является источником честных speed-метрик.
> Для honest speed-check используйте 'benches/speed/run_speed_bench.py' и отчёты в 'benches/results/speed/'.

> Система мониторинга и алертинга — [prometheus/prometheus](https://github.com/prometheus/prometheus)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 804 (628 Go + 172 TypeScript + 4 Bash) |
| Символов извлечено | 12039 |
| Время индексации | 4.39 сек |
| Размер .code-index.db | 137 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `NewDiscovery` | 0.009 сек, 23 определения (с сигнатурами и return types) | 0.027 сек, 23 совпадения | **3x** + сигнатуры |
| Найти определение типа `SDConfig` | 0.008 сек, 24 определения (struct + type_alias) | 0.028 сек, 24 совпадения | **3.5x** + семантика |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `Manager` | 50 ref (50 TypeUse) за 0.008 сек | 136 строк за 0.025 сек | Классификация: TypeUse без шума комментариев |
| Кто вызывает `NewDiscovery` | 56 callers с точными локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callees `NewDiscovery` | 4 вызова (depth=3) | **Невозможно** | Уникальная возможность |
| Callers `NewDiscovery` (depth=2) | 56 callers | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `prompb/types.pb.go` | 292 символа — иерархия с parent, line ranges | `rg "(func\|type)"` → 40 строк | **7.3x** больше символов: struct → method |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `NewDsc` | Находит `NewDiscovery` [score: 0.90] — 23 определения | `rg "\bNewDsc\b"` → 0 результатов | Толерантность к опечаткам |
| Поиск с опечаткой `Discovry` | Находит `Discovery` [score: 0.98] — 23 struct | `rg "\bDiscovry\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные импорты | `rg "^import"` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 8939 (4439 function + 4500 method) |
| Типы (struct/interface) | YES | 3100 (1154 struct + 319 interface + 1602 type_alias + 15 enum + 10 class) |
| Импорты | YES | Структурированные `import` блоки |
| Ссылки (Call/TypeUse) | YES | Классификация каждой ссылки |
| Generics (Go 1.18+) | YES | 48 символов с generic параметрами |
| Параметры с типами | YES | 5898 символов с типизированными параметрами |
| Return types | YES | 5557 символов с return type |
| Visibility (exported/unexported) | YES | 21 символ с explicit visibility |
| Doc comments | NO | 0 (Go doc comments не извлекаются) |

## Воспроизведение

```bash
rm -f benches/repos/prometheus/.code-index.db
time code-indexer --db benches/repos/prometheus/.code-index.db index benches/repos/prometheus
code-indexer --db benches/repos/prometheus/.code-index.db stats
time code-indexer --db benches/repos/prometheus/.code-index.db definition "NewDiscovery"
time rg "func\s+NewDiscovery\b" benches/repos/prometheus
code-indexer --db benches/repos/prometheus/.code-index.db symbols "Discovry" --fuzzy
```
