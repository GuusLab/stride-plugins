# AGENTS.md

Guide for AI coding agents working in this repository.

## What this is

The official plugin registry and store for [Stride](https://github.com/guuslab/stride).
GitHub Pages serves `site/`: a signed `index.json`, the WebAssembly modules,
store images and a gallery page. Every listed plugin is rebuilt from source in
CI and must match its published SHA-256 byte for byte.

## Layout

```
plugins/<id>/     submissions: plugin.json (publisher, manifest, release, store), icon, screenshots
sources/<id>/     first-party plugin sources (Rust, wasm32-unknown-unknown)
pdk-rust/         vendored Stride Rust PDK
publishers/       publisher records
site/             what GitHub Pages serves (index.json, modules, images, index.html)
tools/            build, verify, render and demo scripts
.github/workflows verify.yml (rebuild and check), pages.yml (deploy site/)
```

## Build and test

- `npm install` once for the Node tools.
- `tools/build-plugin.sh <id>`: reproducible release build; prints SHA-256 and size.
- `node tools/verify.mjs`: what CI runs. Rebuilds every source, compares hashes,
  checks store images and the published copies under `site/`.
- `node tools/gallery.mjs`: regenerates `site/index.html` and the README plugin table.
- The reference build host is x86_64 Linux. A macOS build can differ by a few
  bytes; publish the modules from the verify workflow's `rebuilt-modules` artifact.
- On small machines, limit Cargo: `CARGO_BUILD_JOBS=3`.

## Publishing and signing

- Anything under `plugins/*/plugin.json` or `site/index.json` is signed. Changing
  a manifest, release or store field means re-signing: rebuild the index with
  `stride plugin index . --key ~/.stride/stride-registry.key --base-url https://guuslab.github.io/stride-plugins`,
  then run `node tools/gallery.mjs`. If you cannot re-sign, do not touch signed fields.
- Keys live in `~/.stride/` only. Never copy, print, commit or upload a key,
  and never add one to CI.
- Changing plugin code changes its hash: bump the version and publish a new release.

## Writing standard

- English only: code, comments, docs, commit messages, logs and errors.
  The only exception is localized user-facing text a plugin ships on purpose
  (for example the Dutch defaults in Cookie Consent); that must read as natural Dutch.
- UI and store copy: short, clear, human, sentence case, verbs on buttons, no
  jargon a site owner would not know, one word per concept (page, post, site,
  theme, plugin), errors that say what went wrong and how to fix it.
- Docs: skimmable, accurate, honest about limits.
- Comments explain why, not what. Remove stale ones; do not rewrite correct ones for style.
- Commit messages: imperative summary line, English.
