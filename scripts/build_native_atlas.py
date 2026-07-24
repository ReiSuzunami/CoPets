#!/usr/bin/env python3
"""Build a scaled CoPets atlas directly from generated row strips.

The official 1x frames are used only for alpha-bounds layout. Pixel data always
comes from the original generated strips and is never enlarged.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
from pathlib import Path

from PIL import Image

BASE_CELL_WIDTH = 192
BASE_CELL_HEIGHT = 208
COLUMNS = 8
ROWS = (
    ("idle", 6),
    ("running-right", 8),
    ("running-left", 8),
    ("waving", 4),
    ("jumping", 5),
    ("failed", 8),
    ("waiting", 6),
    ("running", 6),
    ("review", 6),
)


def parse_hex_color(value: str) -> tuple[int, int, int]:
    if not re.fullmatch(r"#[0-9a-fA-F]{6}", value):
        raise SystemExit(f"invalid chroma key: {value}; expected #RRGGBB")
    return tuple(int(value[index : index + 2], 16) for index in (1, 3, 5))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def color_distance(
    red: int,
    green: int,
    blue: int,
    key: tuple[int, int, int],
) -> float:
    return math.sqrt(
        (red - key[0]) ** 2 + (green - key[1]) ** 2 + (blue - key[2]) ** 2
    )


def remove_chroma_background(
    image: Image.Image,
    chroma_key: tuple[int, int, int],
    threshold: float,
) -> Image.Image:
    rgba = image.convert("RGBA")
    pixels = rgba.load()
    for y in range(rgba.height):
        for x in range(rgba.width):
            red, green, blue, alpha = pixels[x, y]
            if color_distance(red, green, blue, chroma_key) <= threshold:
                pixels[x, y] = (0, 0, 0, 0)
    soften_chroma_fringe(rgba, chroma_key, threshold)
    despill_chroma_boundary(rgba, chroma_key)
    return rgba


def soften_chroma_fringe(
    image: Image.Image,
    chroma_key: tuple[int, int, int],
    threshold: float,
    spill_range: float = 84.0,
) -> None:
    source = image.copy()
    source_pixels = source.load()
    pixels = image.load()
    for y in range(image.height):
        for x in range(image.width):
            red, green, blue, alpha = source_pixels[x, y]
            if alpha == 0:
                continue
            distance = color_distance(red, green, blue, chroma_key)
            if distance > threshold + spill_range:
                continue
            touches_transparent = any(
                0 <= nx < image.width
                and 0 <= ny < image.height
                and source_pixels[nx, ny][3] == 0
                for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1))
            )
            if touches_transparent:
                keep = max(0.0, min(1.0, (distance - threshold) / spill_range))
                if keep <= 0:
                    pixels[x, y] = (red, green, blue, 0)
                    continue
                # Undo the chroma contribution before lowering alpha. Without
                # this step, texture filtering exposes a green silhouette halo.
                foreground = tuple(
                    max(0, min(255, round((channel - (1 - keep) * key) / keep)))
                    for channel, key in zip((red, green, blue), chroma_key)
                )
                pixels[x, y] = (*foreground, min(alpha, round(alpha * keep)))


def despill_chroma_boundary(
    image: Image.Image,
    chroma_key: tuple[int, int, int],
    radius: int = 4,
) -> None:
    key_channel = max(range(3), key=lambda index: chroma_key[index])
    if chroma_key[key_channel] < 200:
        return
    source = image.copy()
    source_pixels = source.load()
    pixels = image.load()
    for y in range(image.height):
        for x in range(image.width):
            channels = list(source_pixels[x, y][:3])
            alpha = source_pixels[x, y][3]
            other_channels = [channels[index] for index in range(3) if index != key_channel]
            if alpha <= 16 or channels[key_channel] - max(other_channels) < 25:
                continue
            near_transparency = any(
                source_pixels[nx, ny][3] <= 16
                for ny in range(max(0, y - radius), min(image.height, y + radius + 1))
                for nx in range(max(0, x - radius), min(image.width, x + radius + 1))
            )
            if near_transparency:
                channels[key_channel] = max(other_channels)
                pixels[x, y] = (*channels, alpha)


def connected_components(image: Image.Image) -> list[dict[str, object]]:
    alpha = image.getchannel("A")
    width, height = image.size
    data = alpha.tobytes()
    visited = bytearray(width * height)
    components: list[dict[str, object]] = []

    for start, alpha_value in enumerate(data):
        if alpha_value <= 16 or visited[start]:
            continue
        stack = [start]
        visited[start] = 1
        pixels: list[int] = []
        min_x = width
        min_y = height
        max_x = 0
        max_y = 0
        while stack:
            current = stack.pop()
            pixels.append(current)
            x = current % width
            y = current // width
            min_x = min(min_x, x)
            min_y = min(min_y, y)
            max_x = max(max_x, x)
            max_y = max(max_y, y)
            neighbors = []
            if x > 0:
                neighbors.append(current - 1)
            if x + 1 < width:
                neighbors.append(current + 1)
            if y > 0:
                neighbors.append(current - width)
            if y + 1 < height:
                neighbors.append(current + width)
            for neighbor in neighbors:
                if not visited[neighbor] and data[neighbor] > 16:
                    visited[neighbor] = 1
                    stack.append(neighbor)
        components.append(
            {
                "pixels": pixels,
                "area": len(pixels),
                "bbox": (min_x, min_y, max_x + 1, max_y + 1),
                "center_x": (min_x + max_x + 1) / 2,
            }
        )
    return components


def group_components(
    components: list[dict[str, object]],
    frame_count: int,
) -> list[list[dict[str, object]]]:
    if not components:
        raise ValueError("no sprite components found")
    largest_area = max(component["area"] for component in components)
    seed_threshold = max(120, largest_area * 0.20)
    seeds = [component for component in components if component["area"] >= seed_threshold]
    if len(seeds) < frame_count:
        seeds = sorted(components, key=lambda item: item["area"], reverse=True)[:frame_count]
    if len(seeds) < frame_count:
        raise ValueError(f"found {len(seeds)} poses; expected {frame_count}")
    seeds = sorted(
        sorted(seeds, key=lambda item: item["area"], reverse=True)[:frame_count],
        key=lambda item: item["center_x"],
    )
    seed_ids = {id(seed) for seed in seeds}
    groups = [[seed] for seed in seeds]
    noise_threshold = max(12, largest_area * 0.002)
    for component in components:
        if id(component) in seed_ids or component["area"] < noise_threshold:
            continue
        nearest = min(
            range(len(seeds)),
            key=lambda index: abs(seeds[index]["center_x"] - component["center_x"]),
        )
        groups[nearest].append(component)
    return groups


def component_group_image(
    source: Image.Image,
    components: list[dict[str, object]],
    padding: int = 4,
) -> Image.Image:
    width, height = source.size
    min_x = max(0, min(item["bbox"][0] for item in components) - padding)
    min_y = max(0, min(item["bbox"][1] for item in components) - padding)
    max_x = min(width, max(item["bbox"][2] for item in components) + padding)
    max_y = min(height, max(item["bbox"][3] for item in components) + padding)
    output = Image.new("RGBA", (max_x - min_x, max_y - min_y), (0, 0, 0, 0))
    source_pixels = source.load()
    output_pixels = output.load()
    for component in components:
        for pixel_index in component["pixels"]:
            x = pixel_index % width
            y = pixel_index // width
            output_pixels[x - min_x, y - min_y] = source_pixels[x, y]
    return output


def split_strip_slots(strip: Image.Image, frame_count: int) -> list[Image.Image]:
    """Split a strip into equal logical slots without discarding local coordinates."""
    if frame_count <= 0:
        raise ValueError("frame count must be positive")
    return [
        strip.crop(
            (
                round(index * strip.width / frame_count),
                0,
                round((index + 1) * strip.width / frame_count),
                strip.height,
            )
        )
        for index in range(frame_count)
    ]


def union_alpha_bbox(images: list[Image.Image]) -> tuple[int, int, int, int]:
    """Return one local-coordinate alpha bound shared by every image."""
    bboxes = [image.getbbox() for image in images]
    if not bboxes or any(bbox is None for bbox in bboxes):
        raise ValueError("empty frame in shared transform state")
    concrete = [bbox for bbox in bboxes if bbox is not None]
    return (
        min(bbox[0] for bbox in concrete),
        min(bbox[1] for bbox in concrete),
        max(bbox[2] for bbox in concrete),
        max(bbox[3] for bbox in concrete),
    )


def crop_to_shared_bbox(
    images: list[Image.Image],
) -> tuple[list[Image.Image], tuple[int, int, int, int]]:
    """Crop every frame with the same box so unchanged pixels remain stationary."""
    bbox = union_alpha_bbox(images)
    return [image.crop(bbox) for image in images], bbox


def place_native_sprite(
    source: Image.Image,
    layout_frame: Image.Image,
    scale: int,
    *,
    preserve_source_canvas: bool = False,
    layout_bbox_override: tuple[int, int, int, int] | None = None,
) -> tuple[Image.Image, dict[str, object]]:
    cell_size = (BASE_CELL_WIDTH * scale, BASE_CELL_HEIGHT * scale)
    target = Image.new("RGBA", cell_size, (0, 0, 0, 0))
    source_bbox = source.getbbox()
    layout_bbox = layout_bbox_override or layout_frame.getbbox()
    if source_bbox is None or layout_bbox is None:
        raise ValueError("empty source or layout frame")
    sprite = source if preserve_source_canvas else source.crop(source_bbox)
    target_box = tuple(round(value * scale) for value in layout_bbox)
    target_width = target_box[2] - target_box[0]
    target_height = target_box[3] - target_box[1]
    resize_scale = min(target_width / sprite.width, target_height / sprite.height, 1.0)
    output_size = (
        max(1, round(sprite.width * resize_scale)),
        max(1, round(sprite.height * resize_scale)),
    )
    if output_size != sprite.size:
        sprite = sprite.resize(output_size, Image.Resampling.LANCZOS)
    left = target_box[0] + (target_width - sprite.width) // 2
    top = target_box[1] + (target_height - sprite.height) // 2
    target.alpha_composite(sprite, (left, top))
    return target, {
        "sourceSpriteSize": [source_bbox[2] - source_bbox[0], source_bbox[3] - source_bbox[1]],
        "sourceCanvasSize": list(source.size),
        "layoutTargetSize": [target_width, target_height],
        "outputSpriteSize": list(output_size),
        "resizeScale": round(resize_scale, 6),
        "upscaled": False,
    }


def make_contact_sheet(atlas: Image.Image, cell_width: int, cell_height: int) -> Image.Image:
    background = Image.new("RGBA", atlas.size, (29, 31, 36, 255))
    checker = Image.new("RGBA", atlas.size, (0, 0, 0, 0))
    pixels = checker.load()
    block = 24
    for y in range(atlas.height):
        for x in range(atlas.width):
            value = 48 if (x // block + y // block) % 2 == 0 else 62
            pixels[x, y] = (value, value, value + 4, 255)
    background.alpha_composite(checker)
    background.alpha_composite(atlas)
    return background


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--scale", type=int, default=2, choices=(2, 3, 4))
    parser.add_argument("--chroma-key")
    parser.add_argument("--key-threshold", type=float, default=96.0)
    parser.add_argument(
        "--shared-transform-state",
        action="append",
        default=[],
        choices=tuple(state for state, _ in ROWS),
        help="repeatable state whose equal strip slots share one crop, scale, and placement",
    )
    parser.add_argument(
        "--write-fallback",
        action="store_true",
        help="also downsample the completed native atlas to official-compatible 1x outputs",
    )
    args = parser.parse_args()

    run_dir = Path(args.run_dir).expanduser().resolve()
    decoded_dir = run_dir / "decoded"
    layout_root = run_dir / "frames"
    final_dir = run_dir / "final"
    qa_dir = run_dir / "qa"
    native_frames = run_dir / f"frames-native-{args.scale}x"
    final_dir.mkdir(parents=True, exist_ok=True)
    qa_dir.mkdir(parents=True, exist_ok=True)
    native_frames.mkdir(parents=True, exist_ok=True)

    request = json.loads((run_dir / "pet_request.json").read_text(encoding="utf-8"))
    key_hex = args.chroma_key or request["chroma_key"]["hex"]
    chroma_key = parse_hex_color(key_hex)
    cell_width = BASE_CELL_WIDTH * args.scale
    cell_height = BASE_CELL_HEIGHT * args.scale
    atlas = Image.new("RGBA", (COLUMNS * cell_width, len(ROWS) * cell_height), (0, 0, 0, 0))
    provenance_rows = []
    shared_transform_states = set(args.shared_transform_state)

    for row_index, (state, frame_count) in enumerate(ROWS):
        strip_path = decoded_dir / f"{state}.png"
        with Image.open(strip_path) as opened:
            raw_size = list(opened.size)
            strip = remove_chroma_background(opened, chroma_key, args.key_threshold)
        layouts = []
        for column in range(frame_count):
            layout_path = layout_root / state / f"{column:02d}.png"
            with Image.open(layout_path) as opened:
                layouts.append(opened.convert("RGBA"))
        placement_mode = "per-frame-components"
        shared_source_bbox = None
        shared_layout_bbox = None
        if state in shared_transform_states:
            sources, shared_source_bbox = crop_to_shared_bbox(
                split_strip_slots(strip, frame_count)
            )
            shared_layout_bbox = union_alpha_bbox(layouts)
            placement_mode = "shared-slot-transform"
        else:
            groups = group_components(connected_components(strip), frame_count)
            sources = [component_group_image(strip, group) for group in groups]
        state_dir = native_frames / state
        state_dir.mkdir(parents=True, exist_ok=True)
        frame_records = []
        for column, source in enumerate(sources):
            frame, frame_record = place_native_sprite(
                source,
                layouts[column],
                args.scale,
                preserve_source_canvas=state in shared_transform_states,
                layout_bbox_override=shared_layout_bbox,
            )
            frame_path = state_dir / f"{column:02d}.png"
            frame.save(frame_path)
            atlas.alpha_composite(frame, (column * cell_width, row_index * cell_height))
            frame_record["frame"] = column
            frame_record["path"] = str(frame_path)
            frame_records.append(frame_record)
        provenance_rows.append(
            {
                "state": state,
                "row": row_index,
                "sourcePath": str(strip_path),
                "sourceSha256": sha256(strip_path),
                "sourceSize": raw_size,
                "placementMode": placement_mode,
                "sharedSourceCrop": list(shared_source_bbox) if shared_source_bbox else None,
                "sharedLayoutTarget": list(shared_layout_bbox) if shared_layout_bbox else None,
                "frames": frame_records,
            }
        )

    png_path = final_dir / f"spritesheet-native-{args.scale}x.png"
    webp_path = final_dir / f"spritesheet-native-{args.scale}x.webp"
    atlas.save(png_path, optimize=True)
    atlas.save(webp_path, format="WEBP", lossless=True, method=6)
    fallback_record = None
    if args.write_fallback:
        fallback = atlas.resize(
            (COLUMNS * BASE_CELL_WIDTH, len(ROWS) * BASE_CELL_HEIGHT),
            Image.Resampling.LANCZOS,
        )
        fallback_png_path = final_dir / "spritesheet.png"
        fallback_webp_path = final_dir / "spritesheet.webp"
        fallback.save(fallback_png_path, optimize=True)
        fallback.save(fallback_webp_path, format="WEBP", lossless=True, method=6)
        fallback_record = {
            "generationPath": f"native-{args.scale}x-atlas-downsample",
            "scale": 1,
            "pngPath": str(fallback_png_path),
            "pngSha256": sha256(fallback_png_path),
            "webpPath": str(fallback_webp_path),
            "webpSha256": sha256(fallback_webp_path),
        }
    contact_path = qa_dir / f"contact-sheet-native-{args.scale}x.png"
    make_contact_sheet(atlas, cell_width, cell_height).save(contact_path, optimize=True)
    provenance = {
        "schemaVersion": 1,
        "generationPath": "decoded-strips-direct",
        "canonicalFramesUsedForPixels": False,
        "canonicalFramesUsedForLayoutOnly": True,
        "upscalingAllowed": False,
        "scale": args.scale,
        "cellSize": [cell_width, cell_height],
        "atlasSize": list(atlas.size),
        "chromaKey": key_hex.upper(),
        "keyThreshold": args.key_threshold,
        "pngPath": str(png_path),
        "pngSha256": sha256(png_path),
        "webpPath": str(webp_path),
        "webpSha256": sha256(webp_path),
        "fallback": fallback_record,
        "rows": provenance_rows,
    }
    provenance_path = final_dir / f"native-{args.scale}x-provenance.json"
    provenance_path.write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "ok": True,
                "webp": str(webp_path),
                "provenance": str(provenance_path),
                "contactSheet": str(contact_path),
                "fallback": fallback_record,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
