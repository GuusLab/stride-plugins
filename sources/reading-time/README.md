# Reading Time

Adds one line under the first heading of every page, like `4 min read`, and
keeps a per-page word count that an editor can see in the site settings.

```html
<h1>How to brew better filter coffee</h1>
<p class="stride-reading-time stride-reading-time--badge" data-words="612">
  <svg aria-hidden="true" …>…</svg><span>4 min read</span>
</p>
```

With no configuration you get a small pill with a clock icon. It uses the page's
own text colour, so it suits any theme. The CSS is under 600 bytes and goes
inline in `<head>`. The plugin adds no JavaScript and makes no third-party
requests. Choose **Plain text** and you get the bare
`<p class="stride-reading-time" data-words="…">4 min read</p>` with no CSS,
which is the markup the original plugin produced, for your theme to style.

## What is counted

The plugin counts the words inside `<main>`, or inside `<body>` when the page
has no `<main>`. It does not count scripts, styles, `<noscript>`, `<template>`,
SVG, navigation (`<nav>`), footers or asides, and it ignores tag names and
attributes. A word is a run of non-whitespace. The time is rounded up to whole
minutes and is never less than one minute.

## Where the line goes

The line goes after the first `</h1>` inside `<body>`. If there is no `<h1>`,
it goes right after the `<body …>` tag. A page with no `<body>`, such as a
fragment or a feed, comes back exactly as it arrived. So does a page that
already has a reading time, which means rendering a page twice never adds a
second one.

## Settings

The settings are under **Site settings → Reading time**:

| Field | Default | What it does |
|---|---|---|
| Show a reading time | on | Turn it off to leave every page as it was rendered. |
| Words a minute | 200 | Assumed reading speed, 60–600. |
| Label | `min read` | The words after the number, up to 40 characters, escaped. |
| Style | Badge with clock | Or **Plain text**, with no CSS. |
| Accent colour | page text colour | Colour of the badge's clock, tint and outline. The text keeps the page colour, so contrast stays with your theme. |
| Skip these pages | none | Slugs separated by commas, e.g. `home, contact`. |

The panel also tells you how many pages have been counted, their total word
count, and which page is the longest.

Changes apply to a page the next time it is published.

## Permissions

| Permission | Why |
|---|---|
| `storage` | For two things only. The first is the panel's settings, because the host stores nothing on a plugin's behalf. The second is one `count:<slug>` key per page, which the panel uses to report which page is longest without reading any documents. |

The plugin does **not** ask for `read-pages`: `on_page_render` gets the
rendered HTML, and the word count comes from that. It does not ask for
`write-pages` either, because it only changes the served HTML and never the
stored document.

### If storage is refused

- A reading time is **still shown**, with all the default settings.
- No word counts are recorded, so the panel's summary line is empty.
- Saving in the panel shows an error explaining this. It does not pretend the
  settings were saved.

A refused host call is handled inside the plugin and is never returned as an
error from a hook, so the plugin keeps its good standing.

## Build and test

```sh
./build.sh      # ../../tools/build-plugin.sh reading-time: pinned, --locked, path-remapped
cargo test      # counting, rounding, escaping, placement, settings
```

## Limits

- Each page uses one storage key. On a very large site the storage bag
  eventually fills up. After that the word counts are no longer recorded, but
  every page still gets its reading time.
- A slug long enough to push `count:<slug>` past 128 bytes is not recorded.
- Words are counted by spaces, so text in Chinese, Japanese or Korean, which
  has no spaces between words, gives too low an estimate.

MIT.
