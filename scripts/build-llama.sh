#!/usr/bin/env bash
# Pin llama.cpp at b11461, apply upstream/scripts/llama.cpp-no-logits.patch, and
# build trowel-worker for vulkan, cpu, or both. Mirrors upstream/scripts/build-llama.sh
# with Metal swapped for Vulkan (EXECUTION-PLAN Phase 0).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAG=b11461
PATCH="$ROOT/upstream/scripts/llama.cpp-no-logits.patch"
SOURCE="$ROOT/cpp/llama/src"
JOBS="$(nproc 2>/dev/null || echo 8)"

backend="${1:-all}"
case "$backend" in
  vulkan|cpu|all|sizing) ;;
  *)
    echo "usage: $0 [vulkan|cpu|all|sizing]" >&2
    exit 2
    ;;
esac

if [[ "$backend" == "sizing" ]]; then
  cmake -S "$ROOT/cpp" -B "$ROOT/cpp/build-sizing" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DTROWEL_BUILD_WORKER=OFF
  cmake --build "$ROOT/cpp/build-sizing" -j "$JOBS" --target trowel-image-sizing-test
  echo "Built $ROOT/cpp/build-sizing/trowel-image-sizing-test"
  exit 0
fi

if [[ ! -d "$SOURCE" ]]; then
  git -c advice.detachedHead=false clone --depth 1 --branch "$TAG" \
    https://github.com/ggml-org/llama.cpp "$SOURCE"
elif [[ "$(git -C "$SOURCE" describe --tags --exact-match 2>/dev/null || true)" != "$TAG" ]]; then
  git -C "$SOURCE" fetch --depth 1 origin tag "$TAG"
  git -C "$SOURCE" checkout --force "$TAG"
fi

if git -C "$SOURCE" apply --check "$PATCH" 2>/dev/null; then
  git -C "$SOURCE" apply "$PATCH"
fi

build_one() {
  local name="$1" vulkan="$2"
  local build="$ROOT/cpp/build-$name"
  cmake -S "$ROOT/cpp" -B "$build" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DTROWEL_VULKAN="$vulkan" \
    -DTROWEL_BUILD_WORKER=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_NATIVE=OFF \
    -DLLAMA_BUILD_MTMD=ON \
    -DLLAMA_BUILD_COMMON=OFF \
    -DLLAMA_BUILD_TOOLS=OFF \
    -DLLAMA_BUILD_SERVER=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_APP=OFF \
    -DLLAMA_BUILD_UI=OFF \
    -DLLAMA_OPENSSL=OFF
  cmake --build "$build" -j "$JOBS" --target trowel-worker trowel-image-sizing-test
  echo "Built $build/trowel-worker ($name)"
}

if [[ "$backend" == "all" || "$backend" == "vulkan" ]]; then
  build_one vulkan ON
fi
if [[ "$backend" == "all" || "$backend" == "cpu" ]]; then
  build_one cpu OFF
fi
