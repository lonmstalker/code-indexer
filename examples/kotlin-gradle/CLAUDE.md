# CLAUDE — local (Claude)

Этот файл дополняет корневой `CLAUDE.md`.
Сначала прочитай `../CLAUDE.md` или `../../CLAUDE.md` (см. ссылку ниже).

Ссылка на корень: ../../CLAUDE.md

Локальный контекст:
Пример Kotlin Gradle‑проекта. Есть `build.gradle.kts`, `settings.gradle.kts`, `src/`, и файл `.code-index.db` как индекс‑артефакт; не менять без явного запроса.
`code-indexer index` работает инкрементально по `content_hash`; полный rebuild делай только через удаление `.code-index.db` и повторный запуск индексации.
