#!/usr/bin/env sh
set -eu

REPO_OWNER="${REPO_OWNER:-lonmstalker}"
REPO_NAME="${REPO_NAME:-code-indexer}"
VERSION="${CODE_INDEXER_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

say() {
  printf '%s\n' "$*"
}

fail() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

detect_target() {
  os="$(uname -s 2>/dev/null || true)"
  arch="$(uname -m 2>/dev/null || true)"

  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux) fail "Linux assets are not published for this release yet" ;;
    *) fail "unsupported OS: $os (supported: Darwin)" ;;
  esac

  case "$arch" in
    arm64 | aarch64) arch_part="aarch64" ;;
    x86_64 | amd64) arch_part="x86_64" ;;
    *) fail "unsupported architecture: $arch (supported: arm64/aarch64, x86_64/amd64)" ;;
  esac

  printf '%s-%s' "$arch_part" "$os_part"
}

release_path() {
  case "$VERSION" in
    latest) printf 'latest/download' ;;
    v*) printf 'download/%s' "$VERSION" ;;
    *) printf 'download/v%s' "$VERSION" ;;
  esac
}

main() {
  need_cmd curl
  need_cmd tar

  target="$(detect_target)"
  asset="code-indexer-$target.tar.gz"
  path="$(release_path)"
  url="https://github.com/$REPO_OWNER/$REPO_NAME/releases/$path/$asset"

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

  archive="$tmp_dir/$asset"
  say "Downloading $url"
  curl -fsSL "$url" -o "$archive" || fail "failed to download release asset for target '$target'"

  tar -xzf "$archive" -C "$tmp_dir"
  unpacked="$tmp_dir/code-indexer-$target/code-indexer"
  [ -f "$unpacked" ] || fail "unexpected archive format: missing code-indexer-$target/code-indexer"

  mkdir -p "$INSTALL_DIR"
  cp "$unpacked" "$INSTALL_DIR/code-indexer"
  chmod 755 "$INSTALL_DIR/code-indexer"

  say "Installed to $INSTALL_DIR/code-indexer"
  "$INSTALL_DIR/code-indexer" --version || true

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) say "Add to PATH: export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
  esac
}

main "$@"
