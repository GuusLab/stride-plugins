# Announcement Bar

Shows one announcement bar at the very top of every published page: a sale, an
opening-hours change, a new product. It is edited from **Site settings >
Announcement bar** and goes away as soon as the message is empty.

Ported from Stride's worked example (`plugins/announcement`) and extended. The
original `message` and `tone` settings keep working.

## Settings

| Field | What it does |
| --- | --- |
| Message | The text of the bar, up to 160 characters. Empty means no bar. |
| Link text | Optional, e.g. "Read more", shown after the message with an arrow. |
| Link address | `https://`, `http://`, `mailto:`, a path like `/shop`, or `#anchor`. Without link text the whole message is the link. Other schemes (such as `javascript:`) are refused. |
| Style | Quiet (dark), Loud (blue), Warm (orange) or My own colour. |
| Bar colour | Used by "My own colour". Text switches between white and near-black by WCAG contrast. |
| Visitors can close it | Adds a close button. The choice is remembered in the visitor's browser (`localStorage`) per message, so a new message shows again. |

With zero configuration nothing is shown; type a message and the Quiet style
looks good on any theme.

## Output

A `<div role="region" aria-label="Announcement">` right after `<body>`, with a
small inline `<style>` and, only when closing is enabled, a few hundred bytes of
inline script. All text is HTML-escaped, the bar is inserted once per page, the
close button is a real `<button>` with an accessible name and visible focus
ring. No fonts, images or third-party requests.

## Permissions

- `storage`: to keep the settings for this site. That is all it needs.

If `storage` is refused, pages are served unchanged (a warning is logged) and
the settings panel explains that there is nowhere to save.

## Build

```
./build.sh          # or: tools/build-plugin.sh announcement from the repo root
```
