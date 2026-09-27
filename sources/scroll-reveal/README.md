# Scroll Reveal

Page content fades and slides into place as visitors scroll to it: headings,
text, images and cards one by one, or whole sections at once.

- Modern browsers run it as a native CSS scroll-driven animation
  (`animation-timeline: view()`), so no code runs on scroll. The effect
  follows the scroll: content is fully in place by the time it is fully on
  screen.
- Other browsers get the same look from about 800 bytes of inline JavaScript
  built on `IntersectionObserver`. It hides only content that starts below the
  fold, and only once it is watching it; when anything comes into view,
  everything above it is revealed too, so jumping to the end or to an anchor
  never leaves skipped content invisible.
- Zero configuration: a gentle 40 px fade up for headings, paragraphs, lists,
  images, quotes and cards inside `<main>`. Content already on screen when the
  page opens never moves. The header, navigation and footer are left alone.
- Accessible by default: everything sits behind
  `prefers-reduced-motion: no-preference`, so visitors who ask for less motion
  see a normal page. Nothing is hidden without JavaScript, when printing, or
  from screen readers and search engines. No markup is added, only one
  `<style>` and one `<script>`.
- No third-party requests, fonts or images. About 1.5 KB of CSS and JavaScript.
- The hook is idempotent and leaves fragments (HTML without `<body>`) alone.

## Settings

Site settings, panel "Scroll reveal". Changes apply to a page the next time
it is published.

| Field | Default | What it does |
| --- | --- | --- |
| Reveal content on scroll | on | Off leaves every page as rendered. |
| Effect | Fade up | Fade up, fade in, slide in from the left, slide in from the right, or zoom in. |
| Distance | Medium (40 px) | How far content travels: 16, 40 or 80 px. For zoom in, how much it grows (97, 92 or 85 % to full size). Fade in does not move. |
| What to reveal | Headings, text, images and cards, one by one | Or whole sections at once: each section's content moves as one piece while its background band stays put. |
| Pages to skip | empty | Comma-separated slugs that never get the effect. |

The slide effects set `overflow-x: clip` on `<main>`, so content waiting off to
the side never widens the page on a phone.

## Permissions

- `storage`: to keep the settings above. That is all. If it is refused, the
  plugin still reveals content with the defaults, and the panel says that
  settings cannot be saved.

## Hooks

- `on_page_render`: adds a `<style>` to `<head>` and the small fallback script
  before `</body>`.

## Limits

- An element taller than the screen, such as a whole long section, finishes
  its effect only when it fills the screen. Use "Headings, text, images and
  cards" on long pages.
- If an element already has its own scroll animation from the editor, the
  plugin's effect replaces it on that element. Skip the page, or use the
  editor's motion instead, when you want the editor's animation.

## Build

```
../../tools/build-plugin.sh scroll-reveal   # or ./build.sh
cargo test                                  # unit tests
stride plugin test .
```
