# Code Indexer Skill — Installation

## Способ 1: Локальная установка (рекомендуется)

```bash
# Распаковать в глобальные скиллы
tar -xzf code-indexer-skill.tar.gz -C ~/.claude/skills/

# Или в проект
tar -xzf code-indexer-skill.tar.gz -C .claude/skills/
```

## Способ 2: Прямое копирование

```bash
# Глобально (для всех проектов)
cp -r code-indexer ~/.claude/skills/

# Для конкретного проекта
mkdir -p .claude/skills
cp -r code-indexer .claude/skills/
```

## Способ 3: Из GitHub (если опубликован)

```bash
# Клонировать репозиторий
git clone https://github.com/USER/code-indexer-skill.git ~/.claude/skills/code-indexer

# Или через curl
curl -L https://github.com/USER/code-indexer-skill/archive/main.tar.gz | tar -xz -C ~/.claude/skills/
```

## Проверка установки

После установки скилл автоматически появится в списке доступных. Триггеры:
- "найти определение"
- "поиск символа"
- "граф вызовов"
- "find symbol"
- "call graph"

## Структура

```
code-indexer/
└── SKILL.md          # Основной файл скилла
```

## Требования

- code-indexer CLI должен быть установлен и доступен в PATH
- Индексация: `code-indexer index` перед использованием

## SHA256

```
60c3bf27b551eaf85b738dd87cd6f43ba03c51befdc0ca1a4d3f865e3cb3d388  code-indexer-skill.tar.gz
```
