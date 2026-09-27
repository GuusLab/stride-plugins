# Quote Share

Select a sentence and a small bar appears to share it as a quote on X,
Bluesky, WhatsApp, by email, or as a link that opens the page with that
sentence highlighted.

## What it adds to a page

- A toolbar and a small script before `</body>`, a stylesheet in `<head>`,
  on pages with enough words.
- The bar shows for selections of 12 to 600 characters inside `<main>`,
  never in the menu, header, footer, links or form fields.
- Share links are plain intent URLs opened on click; the copied link carries
  a text fragment (`#:~:text=start,end`).

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show the share bar on selected text | on | Master switch. |
| X, Bluesky, WhatsApp, Email, Copy quote with link | all on | Which buttons to show. |
| Bar colour | `#111827` | |
| Language | The language of the page | For the labels screen readers hear. |
| Only on pages with at least this many words | 150 | |
| Skip these pages | `home` | Comma-separated slugs; a trailing `*` matches every page under it. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it the defaults apply. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh quote-share
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
