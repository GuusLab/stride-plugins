# Related Pages

A "Related reading" block at the end of articles and posts: the published
pages of the same site that have the most in common with the page being
read, as cards or as a simple list.

- **How pages are matched.** Pages that share a topic (from the Topics
  setting) count most, then pages that share words in their titles ("Notes
  on timber joints" meets "Timber joints that last"), then pages in the same
  folder. Common words ("the", "how", "guide", "de", "het"…) are ignored and a
  plural `s` is dropped. Ties go to the newest page. Pages with nothing in
  common are never shown, so a block can have fewer pages than asked for, or
  none at all.
- **Which pages get the block.** By default, articles and posts: a page with
  an `<article>` element, a page in a folder (`blog/…`), or a page with at
  least about 175 words of paragraphs. About, contact and landing pages are
  usually shorter and are left alone. Switch to "Every page except the home
  page" to put it everywhere.
- **Where it goes.** Just before the end of the `<article>`, else the end of
  `<main>`, else just above the site `<footer>`, else the end of the page.
- **Zero configuration:** up to three cards headed "Related reading", in a violet
  accent, matched on title words and folders.
- **Light and accessible.** About 2 KB of inline CSS, no JavaScript, no
  fonts, no images, no third-party requests. The block is an `<aside>` named
  by its `<h2>`, holding a real list of real links. Every title and label is
  HTML-escaped. It has a visible focus ring, follows the page's own font and
  text colour, keeps its hover lift off for
  visitors who prefer reduced motion, and is hidden when printing.
- The hook is idempotent and leaves fragments (HTML without `<body>`) alone.

## Settings

Site settings, panel "Related pages".

| Field | Default | What it does |
| --- | --- | --- |
| Show related pages | on | Off leaves every page as rendered. |
| Heading | empty ("Related reading") | The block's heading, up to 60 characters. |
| Style | Cards | Cards in a grid, or a list with a divider between rows. |
| How many pages | 3 | 1 to 6. The most it shows. |
| Show on | Articles and posts | Or every page except the home page. |
| Only in these folders | empty | Comma-separated folders (`blog, writing`). Only pages in them get the block or are suggested. |
| Topics | empty | One line per page: `slug: topic, topic`. Shared topics rank first, and the first shared one is shown above the title. |
| Accent colour | empty (violet `#6d28d9`) | Topic labels, arrows, the hover border and the focus ring. |
| Pages to skip | empty | Comma-separated slugs that never get the block and are never suggested. |

## Publish again to refresh

The block is part of the published HTML, built when a page is published.
A page published before the pages related to it existed shows fewer or no
suggestions until it is published again. After adding articles or changing
the settings, publish the whole site to bring every block up to date.

## Permissions

- `read-pages`: to list the site's published pages (slug, title and date),
  which is what it suggests from. It reads nothing else and writes nothing. If
  it is refused, the plugin has nothing to suggest and leaves every page
  exactly as rendered; the settings panel says so.
- `storage`: to keep the settings above. If it is refused, the plugin runs
  with the defaults and the panel says that settings cannot be saved.

## Limits

- **Post tags are not used.** Stride keeps a blog post's tags where no plugin
  permission can read them (`posts.list` is not part of `read-pages`), and
  they are not in the rendered page. The Topics setting is the way to group
  pages by subject.
- Only page titles are compared, not their text: a render has 50 ms, and
  reading every page's content would not fit.
- At most 500 published pages are considered.

## Hooks

- `on_page_render`: adds a `<style>` to `<head>` and the block to the page.
  One `documents.list` call per page render that qualifies.

## Build

```
../../tools/build-plugin.sh related-pages   # or ./build.sh
cargo test                                  # unit tests
stride plugin test .
```
