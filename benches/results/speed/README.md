# Honest Speed Reports (v2)

Это директория для **честных speed-benchmark** отчётов `code-indexer` vs `rg`.

## Что считается честным speed-check

- сравниваются только definition-like сценарии из `benches/speed/cases.json`;
- перед таймингом выполняется strict precheck count parity (`code_indexer_count == rg_count`);
- invalid case (mismatch по количеству) исключается из итогового speedup summary;
- считаются `median`, `p95`, `cv%`.

## Режимы

- `query-only`: один `index`, затем многократные query-замеры.
- `first-run`: на каждую итерацию `rm db + index + query` для `code-indexer`; для `rg` только query.

## Ограничения

На macOS не делается принудительный page-cache flush, поэтому это process-cold benchmark, а не guaranteed disk-cold benchmark.

## Запуск

```bash
# 1) Скачать pinned репозитории
./benches/download_repos.sh

# 2) Собрать бинарь
cargo build --release

# 3) Запустить честный speed-check
python3 benches/speed/run_speed_bench.py \
  --repos all \
  --mode both \
  --runs 10 \
  --warmup 3 \
  --out-json benches/results/speed/latest.json \
  --out-md benches/results/speed/latest.md
```

CI smoke использует только `ripgrep,tokio` и проверяет наличие валидных parity-кейсов.
