# Promo Popup

One popup for a sale, newsletter or event, shown after a delay, on scroll or
on exit intent, and not again for as long as you choose.

## What it adds to a page

- A native `<dialog>` (modal, or non-modal in a corner) and a small script
  before `</body>`, a stylesheet in `<head>`.
- Whether a visitor has seen it is kept in their `localStorage` (or
  `sessionStorage` for once per visit) under a key derived from the title,
  text and link, so a new campaign is shown again.
- Exit intent is the pointer leaving the top of the window; on touch screens,
  where there is no pointer, it falls back to scrolling 60% of the page.
- Never opens over another open dialog.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show the popup | on | Master switch. |
| Title | empty | Nothing is shown while it is empty. |
| Text | empty | Blank lines start paragraphs. |
| Button text, Button link | empty | Both or neither. |
| Image | empty | `https://` or a path on this site. |
| When to show it | After a few seconds | Or after scrolling, or on exit intent. |
| Seconds to wait | 8 | 0-300. |
| Scrolled this far | 50% | 10-100. |
| How often | Once, then again after 7 days | Or once per visit, or only once. |
| Look | Middle of the screen | Or small, in the bottom corner. |
| Button colour | `#4F46E5` | |
| Close text | No thanks | |
| Pages, Page list | Every page | Or only / except the listed slugs. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the popup. Without it nothing is shown. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh promo-popup
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
