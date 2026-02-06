# kotlin (Kotlin)

> [!WARNING]
> Legacy speed data: этот файл сохранён как historical capability snapshot и не является источником честных speed-метрик.
> Для honest speed-check используйте 'benches/speed/run_speed_bench.py' и отчёты в 'benches/results/speed/'.

> Компилятор языка Kotlin — [JetBrains/kotlin](https://github.com/JetBrains/kotlin)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 63183 (58194 Kotlin + 3980 Java + 471 C++ + 339 TS + 192 Swift + 4 Python + 3 Bash) |
| Символов извлечено | 507490 |
| Время индексации | 15 мин 10 сек |
| Размер .code-index.db | 3.2 GB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `resolve` | 0.009 сек, 172 определения (с сигнатурами) | 2.754 сек, 189 совпадений | **306x** |
| Найти определение типа `KtClass` | 0.008 сек, 2 класса | 2.672 сек, 6 совпадений | **334x** |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `PsiElement` | 50 ref (47 TypeUse + 3 других) за 0.008 сек | 74687 строк за 4.290 сек | **536x** быстрее + классификация TypeUse/Call |
| Кто вызывает `resolve` | callers с точными локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Граф вызовов `resolve` | Построение графа за мгновение | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `libraries/stdlib/common/src/generated/_Arrays.kt` | 1775 символов — иерархия | `rg "(fun\|class)"` → 2661 строк | Семантическая иерархия vs плоский текст |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `resolv` | Находит `resolve` [score: 0.97] — 172 определения | `rg "\bresolv\b"` → 0 результатов | Толерантность к опечаткам |
| Поиск с опечаткой `KtClss` | Находит `KtClass` [score: 0.97] — 2 класса | `rg "\bKtClss\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные | `rg "^import"` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 395511 (393038 function + 2473 method) |
| Типы (class/interface/object) | YES | 111979 (106103 class + 1129 interface + 811 struct + 244 enum + 3692 type_alias) |
| Импорты | YES | Структурированные `import` |
| Ссылки (Call/TypeUse/Extend) | YES | Классификация каждой ссылки |
| Generics | YES | 33675 символов с generic параметрами |
| Параметры с типами | YES | 149971 символов с типизированными параметрами |
| Return types | YES | 83 символа с return type |
| Visibility (public/private/internal) | YES | 244636 символов с visibility |
| Doc comments (KDoc) | YES | 33438 символов с doc comments |

## Воспроизведение

```bash
rm -f benches/repos/kotlin/.code-index.db
time code-indexer --db benches/repos/kotlin/.code-index.db index benches/repos/kotlin
code-indexer --db benches/repos/kotlin/.code-index.db stats
time code-indexer --db benches/repos/kotlin/.code-index.db definition "resolve"
time rg "fun\s+resolve\b" benches/repos/kotlin --type kotlin
code-indexer --db benches/repos/kotlin/.code-index.db symbols "resolv" --fuzzy
```
