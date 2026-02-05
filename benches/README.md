# Benchmarks

Сравнение code-indexer с CLI-инструментами (rg, grep, wc, find) на реальных open-source проектах.

## Тестовые репозитории

| Репо | Язык | Описание | Результаты |
|------|------|----------|-----------|
| ripgrep | Rust | CLI поиск ([BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep)) | [results/ripgrep.md](results/ripgrep.md) |
| tokio | Rust | Async runtime ([tokio-rs/tokio](https://github.com/tokio-rs/tokio)) | [results/tokio.md](results/tokio.md) |
| excalidraw | TypeScript | Whiteboard app ([excalidraw/excalidraw](https://github.com/excalidraw/excalidraw)) | [results/excalidraw.md](results/excalidraw.md) |
| guava | Java | Core libraries ([google/guava](https://github.com/google/guava)) | [results/guava.md](results/guava.md) |
| prometheus | Go | Monitoring system ([prometheus/prometheus](https://github.com/prometheus/prometheus)) | [results/prometheus.md](results/prometheus.md) |
| django | Python | Web framework ([django/django](https://github.com/django/django)) | [results/django.md](results/django.md) |
| kotlin | Kotlin | Kotlin compiler ([JetBrains/kotlin](https://github.com/JetBrains/kotlin)) | [results/kotlin.md](results/kotlin.md) |

## Как запустить замеры

```bash
# 1. Скачать репозитории (~2 ГБ, shallow clone)
./benches/download_repos.sh

# 2. Собрать code-indexer
cargo build --release

# 3. Проиндексировать репо (пример: ripgrep)
time ./target/release/code-indexer index benches/repos/ripgrep

# 4. Посмотреть статистику
./target/release/code-indexer stats --db benches/repos/ripgrep/.code-index.db

# 5. Замеры rg для сравнения
time rg "fn\s+new\b" benches/repos/ripgrep

# 6. Замеры code-indexer
time ./target/release/code-indexer definition "new" --db benches/repos/ripgrep/.code-index.db

# 7. Заполнить результаты в соответствующем MD-файле
```

## Структура

```
benches/
├── README.md              # Этот файл
├── download_repos.sh      # Скрипт загрузки репозиториев
├── repos/                 # Скачанные репозитории (gitignored)
└── results/               # Шаблоны сравнений по репо
    ├── ripgrep.md
    ├── tokio.md
    ├── excalidraw.md
    ├── guava.md
    ├── prometheus.md
    ├── django.md
    └── kotlin.md
```
