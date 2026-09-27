# Celebrate

A burst of confetti on thank-you pages and on any link to `#celebrate`.

## What it adds to a page

- One inline script before `</body>`, only on the listed pages or pages
  with a link ending in `#celebrate`.
- A full-screen `<canvas>` with `pointer-events: none`, drawn for about three
  seconds and removed. Nothing runs under `prefers-reduced-motion: reduce`.
- `window.strideCelebrate(x, y)` is there for anyone who wants a burst from
  their own code.

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Celebrate | on | Master switch. |
| Pages that open with confetti | `thanks, thank-you, bedankt` | Comma-separated slugs. |
| Shapes | Confetti | Or hearts, or stars. |
| Colours | Party | Or gold, pastel, or your own colour. |
| My own colour | `#E11D48` | |
| How much | A cheerful burst | Or a proper party. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it the defaults apply. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh celebrate
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
