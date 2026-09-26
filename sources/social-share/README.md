# Social Share

Share buttons for X, LinkedIn, Facebook, WhatsApp, e-mail and "copy link",
under every article on a Stride site.

Privacy first: every button is a plain `<a href>` to the network's own share
address, with an inline SVG icon. The plugin loads nothing from another site,
sets no cookies and tracks nobody. A reader's browser only talks to a network
when the reader clicks its button.

## What it adds to a page

- A small `<aside aria-label="Share this page">` with an optional heading and a
  list of buttons. It goes after the first `</article>` when the page has one,
  and otherwise at the end of the page's outermost wrapper, so it picks up the
  page's own background, colour and width.
- Under 3 KB of inline CSS, all classes prefixed `ss-`.
- A few hundred bytes of first-party inline JavaScript. It powers the
  copy-link button, and it fills in the page address when the server does not
  know it (see "Site address" below). The copy button stays `hidden` when
  JavaScript or the Clipboard API is unavailable. Every other button works
  without JavaScript whenever the address is known on the server.
- Accessibility: every link has a descriptive `aria-label`, such as "Share on X
  (opens in a new tab)". Icons are `aria-hidden`, there is a visible
  `:focus-visible` ring, an `aria-live` status says "Link copied", and motion
  is off under `prefers-reduced-motion`.

It never touches the home page by default, a page without a `<body>`, or a page
that already has the row.

## Settings

Everything lives in **Settings, From plugins, Share buttons** (a
`site-settings` panel). The defaults look good with no configuration at all.

| Setting | Default | What it does |
| --- | --- | --- |
| Show share buttons | on | Master switch. |
| Heading | "Share this article" | The small line above the buttons; empty for none. |
| Style | Brand colours, with labels | Or: outlined with labels, round brand icons, minimal icons in the accent colour. |
| Accent colour | `#DB2777` | Colours the copy-link button, the minimal style and the focus ring. |
| Alignment | Left | Left or centre. |
| X, LinkedIn, Facebook, WhatsApp, E-mail, Copy link | all on | Which buttons to show. |
| Only on pages starting with | empty | Comma-separated slug prefixes, e.g. `blog, news`. Empty means every page. |
| Never on | `home` | Comma-separated slugs. A trailing `*` matches a prefix, e.g. `legal/*`. |
| Site address | empty | For example `https://example.com`. See below. |

### Site address

Stride tells a plugin a page's slug, not its full address, and a share link
needs the full address. The plugin works it out in this order:

1. the **Site address** setting, when filled in;
2. the site's origin, remembered from `on_publish`, which reports
   `https://{primary domain}/{slug}` when the site has a domain;
3. the reader's browser (`location.href`), through the inline script.

With 1 or 2 the links are complete in the HTML itself.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the panel's settings and the site's origin learned from `on_publish`. Nothing else is stored. |

Without `storage` the plugin still works. It shows the default buttons on
every page but the home page and reads the page address in the browser. Saving
the panel then shows a clear error instead of pretending to save.

## Hooks

- `on_page_render` adds the share row.
- `on_publish` remembers the site's origin when the published URL is absolute.

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh social-share
cargo test          # unit tests for encoding, escaping, placement and filters
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
