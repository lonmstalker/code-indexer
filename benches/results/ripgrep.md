# ripgrep (Rust)

> Быстрый рекурсивный поиск по содержимому файлов — [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 84 (83 Rust + 1 Bash) |
| Символов извлечено | 3171 |
| Время индексации | 0.86 сек |
| Размер .code-index.db | 14 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `new` | 0.010 сек, 84 определения (с сигнатурами) | 0.012 сек, 84 совпадения (raw text) | **~1x** (оба мгновенны, но code-indexer даёт сигнатуры) |
| Найти определение типа `Searcher` | 0.009 сек, 1 определение | 0.010 сек, 1 совпадение | **~1x** (code-indexer точнее: struct vs regex) |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `Regex` | 24 ref (6 Call / 18 TypeUse) | 67 строк (включая комментарии, строки) | Классификация ссылок: Call vs TypeUse |
| Кто вызывает функцию `new` | 200+ Call-ссылок, 100 callers | **Невозможно** — rg не различает определение и вызов | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callees функции `search` | 7 вызовов (depth=3) | **Невозможно** | Уникальная возможность |
| Callers `search` (depth=2) | 1 caller | **Невозможно** | Уникальная возможность |
| Callees функции `build` | 4 вызова (depth=3) | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `crates/core/flags/defs.rs` (982 символа) | 983 строки — иерархия с parent, line ranges | `rg "(fn\|struct)" defs.rs` → 1089 строк (плоский список) | Иерархия: struct → impl → method |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `RegxMatcher` | Находит `RegexMatcher` [score: 0.98] — 4 определения | `rg "\bRegxMatcher\b"` → 0 результатов | Толерантность к опечаткам |
| Поиск с опечаткой `Searchr` | Находит `Searcher` [score: 0.97] | `rg "\bSearchr\b"` → 0 результатов | Толерантность к опечаткам |
| Поиск с опечаткой `GlobSetBilder` | Находит `GlobSetBuilder` [score: 0.99] | `rg "\bGlobSetBilder\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports main.rs` → 8 структурированных импортов | `rg "^use "` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 2753 (755 function + 1998 method) |
| Типы (struct/enum/trait) | YES | 418 (295 struct + 71 enum + 7 trait + 45 type_alias) |
| Импорты | YES | Структурированные `use` с путями модулей |
| Ссылки (Call/TypeUse/Extend) | YES | Классификация каждой ссылки по типу |
| Generics | NO | 0 символов (известное ограничение: Rust generic params пока не извлекаются) |
| Параметры с типами | YES | 2159 символов с типизированными параметрами |
| Return types | YES | 2174 символа с return type |
| Visibility (pub/private) | YES | 876 символов с visibility |
| Doc comments | YES | 856 символов с doc comments |

## Воспроизведение

```bash
# 1. Скачать репо
./benches/download_repos.sh

# 2. Проиндексировать
rm -f benches/repos/ripgrep/.code-index.db
time code-indexer --db benches/repos/ripgrep/.code-index.db index benches/repos/ripgrep

# 3. Статистика
code-indexer --db benches/repos/ripgrep/.code-index.db stats

# 4. Замеры
time code-indexer --db benches/repos/ripgrep/.code-index.db definition "new"
time rg "fn\s+new\b" benches/repos/ripgrep
time code-indexer --db benches/repos/ripgrep/.code-index.db references "Regex"
time rg "\bRegex\b" benches/repos/ripgrep
code-indexer --db benches/repos/ripgrep/.code-index.db symbols "RegxMatcher" --fuzzy
```
