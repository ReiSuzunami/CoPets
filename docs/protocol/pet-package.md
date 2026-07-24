# Pet package compatibility

> Status: Normative
> Owns: Pet discovery, import sources, package installation, manifest fields, atlas geometry, validation, and native-resolution generation
> Update when: `PetPackageManager`, `PetManifest`, import limits, geometry validation, atlas rows, or generation output changes
> Last verified: 2026-07-24

## Discovery

Packages live at:

```text
${CODEX_HOME:-~/.codex}/pets/<id>/pet.json
```

Folder name and manifest `id` must match when loading. [`src-tauri/src/pet.rs`](../../src-tauri/src/pet.rs) is the parser and validation source of truth.

## Import and local management

The settings panel accepts three package selections:

- A folder containing `pet.json`.
- The `pet.json` file itself; its containing folder is the package.
- A `.zip` containing the package at its root or inside one wrapping folder.

A package must contain exactly one discoverable `pet.json`. A spritesheet by itself is not a package because it lacks identity, version, geometry selection, and attribution metadata. Importing never changes the selected source; CoPets copies validated package data into the discovery path above.

### ZIP container contract

A portable ZIP uses one of these two layouts. The ZIP filename and optional wrapping-folder name are descriptive only; installation identity comes from `pet.json.id`.

```text
example-pet.zip
├── pet.json
├── spritesheet.webp
└── spritesheet-native-2x.webp   # optional CoPets-native atlas
```

```text
example-pet.zip
└── example-pet/
    ├── pet.json
    ├── spritesheet.webp
    └── spritesheet-native-2x.webp
```

The portable contract requires exactly one lowercase file named `pet.json`, either at archive root or one folder below it. A second wrapper level such as `outer/example-pet/pet.json`, a multi-pet collection, or a standalone spritesheet is not a valid package. Hidden paths and `__MACOSX` do not participate in manifest discovery. Only the chosen package root is installed.

All asset paths named by `pet.json` are relative to that package root. PNG and WebP spritesheets may use any filename; the manifest supplies the filename. Extra regular files are copied but have no runtime meaning unless a manifest field references them. ZIP packages are data-only: CoPets does not execute files from a package.

| ZIP constraint | Limit or rule |
| --- | --- |
| Input archive size | At most 128 MiB |
| Expanded regular-file data | At most 128 MiB total |
| Entries | At most 512 files and directories |
| Referenced spritesheet | At most 64 MiB |
| Manifest position | Archive root or exactly one wrapping folder |
| Entry names | Unique without case distinctions; `/` separators only |
| Unsupported data | Encryption, overlapping entries, symbolic links, special files, absolute paths, and parent-directory escapes |

After validation, CoPets installs the selected package as `${CODEX_HOME:-~/.codex}/pets/<pet.json.id>/`. Replacing an existing package is therefore keyed by manifest `id`, not by the ZIP filename.

`PetPackageManager` stages every install under the hidden `.copets-staging` directory in the same pets filesystem. It validates the staged copy before activation. New installs become visible with one rename. On macOS, replacement swaps the staged and installed directories atomically, then deletes the old copy. A validation or activation failure leaves the installed package unchanged. Removal first renames the installed package out of discovery and restores it if deletion fails.

Import rejects:

- ZIPs that violate any size, entry-count, path, type, encryption, overlap, or duplicate-name constraint above.
- ZIPs whose supported wrapper level contains ambiguous manifest candidates, or whose manifest is nested under more than one wrapping folder.
- Packages that fail the manifest, media, or atlas checks below.

Invalid folders placed manually under `pets/` are omitted from the selectable catalog and reported in settings. An installed package entry must be a real direct child directory; a directory symlink cannot escape the pets root. Source paths are transient native-dialog inputs; CoPets does not persist them or send them back as package metadata.

## Manifest: `pet.json`

`pet.json` is the package manifest and index, not the animation itself. It gives the package a stable identity and user-facing metadata, tells a compatible renderer which atlas layout to use, and points to the image files that contain the pixels. Without it, CoPets cannot safely name, install, validate, or slice a spritesheet.

