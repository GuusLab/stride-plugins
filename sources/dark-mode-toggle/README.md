# Dark Mode Toggle

A dark version of every page and a light/dark switch for visitors.

- **Zero configuration.** The dark colours are derived at publish time from
  the page's own design colours (the `--c-*` custom properties Stride renders
  into `<head>`). Each colour's lightness is flipped in OKLab across the site's
  own range: the lightest colour becomes a near-black background, the darkest
  becomes the text colour, and hues are kept but softened. Contrast between
  text and its background stays about the same.
- **Follows the device** (`prefers-color-scheme`) with plain CSS until the
  visitor chooses. The choice is kept in `localStorage` and restored by a
  one-line script in `<head>` that runs before anything paints, so there is
  no flash of the wrong colours on load.
- **Accessible switch.** A real `<button>` with `aria-pressed` and an
  accessible name ("Dark mode", configurable), a 44 px target (40 px in the
  header), a visible focus ring and a moon or sun icon. It is `hidden` until
  its script runs, so visitors without JavaScript never see a dead button;
  they still get the device setting. Hidden when printing.
- `color-scheme` switches too, so form controls and scrollbars follow.
- No third-party requests, fonts or images. About 2.5 KB of inline CSS and JS.
- The hook is idempotent and leaves fragments (HTML without `<body>`) alone.

Limits: only colours that come from design tokens change. A colour typed
straight into a component, and photographs, stay as they are. A page with no
design colours gets the browser's own dark canvas (`Canvas`/`CanvasText`).

## Settings

Plugin page, panel "Dark mode". Changes apply to a page the next time it is
published.

| Field | Default | What it does |
| --- | --- | --- |
| Offer a dark version of the site | on | Off leaves every page as rendered. |
| Before a visitor chooses | Follow the visitor's device | Or always start light, or always start dark. |
| Switch position | Floating, bottom right | Floating bottom left, or at the end of the header's menu (pages without a `<header>` get the floating switch). |
| Dark background | empty (derived) | The darkest colour of the dark version. Neutral colours are tinted towards it. |
| Dark text | empty (derived) | The lightest colour of the dark version. |
| Colour overrides | empty | One `token = #hex` per line, e.g. `surface.deep = #0f2a22`. `--c-surface-deep: #0f2a22` works too. |
| Switch label | empty ("Dark mode") | What screen readers announce, for sites in other languages. |

## Permissions

- `storage`: to keep the settings above. That is all. If it is refused, the
  plugin still adds dark mode with the defaults, and the panel says that
  settings cannot be saved.

## Hooks

- `on_page_render`: adds the dark palette `<style>` and the tiny restore
  script before `</head>`, the switch after `<body>` (or in the header's menu),
  and the switch's script before `</body>`.

## Build

```
../../tools/build-plugin.sh dark-mode-toggle   # or ./build.sh
cargo test                                     # unit tests
```
