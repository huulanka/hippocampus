#!/usr/bin/env bash
# Downloads the on-device speech model the desktop client uses.
#
# ~670 MB, int8-quantised parakeet-tdt-0.6b-v3 (multilingual, German
# included). transcribe-rs expects exactly these four files side by side.
set -euo pipefail

REPO="istupakov/parakeet-tdt-0.6b-v3-onnx"
FILES=(
  encoder-model.int8.onnx
  decoder_joint-model.int8.onnx
  nemo128.onnx
  vocab.txt
)

DEST="${HIPPOCAMPUS_ASR_MODEL_DIR:-$HOME/Library/Application Support/com.andreasbauer.hippocampus/models/parakeet-tdt-0.6b-v3}"
mkdir -p "$DEST"

for file in "${FILES[@]}"; do
  if [ -s "$DEST/$file" ]; then
    echo "have    $file"
    continue
  fi
  echo "fetching $file"
  curl -fL --progress-bar -o "$DEST/$file" \
    "https://huggingface.co/$REPO/resolve/main/$file"
done

echo
echo "Model ready in: $DEST"
