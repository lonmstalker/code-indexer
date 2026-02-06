#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPOS_DIR="$SCRIPT_DIR/repos"
LOCK_FILE="$SCRIPT_DIR/repos.lock"
SELECTED_REPOS="all"

usage() {
  cat <<'USAGE'
Usage: ./benches/download_repos.sh [--repos <name1,name2|all>] [--repos-dir <path>] [--lock-file <path>]

Options:
  --repos       Comma-separated repo names from repos.lock (default: all)
  --repos-dir   Destination directory for repositories (default: benches/repos)
  --lock-file   Path to lock file with pinned SHAs (default: benches/repos.lock)
  -h, --help    Show this help

Examples:
  ./benches/download_repos.sh
  ./benches/download_repos.sh --repos ripgrep,tokio
USAGE
}

trim_spaces() {
  local value="$1"
  # shellcheck disable=SC2001
  echo "$(echo "$value" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
}

is_selected() {
  local repo_name="$1"
  local selected="$2"

  if [[ "$selected" == "all" ]]; then
    return 0
  fi

  local IFS=','
  local item
  read -r -a items <<< "$selected"
  for item in "${items[@]}"; do
    item="$(trim_spaces "$item")"
    if [[ "$item" == "$repo_name" ]]; then
      return 0
    fi
  done

  return 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repos)
      [[ $# -ge 2 ]] || { echo "--repos requires a value" >&2; exit 1; }
      SELECTED_REPOS="$2"
      shift 2
      ;;
    --repos-dir)
      [[ $# -ge 2 ]] || { echo "--repos-dir requires a value" >&2; exit 1; }
      REPOS_DIR="$2"
      shift 2
      ;;
    --lock-file)
      [[ $# -ge 2 ]] || { echo "--lock-file requires a value" >&2; exit 1; }
      LOCK_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ! -f "$LOCK_FILE" ]]; then
  echo "Lock file not found: $LOCK_FILE" >&2
  exit 1
fi

mkdir -p "$REPOS_DIR"

processed=0
selected_found=0

while read -r name url sha _; do
  [[ -z "${name:-}" ]] && continue
  [[ "${name:0:1}" == "#" ]] && continue

  selected_found=$((selected_found + 1))

  if ! is_selected "$name" "$SELECTED_REPOS"; then
    continue
  fi

  processed=$((processed + 1))
  dest="$REPOS_DIR/$name"

  if [[ -d "$dest/.git" ]]; then
    echo "Updating $name..."
  else
    echo "Cloning $name..."
    git clone --depth 1 "$url" "$dest"
  fi

  if ! git -C "$dest" cat-file -e "${sha}^{commit}" 2>/dev/null; then
    git -C "$dest" fetch --depth 1 origin "$sha" || git -C "$dest" fetch origin
  fi

  git -C "$dest" checkout --detach "$sha"
  current_sha="$(git -C "$dest" rev-parse HEAD)"
  echo "Pinned $name at $current_sha"
done < "$LOCK_FILE"

if [[ "$processed" -eq 0 ]]; then
  echo "No repositories matched --repos='$SELECTED_REPOS'." >&2
  if [[ "$selected_found" -eq 0 ]]; then
    echo "Lock file appears empty: $LOCK_FILE" >&2
  fi
  exit 1
fi

echo "Done. Repositories are in $REPOS_DIR"