It does not contain executable code, individual frame pixels, state timing, or lifecycle rules. State-to-row mapping and playback behavior remain part of the compatible pet protocol and CoPets runtime.

The file is strict UTF-8 JSON: keys and string values require double quotes, and comments or trailing commas are invalid. Field names use the camel-case spelling shown below. The `id` is 1 to 128 UTF-8 bytes, cannot be `.` or `..`, and cannot contain `/`, `\`, or control characters.

```json
{
  "id": "example-pet",
  "displayName": "Example Pet",
  "description": "A desktop companion",
  "spriteVersionNumber": 2,
  "spritesheetPath": "spritesheet.webp",
  "sidecarSpritesheetPath": "spritesheet-native-2x.webp"
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | Yes | Stable package and replacement identifier; must match folder name after installation |
| `displayName` | Yes | User-visible pet name |
| `description` | Yes | User-visible package description |
| `spriteVersionNumber` | No | Selects atlas geometry; missing means V1, and supported values are 1 and 2 |
| `spritesheetPath` | Yes | Relative path to the official-compatible atlas |
| `sidecarSpritesheetPath` | No | Legacy-named field pointing to the preferred CoPets-native atlas; official consumers ignore it |

Paths must be relative, cannot contain a parent-directory escape, and must resolve inside the package folder. Symlink resolution is checked before load.

## Atlas contract

| Version | Grid | Base atlas | Base cell | Rows |
| --- | --- | --- | --- | --- |
| V1 | 8 x 9 | 1536 x 1872 | 192 x 208 | Core state rows |
| V2 | 8 x 11 | 1536 x 2288 | 192 x 208 | V1 rows plus two look-direction rows |

CoPets accepts integer scales 1x through 4x while preserving the 192:208 cell ratio and row count. Therefore atlas width is `1536 * scale`; height is `1872 * scale` for V1 or `2288 * scale` for V2.

The loader also requires:

- PNG or WebP media.
- A supported V1/V2 grid.
- Integer-scaled cell dimensions.
- A file no larger than the limit enforced by `load_pet`.

Invalid packages are excluded from listing when possible and return a descriptive error when loaded directly.

## Native atlas generation

[`scripts/build_native_atlas.py`](../../scripts/build_native_atlas.py) assembles a 2x, 3x, or 4x atlas directly from generated high-resolution strips. Official 1x frames supply layout bounds only; their pixels are never enlarged into the native atlas.

Required Hatch Pet run structure:

```text
<run>/
  pet_request.json
  decoded/<state>.png
  frames/<state>/<frame>.png
```

Build:

```bash
python3 scripts/build_native_atlas.py --run-dir /absolute/path/to/run --scale 2
```

For an animation whose stationary geometry must remain pixel-locked while its silhouette changes,
repeat `--shared-transform-state <state>`. That state is split by equal source slots and every frame
uses one shared crop, scale, and placement. Add `--write-fallback` to downsample the completed native
atlas into the official-compatible `spritesheet.png` and `spritesheet.webp` outputs.

Outputs:

```text
final/spritesheet-native-<scale>x.png
final/spritesheet-native-<scale>x.webp
final/native-<scale>x-provenance.json
qa/contact-sheet-native-<scale>x.png
frames-native-<scale>x/<state>/<frame>.png
```

The provenance report records input hashes, source/output sizes, placement scale, atlas hashes, and the invariant that source sprites were not upscaled. Treat it as generated evidence, not the protocol definition.

## Compatibility rule

Keep `spritesheetPath` official-compatible. Add the legacy-named `sidecarSpritesheetPath` only for enhanced CoPets rendering. Never replace the official sheet with incompatible geometry or add CoPets-only meanings to official fields.

When this contract changes, update the loader tests, renderer atlas tests, this document, the [feature catalog](../features/catalog.md), and the changelog according to [updating rules](../maintenance/updating.md).
