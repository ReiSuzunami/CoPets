import importlib.util
import unittest
from pathlib import Path

from PIL import Image, ImageDraw


SCRIPT_PATH = Path(__file__).parents[1] / "scripts" / "build_native_atlas.py"
SPEC = importlib.util.spec_from_file_location("build_native_atlas", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class SharedTransformTests(unittest.TestCase):
    def test_shared_crop_preserves_stationary_body_coordinates(self):
        frames = []
        for petal_radius in (4, 9):
            frame = Image.new("RGBA", (24, 40), (0, 0, 0, 0))
            draw = ImageDraw.Draw(frame)
            draw.rectangle((11, 13, 12, 34), fill=(0, 120, 90, 255))
            draw.rectangle((7, 34, 16, 38), fill=(0, 30, 80, 255))
            draw.ellipse(
                (12 - petal_radius, 12 - petal_radius, 12 + petal_radius, 12 + petal_radius),
                fill=(255, 140, 0, 255),
            )
            frames.append(frame)

        cropped, shared_bbox = MODULE.crop_to_shared_bbox(frames)

        self.assertEqual(cropped[0].size, cropped[1].size)
        self.assertEqual(shared_bbox, (3, 3, 22, 39))
        for point in ((8, 31), (9, 35)):
            self.assertEqual(cropped[0].getpixel(point), cropped[1].getpixel(point))

    def test_shared_placement_uses_identical_canvas_transform(self):
        frames = []
        for petal_radius in (4, 9):
            frame = Image.new("RGBA", (24, 40), (0, 0, 0, 0))
            draw = ImageDraw.Draw(frame)
            draw.rectangle((11, 13, 12, 34), fill=(0, 120, 90, 255))
            draw.rectangle((7, 34, 16, 38), fill=(0, 30, 80, 255))
            draw.ellipse(
                (12 - petal_radius, 12 - petal_radius, 12 + petal_radius, 12 + petal_radius),
                fill=(255, 140, 0, 255),
            )
            frames.append(frame)
        cropped, _ = MODULE.crop_to_shared_bbox(frames)
        layout = Image.new("RGBA", (192, 208), (0, 0, 0, 0))
        ImageDraw.Draw(layout).rectangle((20, 20, 38, 55), fill=(255, 255, 255, 255))

        placed = [
            MODULE.place_native_sprite(
                frame,
                layout,
                1,
                preserve_source_canvas=True,
                layout_bbox_override=(20, 20, 39, 56),
            )[0]
            for frame in cropped
        ]

        stationary_color = (0, 30, 80, 255)
        for y in range(208):
            for x in range(192):
                self.assertEqual(
                    placed[0].getpixel((x, y)) == stationary_color,
                    placed[1].getpixel((x, y)) == stationary_color,
                )


if __name__ == "__main__":
    unittest.main()
