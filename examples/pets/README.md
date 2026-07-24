# Example pets

Each child directory is a complete pet package that CoPets can import directly.

- [`sunflower`](sunflower): a smooth Retina sunflower with a Codex-compatible 1x atlas and an optional CoPets-native 2x atlas.
- [`sunflower-gloves`](sunflower-gloves): a brown-pot Sunflower variant with bold stem-mounted rubber-hose gloves, a compatible 1x atlas, and a native Retina 2x atlas.

In CoPets Settings, choose **Import pet** and select either the package directory or its `pet.json` file.

Each package includes `provenance.json` with its license, generation class, atlas construction path,
dimensions, and SHA-256 hashes. Prompts, working strips, and absolute local paths remain outside the
public tree.
