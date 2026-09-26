# Reading Progress

A thin bar at the top of long pages that fills as the reader scrolls, plus an
optional round back-to-top button that fades in once the reader is half a
screen down.

- The bar is a CSS scroll-driven animation (`animation-timeline: scroll(root)`).
  Browsers without it get the same result from about 700 bytes of inline
  JavaScript, which also hides the bar on short pages (less than half a screen
  of scrolling) and toggles the button.
- Zero configuration: a 3 px rose (`#f43f5e`) bar and a button bottom right.
- No third-party requests, no fonts, no images. About 1.3 KB of inline CSS.
- The bar is decorative (`aria-hidden`). The button is a real link to `#top`
  with an accessible name ("Back to top"), a visible focus ring and a 44 px
  hit area. It scrolls smoothly unless the visitor prefers reduced motion. The
  arrow is white on the chosen colour, or near-black when white would fall
  under 3:1. Both are hidden when printing.
- The hook is idempotent and leaves fragments (HTML without `<body>`) alone.

## Settings

Site settings, panel "Reading progress". Changes apply to a page the next time
it is published.

| Field | Default | What it does |
| --- | --- | --- |
| Show a reading progress bar | on | Off leaves every page as rendered. |
| Colour | empty (rose `#f43f5e`) | Colour of the bar and the button. `#rgb` or `#rrggbb`. |
| Thickness | Thin (3 px) | Thin 3 px, medium 5 px or bold 8 px. |
| Show a back-to-top button | on | The round button in the corner. |
| Button position | Bottom right | Bottom right or bottom left. |
| Pages to skip | empty | Comma-separated slugs that get neither bar nor button. |

## Permissions

- `storage`: to keep the settings above. That is all. If it is refused, the
  plugin still adds the bar and button with the defaults, and the panel says
  that settings cannot be saved.

## Hooks

- `on_page_render`: adds a `<style>` to `<head>`, the bar (and button) right
  after `<body>`, and the small script before `</body>`.

## Build

```
../../tools/build-plugin.sh reading-progress   # or ./build.sh
cargo test                                     # unit tests
```
