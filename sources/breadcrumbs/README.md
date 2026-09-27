# Breadcrumbs

A breadcrumb trail above the page content, like Home › Journal › Our spring
menu, plus matching BreadcrumbList JSON-LD so search engines can show the
trail in results. The home page never gets one.

- The trail follows the slug: `journal/spring-menu` gives Home, the page
  `journal`, then the current page. Parent steps use your page titles and
  link only to pages that are published; a step that is not a page is plain
  text rather than a dead link.
- The current page is named by its title, else the first `<h1>`, else the
  `<title>` (without a " | Site" suffix), else its slug.
- Zero configuration: dark teal (`#0f766e`, 5.5:1 on white) links, a `›`
  separator, 14 px text, above the content inside `<main>` (after
  `</header>`, or at the top of `<body>`, when there is no `<main>`).
- No script, no fonts, no images, no third-party requests. About 1 KB of
  inline CSS.
- Accessible: a `<nav aria-label="Breadcrumb">` landmark with an ordered
  list, `aria-current="page"` on the last step, separators drawn in CSS with
  empty alt text so screen readers skip them, a visible focus ring. Hidden
  when printing.
- JSON-LD `item` URLs are absolute when the site has a primary domain, else
  taken from the page's canonical link, else root-relative. The hook leaves
  structured data out when the page already has a BreadcrumbList.
- Every interpolated name is HTML-escaped; `<` in the JSON-LD is written as
  `<`. The hook is idempotent and leaves fragments (HTML without
  `<body>`) alone.

## Settings

Site settings, panel "Breadcrumbs". Changes apply to a page the next time it
is published.

| Field | Default | What it does |
| --- | --- | --- |
| Show breadcrumbs | on | Off leaves every page as rendered. |
| Name of the home page | empty (Home) | The first step. At most 40 characters. |
| Separator | Chevron › | Chevron ›, slash /, arrow → or dot ·. |
| Style | Plain text links | Plain links, or rounded pills tinted with the link colour. |
| Link colour | empty (`#0f766e`) | `#rgb` or `#rrggbb`. |
| Show the current page at the end | on | Off ends the trail at the parent. |
| Add structured data for search engines | on | The BreadcrumbList JSON-LD. |
| Pages to skip | empty | Comma-separated slugs that get no trail. |

## Permissions

- `storage`: keeps the settings above. Refused, the plugin runs with the
  defaults and the panel says settings cannot be saved.
- `read-pages`: `documents.list` for the titles of the parent pages and
  whether they are published, and `sites.get` for the site's domain (absolute
  URLs in the JSON-LD). Refused, parent steps are named after their slug
  ("summer-sale" reads "Summer sale") and shown as plain text, and the
  JSON-LD uses the canonical link or root-relative URLs.

## Hooks

- `on_page_render`: adds a `<style>` and the JSON-LD `<script>` to `<head>`
  and the `<nav>` above the content.

## Limits

- The hook is given the slug, not whether the page is the site's chosen home
  page. The slug `home` is always skipped; a home page with another slug has
  to be added to "Pages to skip".
- Stride serves published pages at `/{slug}` with one path segment, so a
  nested slug such as `journal/spring-menu` is not reachable at
  `/journal/spring-menu` today. The trail and its links follow the slug and
  will be right once nested addresses are served.

## Build

```
../../tools/build-plugin.sh breadcrumbs   # or ./build.sh
cargo test                                # unit tests
```
