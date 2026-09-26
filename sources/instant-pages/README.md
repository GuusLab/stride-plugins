# Instant Pages

Pages start loading while a visitor points at a link, and cross-fade into each
other. Two tags in `<head>`, no script.

## What it adds to a page

- A `<script type="speculationrules">` block (JSON, not JavaScript) asking the
  browser to prerender, or only prefetch, same-site links on hover or press.
- `@view-transition { navigation: auto }` for a cross-fade, or a fade and
  slide, switched off under `prefers-reduced-motion`.
- Never loaded early: `/admin`, `/api/`, `/members/`, `/account`,
  `/checkout`, `/webhooks/`, `/mcp`, `/media/`, the feed, sitemap and
  robots files, links with a query string, `target="_blank"`, `download`,
  `rel="nofollow"` or `data-no-instant`, and any address you add.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Make pages feel instant | on | Master switch. |
| Start loading a page | When the pointer rests on a link | Or when a link is pressed. |
| How much to load ahead | Prepare the whole page | Or only download it. |
| Change between pages | Cross-fade | Or fade and slide, or none. |
| Never load these ahead | empty | Addresses like `/shop/cart`, `/downloads/*`. |

Speculation Rules and cross-document View Transitions are supported in Chromium-based browsers; other browsers ignore both tags.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it the defaults apply. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh instant-pages
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
