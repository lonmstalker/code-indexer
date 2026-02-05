# django (Python)

> Веб-фреймворк для Python — [django/django](https://github.com/django/django)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 2008 (1981 Python + 26 TypeScript + 1 Bash) |
| Символов извлечено | 66503 |
| Время индексации | 8.64 сек |
| Размер .code-index.db | 242 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `get` | 0.009 сек, 103 определения (с сигнатурами) | 0.078 сек, 52 совпадения | **9x** + сигнатуры и больше результатов (method + function) |
| Найти определение класса `Model` | 0.009 сек, 10+ классов (из тестов и core) | 0.073 сек, 239 совпадений (с подклассами) | **8x** + только определения, не подклассы |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `QuerySet` | 24 ref (1 Extend + 23 других) за 0.009 сек | 1403 строки за 0.139 сек | **15x** быстрее + классификация (Extend vs Call) |
| Кто вызывает функцию `get` | callers с точными локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callers `get` (depth=2) | 16 callers | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `tests/admin_views/tests.py` | 964 символа — иерархия class → method | `rg "(def\|class)"` → 559 строк | **1.7x** больше символов + иерархия |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `Queriset` | Находит `queryset` [score: 0.95] — методы и функции | `rg "\bQueriset\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные импорты | `rg "^import\|^from"` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 55623 (31874 function + 23749 method) |
| Классы | YES | 10880 классов |
| Импорты | YES | `import` и `from ... import` |
| Ссылки (Call/TypeUse/Extend) | YES | Включая наследование (Extend) |
| Параметры с типами (type hints) | YES | 55141 символов с типизированными параметрами |
| Return types | YES | 3 символа с return type (Python type hints редки в django) |
| self parameter | YES | Извлекается в параметрах |
| Наследование (Extend) | YES | Классификация Extend в ссылках |
| Doc comments (docstrings) | YES | 30 символов с docstrings |

## Воспроизведение

```bash
rm -f benches/repos/django/.code-index.db
time code-indexer --db benches/repos/django/.code-index.db index benches/repos/django
code-indexer --db benches/repos/django/.code-index.db stats
time code-indexer --db benches/repos/django/.code-index.db definition "get"
time rg "def\s+get\b" benches/repos/django --type py
code-indexer --db benches/repos/django/.code-index.db symbols "Queriset" --fuzzy
```
