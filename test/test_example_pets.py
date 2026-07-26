import hashlib
import json
from pathlib import Path
import unittest

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "examples" / "pets"


class ExamplePetTests(unittest.TestCase):
    def test_example_packages_match_their_public_provenance(self):
        packages = sorted(path for path in EXAMPLES.iterdir() if path.is_dir())
        self.assertEqual([path.name for path in packages], ["sunflower"])

        for package in packages:
            with self.subTest(package=package.name):
                manifest = json.loads((package / "pet.json").read_text())
                provenance = json.loads((package / "provenance.json").read_text())

                self.assertEqual(manifest["id"], package.name)
                self.assertEqual(manifest["spriteVersionNumber"], 1)
                self.assertEqual(provenance["assetLicense"], "MIT")
                self.assertFalse(
                    provenance["generation"]["nativeAtlas"]["upscalingAllowed"]
                )

                expected = {
                    manifest["spritesheetPath"]: (1536, 1872),
                    manifest["sidecarSpritesheetPath"]: (3072, 3744),
                }
                for filename, size in expected.items():
                    asset = package / filename
                    record = provenance["files"][filename]
                    self.assertEqual((record["width"], record["height"]), size)
                    self.assertEqual(hashlib.sha256(asset.read_bytes()).hexdigest(), record["sha256"])
                    with Image.open(asset) as image:
                        self.assertEqual(image.size, size)
                        self.assertEqual(image.format, "WEBP")


if __name__ == "__main__":
    unittest.main()
