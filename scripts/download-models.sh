#!/usr/bin/env bash
# Download the pinned EmbeddingGemma 2 GGUF files (ModelFiles.swift).
set -euo pipefail
REV=bfcd298762cc34d0357ece5ebdd31791a3a374d8
REPO=ggml-org/embeddinggemma-2-GGUF
DEST="${TROWEL_MODELS:-${XDG_DATA_HOME:-$HOME/.local/share}/trowel/models}"
mkdir -p "$DEST"

download() {
  local name="$1" bytes="$2" sha="$3"
  local url="https://huggingface.co/${REPO}/resolve/${REV}/${name}"
  local path="$DEST/$name"
  if [[ -f "$path" ]] && [[ "$(stat -c%s "$path")" == "$bytes" ]]; then
    echo "have $name"
    return
  fi
  echo "downloading $name"
  curl -L --fail --retry 5 --continue-at - -o "$path.partial" "$url"
  mv "$path.partial" "$path"
  local got
  got="$(sha256sum "$path" | awk '{print $1}')"
  if [[ "$got" != "$sha" ]]; then
    echo "SHA-256 mismatch for $name: $got want $sha" >&2
    exit 1
  fi
}

download embeddinggemma-2-Q8_0.gguf 309855456 \
  2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135
download mmproj-embeddinggemma-2-Q8_0.gguf 554821024 \
  c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f
echo "models in $DEST"
