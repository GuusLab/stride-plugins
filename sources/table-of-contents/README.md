# Table of Contents

A Stride plugin that gives long pages a linked table of contents.

On every page it renders, it reads the `h2` and `h3` headings inside `<main>`
(or `<body>` when a page has no `<main>`), gives each one an anchor id, and
puts a `<nav>` with a nested, linked list of them at the top of the article:
directly before the first listed heading, inside the same column as the text.
Headings in the site header, the footer or a `<nav>` are left out.

- **No JavaScript.** The links are plain `#fragment` links. Smooth scrolling is
  CSS, and only for visitors who have not asked for reduced motion.
  `scroll-margin-top` keeps a heading clear of the top edge when you jump to it.
- **Stable anchors.** "What a drawing is for" becomes `#what-a-drawing-is-for`.
  A heading that already has an `id` keeps it; a duplicate gets `-2`, `-3`;
  a new id never collides with one already on the page.
- **Accessible markup.** `<nav aria-labelledby>` with a visible title that is
  not itself a heading, so the page outline stays as the author wrote it.
  Visible focus rings, hover underline, inherited fonts and colours.
- **Two styles.** A quiet box at the top of the article (default), or a sticky
  sidebar that sits in the left margin on screens 1300px and wider and falls
  back to the box on smaller ones.
- **Tiny.** About 1.5 KB of inline CSS in `<head>`, nothing loaded from
  anywhere else.
- **Safe to re-run.** A page that already carries a table of contents is left
  alone.

The table is added when a page is rendered for publishing. After changing a
setting, publish the site again to update pages that are already live.

## Settings

**Admin → Settings → From plugins → Table of contents**. Everything has a
default that works with no setup.

| Setting | Default | What it does |
|---|---|---|
| Show a table of contents | on | Off leaves every page exactly as it was rendered. |
| Headings to list | h2 and h3 | Or sections (`h2`) only. |
| Minimum number of headings | 3 | Pages with fewer listed headings get nothing, so short pages stay clean. 1 to 20. |
| Title | "On this page" | Shown above the list. HTML-escaped. |
| Style | Box | Or a sticky sidebar on wide screens. |
| Number the sections | off | 1, 2, 3 for sections and a, b, c for subsections. |
| Accent colour | none (the page's text colour) | The side rule, the numbers and the hover colour. Must be `#rgb` or `#rrggbb`. |
| Pages to skip | none | Slugs, separated by commas, that never get a table of contents: `home, contact`. |

## Permissions

| Permission | Why |
|---|---|
| `storage` | To keep the settings above, as one JSON value under one key. |

That is all it asks for. It reads the headings from the HTML the host already
passes to `on_page_render`, so it does not need `read-pages`. It does not
change stored documents, so it does not need `write-pages`.

**If `storage` is refused**, the plugin still works with the defaults. The
settings panel says so, and it refuses to pretend a save happened.

## Hooks and panels

- `on_page_render`: adds the anchors, the table and its CSS.
- Panel `table-of-contents` (placement `site-settings`): the settings.

## Building

```sh
./build.sh                       # = ../../tools/build-plugin.sh table-of-contents
stride plugin test .
cargo test                       # unit tests for the HTML work
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build remaps every absolute path. The module rebuilds byte for byte.
