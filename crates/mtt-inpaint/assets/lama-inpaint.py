#!/usr/bin/env python3
"""LaMa manga sidecar for MangaTranslationTools.

Expected contract:
  python scripts/lama-inpaint.py --input source.png --mask mask.png --output output.png

Default model:
  tools/inpaint-models/mayocream-lama-manga/lama-manga.safetensors
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F
from PIL import Image, ImageFilter
from safetensors.torch import load_file


REPO_ROOT = Path(__file__).resolve().parents[1]
LAMA_CODE_DIR = Path(os.environ.get("MANGA_TRANSLATOR_LAMA_CODE_DIR", REPO_ROOT / "tools" / "Er0mangaInpaint"))
DEFAULT_WEIGHTS = Path(
    os.environ.get(
        "MANGA_TRANSLATOR_LAMA_WEIGHTS",
        REPO_ROOT / "tools" / "inpaint-models" / "mayocream-lama-manga" / "lama-manga.safetensors",
    )
)

if str(LAMA_CODE_DIR) not in sys.path:
    sys.path.insert(0, str(LAMA_CODE_DIR))

from saicinpainting.training.modules.ffc import FFCResNetGenerator  # noqa: E402


_MODEL_CACHE: dict[tuple[str, str], torch.nn.Module] = {}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run mayocream/lama-manga inpainting for one image and mask.")
    parser.add_argument("--input", "--image_path", dest="input_path", required=True, help="Source image path.")
    parser.add_argument("--mask", "--mask_path", dest="mask_path", required=True, help="Mask image path. White means inpaint.")
    parser.add_argument("--output", dest="output_path", required=True, help="Output PNG path.")
    parser.add_argument("--weights", default=str(DEFAULT_WEIGHTS), help="lama-manga.safetensors path.")
    parser.add_argument("--device", default="auto", help="auto, cpu, mps, cuda, or cuda:0.")
    parser.add_argument("--threshold", type=int, default=8, help="Mask luma threshold from 0 to 255.")
    parser.add_argument("--dilate", type=int, default=0, help="Odd-pixel mask dilation kernel. Use 0 or 1 to disable.")
    parser.add_argument("--crop-margin", type=int, default=128, help="Context margin around each connected mask window.")
    args = parser.parse_args()

    weights_path = Path(args.weights)
    if not weights_path.exists():
        print(f"LaMa manga weights not found: {weights_path}", file=sys.stderr)
        return 2

    device = resolve_device(args.device)
    model = load_model(weights_path, device)
    image = Image.open(args.input_path).convert("RGB")
    mask = prepare_mask(Path(args.mask_path), image.size, args.threshold, args.dilate)

    result = inpaint(model, image, mask, device, args.crop_margin)
    output_path = Path(args.output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    result.save(output_path)
    return 0


def load_model(weights_path: Path, device: torch.device) -> torch.nn.Module:
    key = (str(weights_path.resolve()), str(device))
    cached = _MODEL_CACHE.get(key)
    if cached is not None:
        return cached

    model = FFCResNetGenerator(
        input_nc=4,
        output_nc=3,
        ngf=64,
        n_downsampling=3,
        n_blocks=18,
        add_out_act="sigmoid",
        init_conv_kwargs={"ratio_gin": 0, "ratio_gout": 0, "enable_lfu": False},
        downsample_conv_kwargs={"ratio_gin": 0, "ratio_gout": 0, "enable_lfu": False},
        resnet_conv_kwargs={"ratio_gin": 0.75, "ratio_gout": 0.75, "enable_lfu": False},
    )
    state_dict = load_file(str(weights_path), device="cpu")
    model.load_state_dict(state_dict, strict=True)
    model.eval().to(device)
    _MODEL_CACHE[key] = model
    return model


def inpaint(model: torch.nn.Module, image: Image.Image, mask: Image.Image, device: torch.device, crop_margin: int) -> Image.Image:
    windows = crop_windows_from_mask(mask, max(0, crop_margin))
    if not windows:
        return image.copy()

    image_width, image_height = image.size
    if len(windows) == 1 and windows[0] == (0, 0, image_width, image_height):
        return inpaint_window(model, image, mask, device)

    result = image.copy()
    working_mask = mask.copy()
    for window in windows:
        crop_image = result.crop(window)
        crop_mask = working_mask.crop(window)
        if not np.asarray(crop_mask).any():
            continue
        crop_result = inpaint_window(model, crop_image, crop_mask, device)
        result.paste(crop_result, window[:2], crop_mask)
        working_mask.paste(Image.new("L", crop_mask.size, 0), window[:2], crop_mask)
    return result


def inpaint_window(model: torch.nn.Module, image: Image.Image, mask: Image.Image, device: torch.device) -> Image.Image:
    image_np = np.asarray(image).astype("float32") / 255.0
    mask_np = np.asarray(mask).astype("float32") / 255.0
    if mask_np.ndim == 2:
        mask_np = mask_np[..., None]

    image_tensor = torch.from_numpy(image_np).permute(2, 0, 1).unsqueeze(0).to(device)
    mask_tensor = torch.from_numpy(mask_np).permute(2, 0, 1).unsqueeze(0).to(device)
    mask_tensor = (mask_tensor > 0.03).float()

    height, width = image_tensor.shape[-2:]
    pad_h = (8 - height % 8) % 8
    pad_w = (8 - width % 8) % 8
    if pad_h or pad_w:
        image_tensor = F.pad(image_tensor, (0, pad_w, 0, pad_h), mode="reflect")
        mask_tensor = F.pad(mask_tensor, (0, pad_w, 0, pad_h), mode="constant", value=0)

    model_input = torch.cat([image_tensor * (1 - mask_tensor), mask_tensor], dim=1)
    with torch.inference_mode():
        predicted = model(model_input).clamp(0, 1)
        inpainted = mask_tensor * predicted + (1 - mask_tensor) * image_tensor

    inpainted = inpainted[:, :, :height, :width]
    output = inpainted[0].permute(1, 2, 0).detach().cpu().numpy()
    output = np.round(output * 255).clip(0, 255).astype("uint8")
    return Image.fromarray(output, mode="RGB")


def crop_windows_from_mask(mask: Image.Image, margin: int) -> list[tuple[int, int, int, int]]:
    mask_np = np.asarray(mask) > 0
    height, width = mask_np.shape
    boxes = connected_mask_boxes(mask_np)
    if not boxes:
        return []

    windows = [
        (
            max(0, x1 - margin),
            max(0, y1 - margin),
            min(width, x2 + margin),
            min(height, y2 + margin),
        )
        for x1, y1, x2, y2 in boxes
    ]
    return merge_windows(windows)


def connected_mask_boxes(mask: np.ndarray) -> list[tuple[int, int, int, int]]:
    height, width = mask.shape
    visited = np.zeros(mask.shape, dtype=bool)
    boxes: list[tuple[int, int, int, int]] = []

    for start_y in range(height):
        for start_x in range(width):
            if visited[start_y, start_x] or not mask[start_y, start_x]:
                continue

            min_x = max_x = start_x
            min_y = max_y = start_y
            stack = [(start_x, start_y)]
            visited[start_y, start_x] = True

            while stack:
                x, y = stack.pop()
                if x < min_x:
                    min_x = x
                elif x > max_x:
                    max_x = x
                if y < min_y:
                    min_y = y
                elif y > max_y:
                    max_y = y

                for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                    if nx < 0 or ny < 0 or nx >= width or ny >= height:
                        continue
                    if visited[ny, nx] or not mask[ny, nx]:
                        continue
                    visited[ny, nx] = True
                    stack.append((nx, ny))

            boxes.append((min_x, min_y, max_x + 1, max_y + 1))

    return boxes


def merge_windows(windows: list[tuple[int, int, int, int]]) -> list[tuple[int, int, int, int]]:
    merged: list[tuple[int, int, int, int]] = []
    for window in sorted(windows):
        merge_window_into(merged, window)
    return sorted(merged)


def merge_window_into(merged: list[tuple[int, int, int, int]], window: tuple[int, int, int, int]) -> None:
    current = window
    while True:
        for index, candidate in enumerate(merged):
            if windows_touch_or_overlap(candidate, current):
                current = union_window(candidate, current)
                merged.pop(index)
                break
        else:
            merged.append(current)
            return


def windows_touch_or_overlap(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> bool:
    return a[0] <= b[2] and b[0] <= a[2] and a[1] <= b[3] and b[1] <= a[3]


def union_window(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    return min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3])


def prepare_mask(mask_path: Path, size: tuple[int, int], threshold: int, dilate: int) -> Image.Image:
    threshold = max(0, min(255, threshold))
    mask = Image.open(mask_path).convert("L")
    if mask.size != size:
        mask = mask.resize(size, Image.Resampling.NEAREST)
    mask = mask.point(lambda value: 255 if value > threshold else 0)

    if dilate > 1:
        kernel = dilate if dilate % 2 == 1 else dilate + 1
        mask = mask.filter(ImageFilter.MaxFilter(kernel))

    return mask


def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        if torch.cuda.is_available():
            return torch.device("cuda:0")
        if getattr(torch.backends, "mps", None) is not None and torch.backends.mps.is_available():
            return torch.device("mps")
        return torch.device("cpu")
    return torch.device(requested)


if __name__ == "__main__":
    raise SystemExit(main())
