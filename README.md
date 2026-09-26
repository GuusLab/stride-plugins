<div align="center">

# Stride Plugins

**The official plugin registry and store for [Stride](https://github.com/guuslab/stride).**

Every plugin here is WebAssembly, sandboxed, permission-scoped, signed, and
rebuilt from source byte for byte before it is listed.

[Browse the index](https://guuslab.github.io/stride-plugins/index.json) ·
[Submit a plugin](#submitting-a-plugin) ·
[Registry format](https://github.com/guuslab/stride/blob/main/docs/plugin-registry.md)

</div>

---

## Installing from the store

1. In the Stride editor, open **Plugins**.
2. Browse or search the store. Each listing shows the icon, screenshots, what
   the plugin does and exactly which permissions it asks for.
3. Click **Install**, review the permission screen and untick anything you do
   not want to grant. A plugin that is refused a permission is told "no" at
   run time; it never gets to ask again behind your back.
4. **Enable** it. Plugins are always installed disabled.

Your server downloads the module itself, checks its SHA-256 against the signed
index and its publisher's signature, and refuses anything that does not match.

For self-hosted installations, point Stride at this registry:

```sh
STRIDE_PLUGIN_REGISTRY_URL=https://guuslab.github.io/stride-plugins/index.json
STRIDE_PLUGIN_REGISTRY_KEY=eb8b920592e7f58a9a76ab0e357e13e6492ddb37f7aff42a3d8416c9a087f8a2
```

## Plugins

| | Plugin | What it does | Permissions |
|---|---|---|---|
| <img src="plugins/announcement/icon.png" width="40" alt=""> | [**Announcement Bar**](sources/announcement) | One clear announcement bar at the top of every page | `storage` |
| <img src="plugins/cookie-consent/icon.png" width="40" alt=""> | [**Cookie Consent**](sources/cookie-consent) | An accessible GDPR cookie banner in Dutch or English, with no third parties | `storage` |
| <img src="plugins/maintenance-mode/icon.png" width="40" alt=""> | [**Maintenance Mode**](sources/maintenance-mode) | A polished coming-soon page for visitors while editors see the real site | `storage` |
| <img src="plugins/reading-progress/icon.png" width="40" alt=""> | [**Reading Progress**](sources/reading-progress) | A slim reading progress bar and a back-to-top button for long pages. | `storage` |
| <img src="plugins/reading-time/icon.png" width="40" alt=""> | [**Reading Time**](sources/reading-time) | Shows readers how long a page takes, like "4 min read", under the title. | `storage` |
| <img src="plugins/schema-markup/icon.png" width="40" alt=""> | [**Schema Markup**](sources/schema-markup) | JSON-LD for your business, website and blog posts, ready for Google rich results | `storage` |
| <img src="plugins/seo-inspector/icon.png" width="40" alt=""> | [**SEO Inspector**](sources/seo-inspector) | A 0-100 SEO score and clear warnings for every page, checked on save. | `storage`, `write-pages` |
| <img src="plugins/social-share/icon.png" width="40" alt=""> | [**Social Share**](sources/social-share) | Privacy-friendly share buttons under every article. No trackers. | `storage` |
| <img src="plugins/table-of-contents/icon.png" width="40" alt=""> | [**Table of Contents**](sources/table-of-contents) | A linked table of contents for long pages, built from your headings. | `storage` |
| <img src="plugins/whatsapp-chat/icon.png" width="40" alt=""> | [**WhatsApp Chat**](sources/whatsapp-chat) | A floating WhatsApp button with a preset message and opening hours | `storage` |

## Submitting a plugin

The full format and the list of checks live in the Stride repository, in
[`docs/plugin-registry.md`](https://github.com/guuslab/stride/blob/main/docs/plugin-registry.md).
In short, open a pull request that adds:

```
plugins/<id>/plugin.json          your submission (publisher, manifest, release, optional "store")
plugins/<id>/icon.png             512x512 PNG, at most 256 KB
plugins/<id>/screenshots/NN.png   1600x1000 PNG, at most 1.5 MB, two to five of them
```

The optional `store` object in `plugin.json` is what makes your listing look good:

```json
"store": {
  "tagline": "One line, at most 80 characters",
  "longDescription": "Plain text. Paragraphs separated by a blank line. At most 4000 characters.",
  "icon": "icon.png",
  "screenshots": [{ "file": "screenshots/01.png", "caption": "At most 120 characters" }],
  "categories": ["content"],
  "accent": "#4F46E5"
}
```

Categories: `content`, `marketing`, `seo`, `social`, `commerce`, `privacy`,
`design`, `analytics`, `developer`, `utilities`.

CI then rebuilds your module in a clean container and refuses the submission
unless its SHA-256 equals the `contentHash` you published. Pin the toolchain,
commit `Cargo.lock`, and build with `--remap-path-prefix` (see
`tools/build-plugin.sh`) and it will.

## This repository

```
plugins/<id>/        submissions and store images
sources/<id>/        source of the first-party plugins (Rust, wasm32-unknown-unknown)
pdk-rust/            the Stride Rust PDK, vendored so sources/ rebuilds on its own
publishers/          publisher records
site/                what GitHub Pages serves: index.json, modules, images
tools/               build, render, demo and verification scripts
```

| Tool | Usage |
|---|---|
| `tools/build-plugin.sh <id>` | reproducible release build of `sources/<id>` into `sources/<id>/plugin.wasm`, prints SHA-256 and size |
| `node tools/icon.mjs <in.svg> <out.png> [w h]` | render an SVG to PNG (512x512 by default; `1600 1000` for screenshots) |
| `tools/demo-server.sh <port> <workdir>` | start a throwaway Stride with a fresh database |
| `node tools/install-local.mjs <port> <sourceDir>` | sideload, grant and enable a plugin on that server |
| `node tools/verify.mjs` | what CI runs: rebuild and compare hashes, check images and the published copies under `site/` |
| `node tools/gallery.mjs` | render `site/index.html` and this README's plugin table from `site/index.json` |

Run `npm install` once for the Node tools.

Publishing (maintainers): after merging a submission, copy the module and
manifest to `site/plugins/<id>/<version>/` and the images to `site/plugins/<id>/`,
then run `stride plugin index . --key ~/.stride/stride-registry.key --base-url
https://guuslab.github.io/stride-plugins` and `node tools/gallery.mjs`, and push.
The registry key never enters this repository.

## Licence

The registry tooling and first-party plugins are [MIT](LICENSE). Third-party
plugins carry the licence stated in their own manifest.
