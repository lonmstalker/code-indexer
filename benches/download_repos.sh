#!/usr/bin/env bash
set -eu

REPOS_DIR="benches/repos"
mkdir -p "$REPOS_DIR"

clone_if_missing() {
    local name="$1"
    local url="$2"
    local dest="$REPOS_DIR/$name"
    if [ -d "$dest" ]; then
        echo "Already exists: $name"
    else
        echo "Cloning $name..."
        git clone --depth 1 "$url" "$dest"
    fi
}

clone_if_missing "ripgrep"    "https://github.com/BurntSushi/ripgrep"
clone_if_missing "tokio"      "https://github.com/tokio-rs/tokio"
clone_if_missing "excalidraw" "https://github.com/excalidraw/excalidraw"
clone_if_missing "guava"      "https://github.com/google/guava"
clone_if_missing "prometheus" "https://github.com/prometheus/prometheus"
clone_if_missing "django"     "https://github.com/django/django"
clone_if_missing "kotlin"     "https://github.com/JetBrains/kotlin"

echo "Done. Repos in $REPOS_DIR/"
