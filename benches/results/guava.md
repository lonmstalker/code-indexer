# guava (Java)

> [!WARNING]
> Legacy speed data: этот файл сохранён как historical capability snapshot и не является источником честных speed-метрик.
> Для honest speed-check используйте 'benches/speed/run_speed_bench.py' и отчёты в 'benches/results/speed/'.

> Основные библиотеки Google для Java — [google/guava](https://github.com/google/guava)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 3098 (3097 Java + 1 Bash) |
| Символов извлечено | 67073 |
| Время индексации | 11.07 сек |
| Размер .code-index.db | 522 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение метода `of` | 0.010 сек, 465 определений (с сигнатурами) | 0.071 сек, 396 совпадений | **7x** + полные сигнатуры |
| Найти определение типа `ImmutableList` | 0.009 сек, 3 класса + 3 конструктора | 0.070 сек, 3 совпадения | **8x** + семантика (class vs constructor) |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `Preconditions` | 50 ref (3 TypeUse + 47 Call) за 0.009 сек | 1514 строк за 0.069 сек | **8x** быстрее + классификация (TypeUse/Call) |
| Кто вызывает метод `of` | callers с точными локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callees метода `of` | 5 вызовов (depth=3) | **Невозможно** | Уникальная возможность |
| Callers (depth=2) | Многоуровневый граф вызовов | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `guava/src/.../collect/Maps.java` | 506 символов — иерархия class → method | `rg "(class\|public)"` → 355 строк | **1.4x** больше символов + иерархия |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `ImmutableLst` | Находит `ImmutableList` [score: 0.98] — 3 класса | `rg "\bImmutableLst\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные импорты | `rg "^import"` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 59742 (56054 function + 3688 method) |
| Типы (class/interface/enum) | YES | 7331 (6417 class + 500 interface + 414 enum) |
| Импорты | YES | Структурированные `import` |
| Ссылки (Call/TypeUse/Extend) | YES | Классификация каждой ссылки |
| Generics | YES | 6803 символа с generic параметрами |
| Параметры с типами | YES | 25580 символов с типизированными параметрами |
| Return types | NO | 0 (известное ограничение: Java return types пока не извлекаются) |
| Visibility (public/private/protected) | YES | 54703 символа с visibility |
| Doc comments (Javadoc) | YES | 14517 символов с doc comments |

## Воспроизведение

```bash
rm -f benches/repos/guava/.code-index.db
time code-indexer --db benches/repos/guava/.code-index.db index benches/repos/guava
code-indexer --db benches/repos/guava/.code-index.db stats
time code-indexer --db benches/repos/guava/.code-index.db definition "of"
time rg "public\s+static.*\bof\b" benches/repos/guava --type java
code-indexer --db benches/repos/guava/.code-index.db symbols "ImmutableLst" --fuzzy
```
