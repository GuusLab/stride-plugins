# Copy Code

A copy button on every code block, with "Copied" feedback, a small language
label and optional line numbers. No highlighting library, no fonts, no
third-party requests: a few kilobytes of inline CSS and
JavaScript, and only on pages that have a code block.

## Writing a code block in Stride

Stride has no code block of its own, so write one in a text block (the
Inspector's text field keeps line breaks), between two lines of three
backticks, with the language after the first:

````
```bash
npm install
npm run build
```
````

When the page is published, a text block that is exactly one fenced block
becomes a real `<pre><code class="language-bash">`. The block keeps its layout
classes. The code is published exactly as written, already escaped by Stride.
A text block with anything around the fences, or only an opening fence, is
left as it is.

Code blocks that are already `<pre>` elements, for example from a theme or
another plugin, get the button too. The language is read from a
`language-*` or `lang-*` class, or a `data-lang` attribute.

## What visitors get

- A toolbar above each block: the language on the left, a **Copy** button on
  the right. After a click it reads **Copied** with a check mark for two
  seconds.
- The button is a real `<button>` with a visible label and a visible focus
  ring. The label change is announced to screen readers (`aria-live`).
- Copying uses the Clipboard API. On a site served without HTTPS it falls
  back to the older copy command. If both fail, the code is selected and the
  button says "Press Ctrl+C" (or "Press ⌘C" on a Mac), so a click never does
  nothing.
- Line numbers sit in a gutter that is hidden from screen readers and never
  copied.
- Without JavaScript the visitor sees the plain code block, as they would
  without the plugin. The toolbar is hidden when printing.

## Settings

Site settings, panel "Copy code". Changes apply to a page the next time it is
published.

| Field | Default | What it does |
| --- | --- | --- |
| Add a copy button to code blocks | on | Off leaves every page as rendered. |
| Code block style | Clean dark | Clean dark, clean light, or keep the theme's own style and only add the button (floating in the top-right corner). |
| Show the language | on | The small label, such as "bash". |
| Show line numbers | off | Numbers in a gutter on the left. |
| Button text | empty ("Copy") | Up to 24 characters, for a site in another language. |
| Text after copying | empty ("Copied") | Up to 24 characters. |
| Pages to skip | empty | Comma-separated slugs that are left alone. |

## Permissions

- `storage`: to keep the settings above. That is all. If it is refused, the
  plugin still adds copy buttons with the defaults, and the panel says that
  settings cannot be saved.

## Hooks

- `on_page_render`: turns fenced text blocks into `<pre><code>`, then, if the
  page has any `<pre>`, adds a `<style>` to `<head>` and the small script
  before `</body>`. The hook is idempotent and leaves fragments (HTML without
  `<body>`) alone.

## Limits

- No syntax highlighting, on purpose.
- Plugins run in id order. A `<pre>` that a later plugin adds (Custom Code,
  say) gets a button only if the page already had a code block of its own.
- "Keep my theme's style" puts the button over the top-right corner of the
  block, where it can cover the end of a very long first line.
- In the canvas, pressing Enter does not keep a line break; type or paste the
  code into the Inspector's text field.

## Build

```
../../tools/build-plugin.sh copy-code   # or ./build.sh
cargo test                              # unit tests
stride plugin test .
```
