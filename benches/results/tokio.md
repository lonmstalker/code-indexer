# tokio (Rust)

> Асинхронный runtime для Rust — [tokio-rs/tokio](https://github.com/tokio-rs/tokio)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 695 (Rust) |
| Символов извлечено | 7981 |
| Время индексации | 1.95 сек |
| Размер .code-index.db | 43 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `spawn` | 0.008 сек, 18 определений (с сигнатурами) | 0.016 сек, 21 совпадение | **2x** + сигнатуры и типы |
| Найти определение типа `Runtime` | 0.008 сек, 2 определения | 0.016 сек, 2 совпадения | **2x** + точная семантика |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `JoinHandle` | 50 ref (50 TypeUse) за 0.009 сек | 310 строк за 0.017 сек | Классификация: только TypeUse, без шума |
| Кто вызывает функцию `spawn` | 100+ callers с локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callees функции `run` | 6 вызовов (depth=3) | **Невозможно** | Уникальная возможность |
| Callers `spawn` (depth=2) | 100 callers | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `tokio/tests/sync_mpsc.rs` | 100 символов — иерархия с parent, line ranges | `rg "(fn\|struct)"` → 100 строк (плоский список) | Иерархия: struct → impl → method |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `Runtme` | Находит `Runtime` [score: 0.97] — 2 определения | `rg "\bRuntme\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные импорты | `rg "^use "` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 6757 (2656 function + 4101 method) |
| Типы (struct/enum/trait) | YES | 1224 (706 struct + 91 enum + 63 trait + 364 type_alias) |
| Импорты | YES | Структурированные `use` с путями модулей |
| Ссылки (Call/TypeUse/Extend) | YES | Классификация каждой ссылки по типу |
| Generics | NO | 0 символов (известное ограничение: Rust generic params пока не извлекаются) |
| Параметры с типами | YES | 4428 символов с типизированными параметрами |
| Return types | YES | 4189 символов с return type |
| Visibility (pub/private) | YES | 2606 символов с visibility |
| Doc comments | YES | 1777 символов с doc comments |

## Воспроизведение

```bash
rm -f benches/repos/tokio/.code-index.db
time code-indexer --db benches/repos/tokio/.code-index.db index benches/repos/tokio
code-indexer --db benches/repos/tokio/.code-index.db stats
time code-indexer --db benches/repos/tokio/.code-index.db definition "spawn"
time rg "fn\s+spawn\b" benches/repos/tokio
code-indexer --db benches/repos/tokio/.code-index.db symbols "Runtme" --fuzzy
```
