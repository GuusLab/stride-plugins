# External Links

Links to other websites open in a new tab with `rel="noopener noreferrer"`,
an optional `nofollow` or `sponsored`, a small arrow and a hidden "(opens in a
new tab)" for screen readers. No JavaScript.

## What it adds to a page

- Rewrites `<a href="http(s)://…">` tags whose host is not the site's own,
  once, at publish time. Everything else in the tag is kept; `rel` tokens are
  merged, and a link with its own `target` keeps it.
- The site's own host is remembered from `on_publish`, which reports the
  page's full address once the site has a primary domain, and read from
  `<link rel="canonical">` or `og:url` when a page has one; subdomains count
  as the site too. Relative links (`/contact`) are always the site's own.
- The arrow is an inline SVG, left out for links that wrap an image or an icon.
- Link-shaped text inside `<script>`, `<style>`, `<textarea>` and `<template>`
  is left alone.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Handle links to other sites | on | Master switch. |
| Open them in a new tab | on | Adds `target="_blank"` and `noopener noreferrer`. |
| Show a small arrow after the link | on | |
| Search engines | Let them follow | Or `nofollow`, or `nofollow sponsored`. |
| Your other domains | empty | Also treated as your own, e.g. `shop.example.com`. |
| Leave these domains alone | empty | Links to them are never changed. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings and the site's domain learned from `on_publish`. Without it the defaults apply. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh external-links
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
