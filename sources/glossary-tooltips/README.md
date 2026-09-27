# Glossary Tooltips

Define your terms once, in **Site settings > Glossary tooltips**. The first
time each term appears on a page, it is marked with a dotted underline and
shows its definition in a small tooltip on hover, tap or keyboard focus.
Later occurrences on the same page are left plain, so a page never turns
into a sea of underlines.

Headings, links, code, buttons, forms, navigation, headers and footers are
never touched. To keep any other part of a page free of tooltips, put
`data-glossary="off"` on the element around it.

## Settings

| Field | What it does |
| --- | --- |
| Show glossary tooltips | Turn this off to leave every page exactly as it was rendered. |
| Terms | One per line: `term: definition`. Add other spellings with a vertical bar, like `API \| APIs: A way for programs to talk to each other`. Lines starting with `#` are ignored. Up to 8,192 characters in total, 80 per term and 400 per definition. A line without a colon is refused with its line number, and nothing is saved. |
| Match case | Off by default, so `api` in your list also matches `API` on a page. The page keeps its own spelling. |
| Look | Dotted underline (default) or a soft highlight. |
| Colour | The underline or highlight. Empty means amber (`#ca8a04`). Only `#rgb` and `#rrggbb` are accepted. |
| Pages to skip | Slugs, separated by commas, that never get tooltips. |

With zero configuration the plugin does nothing: there are no terms yet.
Add a few and the defaults look right on light and dark themes, because the
tooltip carries its own dark surface and the term keeps the page's own font
and colour.

Matching is whole-word, so `site` does not match inside `website`. When two
terms overlap, the longest wins (`static site generator` before `site`).

## Output

Each marked term becomes:

```html
<dfn class="stride-gt">
  <button type="button" class="stride-gt-t" aria-describedby="stride-gt-1">API</button>
  <span class="stride-gt-tip" role="tooltip" id="stride-gt-1">A way for programs to talk</span>
</dfn>
```

- `<dfn>` marks the defining instance of the term.
- The `<button>` makes every term reachable with Tab; its accessible name is
  the term and its description is the definition, so screen readers announce
  both.
- The tooltip shows on hover and focus with CSS alone. About 1 KB of inline
  script adds tap to open and close on touch screens, Escape to dismiss
  (WCAG 1.4.13), and keeps the tooltip inside the viewport, flipping it below
  the term near the top of the screen.
- The tooltip stays open while the pointer moves onto it, has no animation
  for visitors who prefer reduced motion, and is hidden when printing.
- Definitions are HTML-escaped. The colour is validated before it reaches CSS.
- One small `<style>` in `<head>` and one `<script>` before `</body>`, added
  only when a page actually has a term. No fonts, images or third-party
  requests.

## Permissions

- `storage`: to keep the terms and settings for this site. That is all it needs.

If `storage` is refused there is nowhere to keep terms, so pages are served
unchanged and the settings panel says why.

## Build

```
./build.sh          # or: tools/build-plugin.sh glossary-tooltips from the repo root
```
