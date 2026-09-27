# Contributing a plugin

This is the long version of [Submit it to the store](README.md#submit-it-to-the-store).
All you need is Node.js, the Stride CLI from npm and a Rust toolchain. You do
not need a Stride checkout, a server, or any key.

```sh
npm i -g @guuslab/stride        # or prefix every command with: npx @guuslab/stride
rustup target add wasm32-unknown-unknown
```

The CLI runs on macOS (Apple silicon), Linux x64 and Linux arm64.
`stride upgrade` keeps it current.

## What the store accepts

- **Rust plugins, built from `sources/<id>` in this repository.** CI rebuilds
  every listed module from that source and refuses it unless the SHA-256
  matches the signed release byte for byte. The reference build,
  [`tools/build-plugin.sh`](tools/build-plugin.sh), is Rust only.
- **TypeScript plugins are not listed here yet.** `stride plugin new <id> --lang ts`
  works, and so do `stride plugin dev` and `stride plugin test`. You can
  serve a TypeScript plugin from [your own store](README.md#run-your-own-store).
  The public store has no reproducible TypeScript build yet. Open an issue if
  you want one listed.
- **One plugin per pull request**, with an id that is not taken: 3 to 48
  characters of `a-z`, `0-9` and single dashes.

## Step by step

### 1. Fork, clone, scaffold

```sh
git clone https://github.com/<you>/stride-plugins
cd stride-plugins
stride plugin new my-plugin --dir sources/my-plugin --pdk ../../pdk-rust
cp tools/rust-toolchain.toml sources/my-plugin/
(cd sources/my-plugin && cargo generate-lockfile)
```

- `--pdk ../../pdk-rust` uses the Rust PDK vendored in this repository
  instead of a copy, so the source rebuilds from this repository alone.
- The toolchain file pins the Rust version the reference build uses.
- `Cargo.lock` has to be committed, because the reference build runs
  `cargo build --locked`.

### 2. Fill in the manifest

In `sources/my-plugin/stride-plugin.json`:

| Field | Value |
|---|---|
| `publisher` | `"guuslab"`: the maintainer signs store releases with the GuusLab publisher key (see [Keys](#keys)) |
| `homepage` | `"https://github.com/guuslab/stride-plugins/tree/main/sources/my-plugin"` |
| `description` | One or two plain sentences a site owner understands |
| `permissions` | As few as possible. `storage` covers settings panels |
| `hooks`, `panels` | Exactly what the module exports; `stride plugin test` checks this |

In `sources/my-plugin/README.md`, say what the plugin does and why it needs
each permission. Reviewers read this file together with the permission
screen, and so does whoever installs the plugin. Credit yourself here too.

### 3. Build it the way CI does, and test it

```sh
tools/build-plugin.sh my-plugin          # writes sources/my-plugin/plugin.wasm
stride plugin dev sources/my-plugin      # run every hook and panel in the sandbox, print the output
stride plugin test sources/my-plugin     # the registry's checks; exits non-zero on failure
```

Use `tools/build-plugin.sh` rather than the scaffold's `build.sh`. It adds
the `--remap-path-prefix` flags that keep your home directory out of the
module.

Your local `plugin.wasm` can still differ by a few bytes from the reference
build, because the reference host is x86_64 Linux. That is expected: the
maintainer commits the reference build when publishing.

To see the plugin on a real page, `node tools/preview.mjs my-plugin page.html
out.html [values.json]` runs its `on_page_render` with seeded settings.
Run `npm install` once first. The same tool makes good screenshots.

### 4. Add the store listing

```
plugins/my-plugin/plugin.json          manifest, publisher and store; no release
plugins/my-plugin/icon.png             512x512 PNG, at most 256 KB
plugins/my-plugin/screenshots/NN.png   1600x1000 PNG, at most 1.5 MB each, two to five
```

`plugin.json`:

```json
{
  "manifest": { "...": "sources/my-plugin/stride-plugin.json, verbatim" },
  "publisher": {
    "id": "guuslab",
    "name": "GuusLab",
    "publicKey": "13b4051247c00f67a9efe7e1235ac46e49af00912c9806f33472bd38e31b1694"
  },
  "store": {
    "tagline": "One line, at most 80 characters",
    "longDescription": "Plain text. Paragraphs separated by a blank line. At most 4000 characters.",
    "icon": "icon.png",
    "screenshots": [
      { "file": "screenshots/01.png", "caption": "At most 120 characters" },
      { "file": "screenshots/02.png", "caption": "At most 120 characters" }
    ],
    "categories": ["content"],
    "accent": "#4F46E5"
  }
}
```

- The `manifest` must be identical to `stride-plugin.json`. The publish
  script refuses any difference.
- `publisher` is a copy of [`publishers/guuslab.json`](publishers/guuslab.json)
  without `homepage`.
- Leave `release` out. The maintainer adds it when signing.
- Categories: `content`, `marketing`, `seo`, `social`, `commerce`,
  `privacy`, `design`, `analytics`, `developer`, `utilities`.
- `accent` is `#RRGGBB`.
- The only PNGs allowed under `plugins/<id>/` are `icon.png` and
  `screenshots/NN.png`.

`node tools/icon.mjs icon.svg plugins/my-plugin/icon.png` renders an SVG at
512x512. Add `1600 1000` at the end for a screenshot.

### 5. Check and open the pull request

```sh
node tools/verify.mjs --no-build     # store metadata and images, as CI checks them
```

Commit `sources/my-plugin/` (including `Cargo.lock` and
`rust-toolchain.toml`, but not `target/`) and `plugins/my-plugin/`, then open
a pull request. Say in its description what the plugin does and why it needs
each permission.

## What happens next

On your pull request, the **verify** workflow
([`.github/workflows/verify.yml`](.github/workflows/verify.yml), running
`node tools/verify.mjs`) checks:

- **Your store metadata and images:** tagline, description and caption
  lengths, categories, accent, and the PNG sizes and dimensions.
- **Every plugin already listed:** it rebuilds each one from source and
  compares the hash with its signed release.

Your new source is not rebuilt yet, because it has no release. That happens
at publishing.

Then a maintainer:

1. **Reviews the permissions** against your description and README:
   - Does it explain *why* it needs each one?
   - Could it do the job with less?
   - Does the hook do what the description says?
   - Does it cope when a permission is refused?

   This is the check that cannot be automated.
2. **Merges, builds and signs.** The maintainer runs the reference build on
   x86_64 Linux and signs the release with the GuusLab publisher key.
3. **Publishes.** The maintainer signs the store index with the registry key
   and publishes it. From then on, CI rebuilds your source on every push and
   compares it with that release.

You never need the registry key or the publisher key. Both stay with the
maintainer and never enter this repository.

## Updating a plugin

Releases are immutable. To change a listed plugin:

1. Bump `version` in `stride-plugin.json` and in the `manifest` in
   `plugins/<id>/plugin.json`.
2. Make the change and run `stride plugin test`.
3. Open a pull request. It adds a new release next to the old one; it does
   not replace it.

## Keys

Signing with the maintainer's key is the default and needs nothing from you.
If you want releases signed with your own publisher key:

```sh
stride plugin keygen --out ~/.stride/<you>.key    # prints the public half; keep the file private
```

Your first pull request then also adds `publishers/<you>.json`:

```json
{ "id": "<you>", "name": "...", "publicKey": "<hex>", "homepage": "..." }
```

Set `publisher` to your id in both manifests, and sign with:

```sh
stride plugin publish sources/<id> --key ~/.stride/<you>.key --base-url https://guuslab.github.io/stride-plugins
```

This prints the submission, including a signed `release`.

The catch is the `contentHash`. It has to be the hash of the reference build:
x86_64 Linux, with this repository checked out at
`/home/runner/work/stride-plugins/stride-plugins`. The container command in
[`tools/publish-new.mjs`](tools/publish-new.mjs) reproduces that build.
Unless you need your own key, leave signing to the maintainer.

Never commit a key: `*.key` is ignored for a reason.

## Running your own store

See [Run your own store](README.md#run-your-own-store). The same CLI builds
and signs a private registry, and TypeScript plugins are welcome there.
