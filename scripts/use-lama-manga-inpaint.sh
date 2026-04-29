#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export MANGA_TRANSLATOR_LAMA_COMMAND="$root/tools/lama-manga-venv/bin/python"
export MANGA_TRANSLATOR_LAMA_ARGS="[\"$root/scripts/lama-inpaint.py\",\"--input\",\"{source}\",\"--mask\",\"{mask}\",\"--output\",\"{output}\",\"--weights\",\"$root/tools/inpaint-models/mayocream-lama-manga/lama-manga.safetensors\"]"

cd "$root"
exec npm run dev
