# SEO Inspector

A Stride plugin that checks every page each time it is saved and keeps a
score from 0 to 100, with a list of warnings, for every page of the site. The
reports are in **Site settings → SEO Inspector**, worst page first.

It never changes a page and adds nothing to the published site: no markup, no
CSS, no JavaScript, no requests to anyone.

## What it checks

| Check                                                   | Points off |
| ------------------------------------------------------- | ---------- |
| No meta description                                     | 20         |
| Meta description shorter than 50 or longer than 160     | 5          |
| SEO title shorter than 15 or longer than the limit      | 10         |
| No SEO title set (the page name is used instead)        | 5          |
| Images without alt text                                 | 5 each, up to 20 |
| No `h1`                                                 | 15         |
| More than one `h1`                                      | 10         |
| A skipped heading level, such as `h2` then `h4`         | 5 each, up to 15 |
| Thin content: fewer words than the minimum              | 15         |
| A page path longer than the limit                       | 5          |

A page marked `noindex` gets a note but loses no points. Only pages are
checked; templates and partials are skipped. Text inside component instances
is not counted, because the tree the hook receives holds the instance, not the
component's contents.

## Settings

| Setting            | Default | Meaning                                         |
| ------------------ | ------- | ----------------------------------------------- |
| Thin content below | 300     | Minimum word count before a page is "thin".     |
| Longest title      | 60      | Characters before a title counts as too long.   |
| Longest URL        | 75      | Characters in the path (`/about/team`).         |

The defaults are the usual rules of thumb; most sites never change them. New
settings apply the next time each page is saved.

## Permissions

- **`storage`** keeps the per-page reports (one key per page, at most 250
  pages) and the three settings. Without it there is nowhere to keep a report:
  the hook changes nothing and the panel says storage was not granted.
- **`write-pages`** is required by Stride for any plugin that declares
  `on_document_save`. SEO Inspector only reads the document and always answers
  `{"document": null}`, so it never uses this permission. You can refuse it
  and the plugin works exactly the same.

## Building

```sh
./build.sh        # runs ../../tools/build-plugin.sh seo-inspector
stride plugin test .
```

`cargo test` runs the unit tests natively.
