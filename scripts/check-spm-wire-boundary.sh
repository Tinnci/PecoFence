#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
patterns='serde_json::Value|serde_json::json|\.get\("method"\)|requestId|deliveryScope'
if rg -n "$patterns" "$root/crates/plugin-spm"; then
  echo "SPM wire boundary contains untyped JSON access" >&2
  exit 1
fi
