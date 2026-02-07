#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="/Users/nikitakocnev/RustroverProjects/code-indexer"

exec codex \
  -C "$PROJECT_ROOT" \
  -c 'mcp_servers.code-indexer.command="docker"' \
  -c 'mcp_servers.code-indexer.args=["run","--rm","-i","-v","/Users/nikitakocnev/RustroverProjects/code-indexer:/workspace","-w","/workspace","-e","OPENAI_API_KEY","lonmstalkerd/code-indexer:latest","--db","/workspace/.code-index.db","serve"]' \
  "$@"
