#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON_BIN:-python3.11}"
venv="$root/tools/lama-manga-venv"
lama_repo="$root/tools/Er0mangaInpaint"
model_dir="$root/tools/inpaint-models/mayocream-lama-manga"
model_path="$model_dir/lama-manga.safetensors"
model_url="https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors"

mkdir -p "$root/tools" "$model_dir"

if [[ ! -x "$venv/bin/python" ]]; then
  "$python_bin" -m venv "$venv"
fi

"$venv/bin/python" -m pip install --upgrade pip setuptools wheel
"$venv/bin/python" -m pip install \
  torch torchvision opencv-python-headless \
  hydra-core==1.3.2 omegaconf==2.3.0 pytorch-lightning==1.4.2 torchmetrics==0.6.0 \
  kornia==0.5.0 PyYAML tqdm numpy safetensors \
  albumentations==0.5.2 scikit-image scipy pandas matplotlib easydict webdataset

if [[ ! -d "$lama_repo/.git" ]]; then
  rm -rf "$lama_repo"
  git clone --depth 1 https://github.com/Er0manga/Er0mangaInpaint.git "$lama_repo"
fi

if [[ ! -f "$model_path" ]]; then
  curl -L --fail --retry 3 -o "$model_path" "$model_url"
fi

"$venv/bin/python" - <<PY
from safetensors.torch import load_file
from pathlib import Path
path = Path("$model_path")
state = load_file(str(path), device="cpu")
assert len(state) == 989, f"unexpected lama-manga tensor count: {len(state)}"
print(f"lama-manga ready: {path}")
PY

python3 -m py_compile "$root/scripts/lama-inpaint.py"
