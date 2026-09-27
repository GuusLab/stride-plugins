# QR Share

A QR code of the page a visitor is on, so they can open it on their phone in
one scan. By default it sits behind a small round button in the bottom corner;
a click opens a card with the code, the address and a **Download QR code**
button. Print the page and the code is at the bottom of the paper, which makes
any page a poster, a flyer, a menu or a price card that leads back to the site.

The code is made by the plugin itself, on your server, as inline SVG. There is
no QR image service, no library from a CDN and no request to anyone else: the
visitor's browser gets the code with the page.

## Which address it encodes

In this order:

1. The page's own canonical link, when it is a full `https://` address (set it
   in the page's SEO settings).
2. The **Site address** setting plus the page's slug.
3. The site's address learned when a page is published on a domain, plus the
   slug. On a site with a domain this needs no setup at all.

With none of these the page is left exactly as it is: a QR code of `/menu`
would not open anything on a phone. Saving the settings says so when it
applies.

## What it changes

- Adds one `<style>` to the head and, before `</body>`, the button and its
  card, or the card at the bottom of the page.
- The code uses error correction level M (it still scans with about 15% of it
  smudged or covered) and the smallest QR version that holds the address. It is
  always black on white with a quiet zone, whatever the theme.
- Addresses longer than 331 characters are left alone: their codes would be
  too dense to scan at the size shown. Real page addresses are far shorter.
- Pages without a `<body>` are left alone, and running it twice changes nothing.

## Settings

Site settings, **QR code**:

| Setting | Default | What it does |
|---|---|---|
| Show QR codes | on | Off leaves every page exactly as it was rendered. |
| Where | Button in the corner | Or a card at the bottom of every page. |
| Button side | Right | Which bottom corner the button sits in. |
| Heading | "Scan to open this page" | The line next to the code. Empty for none. |
| Button colour | green (#16A34A) | The round button and the download button. Never the code itself. |
| Print the code | on | With the button, also print the code at the bottom of printed pages. |
| Download button | on | Lets visitors save the code as an SVG file, sharp at any size. |
| Never on | none | Slugs separated by commas; a trailing `*` matches a prefix. |
| Site address | empty | e.g. `https://example.com`. Only needed before the site is published on a domain. |

## Accessibility

- The button is a native `<details>`/`<summary>`, so it opens and closes with
  the keyboard and works without JavaScript. It has an accessible name, "Show a
  QR code for this page", and a visible focus ring.
- Escape closes the card and returns focus to the button; so does a click
  outside it.
- The code is an `<svg role="img">` labelled with the address it opens, and the
  address is also printed as text under it.
- The print-only copy is `display: none` on screen, so screen readers do not
  hear the code twice.
- No animation for visitors who prefer reduced motion.

## Permissions

| Permission | Why |
|---|---|
| `storage` | To keep its settings and the site address learned on publish. Refuse it and the plugin still shows a code, with the defaults, on pages with a full canonical link, and leaves other pages alone. |

No network access and no third-party requests. About 3 KB of inline CSS and
JavaScript plus the code's SVG path (1 to 3 KB for a typical address) per page.
The download is made in the browser from the SVG already on the page.

## Build

```sh
./build.sh                  # tools/build-plugin.sh qr-share: reproducible release build
cargo test                  # unit tests
stride plugin test .        # the registry's checks
```

The encoder is checked module for module against the `qrcode` npm package:
generate fixtures with it and run `QR_FIXTURES=fixtures.json cargo test`.
