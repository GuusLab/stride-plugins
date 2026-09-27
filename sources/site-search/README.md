# Site Search

A search box for the whole site, opened with a button in the header or ⌘K /
Ctrl+K / "/", that finds pages by title and text as the visitor types.

## What it adds to a page

- A button at the end of the page header (or floating in a corner), a
  `<dialog>` with a combobox and a small inline script before `</body>`, and
  the index as inline JSON: slug, title and the start of each page's text.
- Every published page is in the index by its title, from `documents.list`.
  A page adds its text the first time it is viewed, from `<main>` when there
  is one, without header, menu, footer, scripts and forms.
- Text is stored only for pages every visitor sees the same way: no access
  rule other than `public` in the page or in any component it places,
  checked with `documents.get` and `components.get` and remembered per page
  and component versions. Stride renders members-only blocks for members
  through the same hook, so other pages are searchable by title only.
- The index is rebuilt only when an entry, the published pages or the
  settings changed, and is capped at about 56 KB inline.

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show a search box | on | Master switch. |
| Search button | In the page header | Or floating in the bottom corner. |
| Open with ⌘K, Ctrl+K or / | on | |
| Language | The language of the page | Or English, Dutch, German, French. |
| Colour | `#2563EB` | The selected result and the floating button. |
| Text searched per page | 600 characters | 100 to 2000. |
| Leave these pages out | `404` | Comma-separated slugs; a trailing `*` matches every page under it. |

One storage key per page (up to 240 pages), plus a few for settings and the cache.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep each page's entry, the per-page public check and the built index. |
| `read-pages` | To list published pages and to check pages and components for access rules. Without it, only visited pages are searchable, by title. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh site-search
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
