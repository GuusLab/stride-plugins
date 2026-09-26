# Image Lightbox

Click an image to see it large, in a native `<dialog>`, with its alt text as
the caption and arrows (and arrow keys, and swipe) to step through the others.

## What it adds to a page

- A stylesheet in `<head>` and one `<dialog>` plus a small inline script
  before `</body>`, only on pages that contain an `<img>`.
- Images become zoomable once they have loaded. By default only images shown
  at least 15% smaller than their natural size qualify; images inside `a`,
  `button`, `header`, `nav` or `footer` never do.
- Zoomable images get `role="button"`, `tabindex="0"` and an `aria-label`;
  Enter or Space opens them, Escape closes, and focus returns to the image.
- The open animation is left out under `prefers-reduced-motion`.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Open images in a lightbox | on | Master switch. |
| Which images | Only images shown smaller than they are | Or every image in the page content. |
| Show the image description as a caption | on | Uses the alt text. |
| Show arrows | on | Arrow keys and swipe work either way. |
| Background | Dark | Or light. |
| Skip these pages | empty | Comma-separated slugs; a trailing `*` matches every page under it. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it the defaults apply, so the lightbox still works. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh image-lightbox
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
