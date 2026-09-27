# Print Friendly

A clean print stylesheet for every page, and an optional "Print this page"
button. Nothing changes on screen unless the button is turned on.

When a visitor prints (or saves as PDF):

- Navigation, the site footer, sidebars, dialogs, cookie and consent banners,
  forms, buttons and embeds (iframes, video, audio) are hidden.
- Text is black on white, backgrounds and shadows are dropped to save ink,
  and fixed or sticky headers stop covering the content.
- Link addresses are written after each link, in full, so the paper copy is
  still useful. Links whose text already is the address, in-page links,
  `mailto:`/`tel:` links and image links are left alone.
- Headings stay with the text that follows them; images, figures, tables,
  code blocks and quotes do not split across pages; table headers repeat on
  every page; long code lines wrap.
- Closed `<details>` sections are opened for printing and closed again after.
- Abbreviations print their full title.

The button is a real `<button>` with a printer icon and a visible label, a
44 px target, a visible focus ring, and it is never printed itself. It starts
`hidden` and only appears once its script runs, so visitors without
JavaScript never see a button that does nothing.

No third-party requests, fonts or images: about 1.8 KB of inline CSS and
0.7 KB of inline JavaScript.

## Settings

Site settings, panel "Print friendly". Changes apply to a page the next time
it is published.

| Field | Default | What it does |
| --- | --- | --- |
| Use the print stylesheet | on | Off prints pages exactly as the browser would. |
| Print link addresses | on | Writes each link's address after its text. |
| Also hide when printing | empty | Extra CSS selectors, comma-separated, e.g. `.newsletter, #comments`. Only selector characters are accepted. |
| Show a Print this page button | off | Adds the button. |
| Button text | empty ("Print this page") | Up to 40 characters, escaped. |
| Button position | After the content | After the content (end of `<main>`, or of the page) or the bottom right corner. |
| Pages to skip | empty | Comma-separated slugs that get neither the stylesheet nor the button. |

## Permissions

- `storage`: to keep the settings above. That is all. If it is refused, pages
  still print cleanly with the defaults, and the panel says that settings
  cannot be saved.

## Hooks

- `on_page_render`: adds a `<style>` to `<head>`, the button (if on), and the
  small script before `</body>`. Idempotent; fragments without `<body>` are
  left alone.

## Build

```
../../tools/build-plugin.sh print-friendly   # or ./build.sh
cargo test                                   # unit tests
```
