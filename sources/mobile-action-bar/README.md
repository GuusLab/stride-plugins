# Mobile Action Bar

A bar fixed to the bottom of the screen on phones with Call, Email, Directions
and a button of your own. Plain links, no JavaScript.

## What it adds to a page

- A `<nav aria-label="Quick actions">` before `</body>` and a stylesheet in
  `<head>`. On the chosen screens the page gets bottom padding so the bar never
  covers content; safe-area insets are respected.
- Call is a `tel:` link built from the digits of the number, Email a `mailto:`
  link, Directions a Google Maps search for the address.
- Only the buttons whose details are filled in are shown; with none, nothing
  is added.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show the action bar | on | Master switch. |
| Phone number | empty | Digits, spaces, dashes, a leading `+`. |
| Email address | empty | |
| Address | empty | Opens in Google Maps. |
| Your own button, Link | empty | Both or neither. |
| Language of the buttons | English | Or Dutch, German, French. |
| Colour | `#0F766E` | Icons and your own button. |
| Show it on | Phones and small tablets | Or every screen, floating like a dock. |
| Skip these pages | empty | Comma-separated slugs; a trailing `*` matches every page under it. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep your details. Without it nothing is shown. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh mobile-action-bar
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
