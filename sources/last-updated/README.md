# Last Updated

Shows a short line like **Last updated on 27 September 2026** on your pages
and posts, so readers can see at a glance how fresh the content is. On pages
whose language is Dutch (`<html lang="nl">`) it reads **Laatst bijgewerkt op
27 september 2026**; every other page gets English.

```html
<p class="stride-last-updated"><svg aria-hidden="true" …></svg>
  <span>Last updated on <time datetime="2026-09-27">27 September 2026</time></span></p>
```

## How the date is recorded

A plugin has no clock of its own, so the date comes from Stride. Every time a
page is published, the plugin reads the site's list of pages and stores, per
page, the day and the version number it was published with.

- The date moves only when the page's content has changed since the last
  time. **Publish site** re-publishes every page, but a page without new edits
  keeps its old date.
- Right after installing, publish the site once: every page is recorded with
  the date Stride has for it.
- The date is the day the edits went live, in UTC. Pages you save but do not
  publish keep the date visitors already see, which is the honest answer to
  "when did this page last change?".
- All dates live in one stored value, about 1,500 pages' worth. Pages that
  are deleted or renamed drop out on the next publish.

Visiting a page costs one storage read and a small string insert. No
JavaScript, no web fonts, no third-party requests: about 450 bytes of inline
CSS and a decorative SVG icon hidden from screen readers.

## Settings

Site settings, **Last updated**:

| Setting       | Default           | What it does                                                           |
| ------------- | ----------------- | ---------------------------------------------------------------------- |
| Show the date | on                | Off leaves every page exactly as it was rendered.                      |
| Position      | Under the title   | Right after the first `<h1>`, or at the end of the content (`</main>`). |
| Date format   | 27 September 2026 | Also 27 Sep 2026, or numeric: 27-09-2026 (Dutch), 2026-09-27 (English). |
| Label         | empty             | Replaces "Last updated on" / "Laatst bijgewerkt op". Escaped.          |
| Pages to skip | home              | Addresses that never show a date, separated by commas.                 |

The panel also tells you how many pages have a date and which changed last,
or why none could be recorded.

## Permissions

| Permission   | Why                                                                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `storage`    | Keeps the settings and the recorded dates.                                                                                                             |
| `read-pages` | Reads the list of pages when one is published, for its last-changed time and version. It only reads the listing (`documents.list`), never page content. |

If `read-pages` is refused, no date can be learned and pages are left
unchanged; the settings panel says so. If `storage` is refused there is
nowhere to keep a date, pages are left unchanged and saving settings explains
why. Nothing ever fails a publish.

## Build and test

```sh
../../tools/build-plugin.sh last-updated   # or ./build.sh
stride plugin test .
cargo test                                  # unit tests on the host
```
