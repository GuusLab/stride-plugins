# Glossary

Terms listed in the settings get a dotted underline in running text; pointing
at one, tapping it or tabbing to it shows its explanation.

## What it adds to a page

- Each marked term becomes a `<button>` with a tooltip right after it; a
  stylesheet in `<head>` and a small script for tapping, Escape and flipping
  the tooltip below the word near the top of the screen.
- Only text inside `<main>` (or `<body>`) and outside `a`, headings,
  `button`, `code`, `pre`, `kbd`, `script`, `style`, `textarea`, `select`,
  `abbr`, `dialog`, `nav`, `header`, `footer` and `svg`. Tags and attributes
  are never changed. Whole words only, longest spelling first.

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Explain terms in the page text | on | Master switch. |
| Terms | empty | `Term: explanation`, one per line; `A | B: …` for other spellings. |
| Mark every mention | off | Otherwise only the first on each page. |
| Underline colour | `#6D28D9` | |
| Skip these pages | empty | Comma-separated slugs; a trailing `*` matches every page under it. |

At most 200 terms. Text whose lowercase form changes length (some non-Latin scripts) is left alone.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep your terms. Without it nothing is marked. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh glossary
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
