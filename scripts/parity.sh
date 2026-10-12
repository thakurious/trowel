#!/usr/bin/env bash
# Vector-parity gate: every reference cosine ≥ 0.999 on vulkan and/or cpu.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
backend="${1:-all}"

run() {
  local name="$1"
  echo "=== parity $name ==="
  cargo run -q -p trowel-cli -- parity --backend "$name"
}

case "$backend" in
  vulkan) run vulkan ;;
  cpu) run cpu ;;
  all)
    run vulkan
    run cpu
    ;;
  *)
    echo "usage: $0 [vulkan|cpu|all]" >&2
    exit 2
    ;;
esac
