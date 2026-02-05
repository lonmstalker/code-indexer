# excalidraw (TypeScript)

> Виртуальная доска для рисования — [excalidraw/excalidraw](https://github.com/excalidraw/excalidraw)

## Характеристики проекта

| Метрика | Значение |
|---------|----------|
| Файлов в индексе | 440 (TypeScript) |
| Символов извлечено | 2974 |
| Время индексации | 2.30 сек |
| Размер .code-index.db | 50 MB |

## Сравнение: Поиск определения

| Задача | code-indexer | rg | Ускорение |
|--------|--------------|-----|-----------|
| Найти определение функции `render` | 0.009 сек, 3 определения (с сигнатурами) | 0.013 сек, 0 совпадений (rg ищет `function render`, но в TS часто arrow fn) | code-indexer находит все, rg — 0 |
| Найти определение типа `ExcalidrawElement` | 0.009 сек, 1 type_alias | 0.020 сек, 2 совпадения | **2x** + точная семантика |

## Сравнение: Поиск ссылок

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Все использования `AppState` | 50 ref (50 TypeUse) за 0.009 сек | 546 строк за 0.025 сек | **3x** быстрее + классификация: только TypeUse |
| Кто вызывает функцию `render` | callers с точными локациями | **Невозможно** | Уникальная возможность |

## Сравнение: Граф вызовов

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Callees функции `render` | 12 вызовов (depth=3) | **Невозможно** | Уникальная возможность |

## Сравнение: Outline файла

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Структура `packages/excalidraw/subset/woff2/woff2-bindings.ts` | 224 символа — иерархия | `rg "(function\|interface)"` → 419 строк | Семантический outline vs raw regex |

## Сравнение: Fuzzy поиск

| Задача | code-indexer | rg | Преимущество |
|--------|--------------|-----|-------------|
| Поиск с опечаткой `rendr` | Находит `render` [score: 0.97] — 3 функции | `rg "\brendr\b"` → 0 результатов | Толерантность к опечаткам |

## Сравнение: Метрики и dead code

| Задача | code-indexer | wc / rg | Преимущество |
|--------|--------------|---------|-------------|
| LOC функции | `code-indexer stats` → per-function line ranges | `wc -l` → per-file only | Гранулярность |
| Импорты файла | `code-indexer imports` → структурированные импорты | `rg "^import"` → raw text | Структурированные данные |

## Языковые фичи

| Фича | Поддержка | Примечание |
|------|:---------:|-----------|
| Функции/методы | YES | 2339 функций |
| Типы (interface/type alias) | YES | 635 (123 interface + 439 type_alias + 70 class + 3 enum) |
| Импорты | YES | `import` / `export` |
| Ссылки (Call/TypeUse/Extend) | YES | Классификация каждой ссылки |
| Generics | YES | 154 символа с generic параметрами |
| Параметры с типами | YES | 620 символов с типизированными параметрами |
| Return types | YES | 165 символов с return type |
| Visibility (export/default) | YES | 175 символов с visibility |
| Doc comments | YES | 88 символов с doc comments |

## Воспроизведение

```bash
rm -f benches/repos/excalidraw/.code-index.db
time code-indexer --db benches/repos/excalidraw/.code-index.db index benches/repos/excalidraw
code-indexer --db benches/repos/excalidraw/.code-index.db stats
time code-indexer --db benches/repos/excalidraw/.code-index.db definition "render"
time rg "function\s+render" benches/repos/excalidraw --type ts
code-indexer --db benches/repos/excalidraw/.code-index.db symbols "rendr" --fuzzy
```
