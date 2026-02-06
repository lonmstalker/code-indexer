# Benchmarks

В репозитории есть два разных вида бенчмарков.

## 1) Capability checks (legacy)

`benches/results/*.md` сохранены как historical snapshot по функциональным преимуществам (`call graph`, `references`, `fuzzy`, `outline`).

Эти файлы **не являются источником честных speed-метрик**.

## 2) Honest speed checks (v2)

Честный speed-check находится в `benches/speed/run_speed_bench.py`.

Он фиксирует:

- baseline только `rg`;
- только definition-like кейсы из `benches/speed/cases.json`;
- strict parity precheck: `code_indexer_count == rg_count`;
- метрики `median`, `p95`, `cv%`;
- два режима: `query-only` и `first-run`.
- учитывает, что `code-indexer index` по умолчанию инкрементальный (skip unchanged по `content_hash`).

Ограничение: page-cache flush не выполняется (process-cold, не guaranteed disk-cold).

## Репозитории и pinning

Список репозиториев и commit SHAs фиксируется в `benches/repos.lock`.

Скачивание pinned репозиториев:

```bash
# Все 7 репо
./benches/download_repos.sh

# Только smoke-набор для CI/быстрой локальной проверки
./benches/download_repos.sh --repos ripgrep,tokio
```

## Локальный запуск честного speed-check

```bash
# 1) build
cargo build --release

# 2) run benchmark
python3 benches/speed/run_speed_bench.py \
  --repos all \
  --mode both \
  --runs 10 \
  --warmup 3 \
  --out-json benches/results/speed/latest.json \
  --out-md benches/results/speed/latest.md
```

Примечание по режимам:
- `query-only`: использует уже построенный индекс;
- `first-run`: runner принудительно удаляет DB перед каждой итерацией для полного cold-пайплайна (`index + query`), без инкрементального skip.

## CI smoke

Workflow: `.github/workflows/bench-smoke.yml`.

Smoke scope:

- repos: `ripgrep,tokio`
- mode: `both`
- output artifacts: `benches/results/speed/ci-smoke.json`, `benches/results/speed/ci-smoke.md`

CI падает, если нарушен контракт runner или не найдено ни одного valid parity-case для выбранных smoke-репозиториев.

## Структура

```text
benches/
├── README.md
├── repos.lock
├── download_repos.sh
├── speed/
│   ├── cases.json
│   └── run_speed_bench.py
├── repos/                 # gitignored
└── results/
    ├── speed/
    │   ├── README.md
    │   ├── latest.json
    │   └── latest.md
    ├── ripgrep.md
    ├── tokio.md
    ├── excalidraw.md
    ├── guava.md
    ├── prometheus.md
    ├── django.md
    └── kotlin.md
```
