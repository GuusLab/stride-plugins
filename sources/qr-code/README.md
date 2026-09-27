# QR Code

A QR code of each page's address, drawn on the server as SVG: behind a small
button, where an editor types `[qr-code]`, and in the corner of printed pages.

## What it adds to a page

- `[qr-code]` becomes a figure with the code and the address (a paragraph
  holding only the code is replaced whole).
- A floating "QR code" button opens a `<dialog>` with the code, the address
  and a download link (the SVG as a data URL).
- Under `@media print`, a 28 mm code with the address in the bottom-right
  corner of the page.
- Encoded with the `qrcodegen` crate, error correction level M. The address
  comes from the setting, else from `on_publish` (once the site has a
  domain), else from the canonical link; without one nothing is added.

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Make QR codes | on | Master switch. |
| Show a QR code button on every page | on | |
| Print the code in the corner of every page | on | |
| Site address | empty | For example `https://example.com`. |
| Add ?ref=qr to the address | off | To count scans in analytics. |
| Colour of the code | `#111827` | Keep it dark. |
| Language | The language of the page | Or English, Dutch, German, French. |
| No button or print code on these pages | empty | `[qr-code]` still works there. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings and the site address learned from `on_publish`. Without it a code needs the canonical link to know the address. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh qr-code
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
