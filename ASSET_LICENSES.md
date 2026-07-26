# Asset licensing and provenance

The repository MIT license covers the source and the first-party assets explicitly designated for
MIT distribution in this file and their package-local provenance records. This inventory records
what maintainers know about an asset's contribution and build route. It is not independent copyright
clearance, legal advice, or a substitute for the terms of a creation tool or service.

## Contribution representation

Anyone who contributes an image, sprite atlas, icon, font, sound, or other asset represents that
they have authority to contribute it under the repository MIT license. Contributions must not
include private working material, prompts, user data, or third-party assets without a documented
right to redistribute them.

## Asset inventory

| Asset family | Public paths | Provenance | Distribution statement |
| --- | --- | --- | --- |
| CoPets cloud identity | `docs/assets/brand/copets-cloud.png`, `src-tauri/icons/*` | [`copets-cloud.provenance.json`](docs/assets/brand/copets-cloud.provenance.json) | Repository-maintainer MIT contribution assertion; icons are generated derivatives of the cloud artwork. |
| Sunflower example pet | `examples/pets/sunflower/*` | [`provenance.json`](examples/pets/sunflower/provenance.json) | Contributor-designated MIT example package; generation/build facts and hashes are recorded. |

The examples are installable source packages, not automatically installed application content.

## Rights concerns

Do not open a public issue containing a disputed asset, private material, or personal data. Use the
repository Security page's private reporting flow and include the affected path plus a concise
rights explanation.
