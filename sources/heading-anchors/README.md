# Heading Anchors

A small `#` link beside every section heading. It stays out of the way until
a reader points at the heading or tabs to it, then one click scrolls smoothly
to that section, puts it in the address bar and copies the link, with a short
"Link copied" message. Perfect for docs, guides and long articles people want
to quote.

Headings that have no id get a stable one made from their text:
"Choosing your paper" becomes `#choosing-your-paper`. Ids are made exactly the
way [Table of Contents](../table-of-contents) makes them, so links from both
plugins agree. Headings that already have an id keep it.

## What it changes

- Section headings (H2 and H3 by default) inside `<main>`, or the whole body
  when there is no main. Navigation, scripts, styles and templates are skipped.
- Headings that already contain a link are left alone, since a link inside a
  link is invalid HTML.
- The page's other HTML is untouched. The plugin runs once per page; running it
  again changes nothing.

## Settings

Site settings, **Heading anchors**:

| Setting | Default | What it does |
|---|---|---|
| Show heading links | on | Off leaves every page exactly as it was rendered. |
| Headings | H2 and H3 | Sections only, sections and subsections, or down to H4. |
| Symbol | Hash (#) | Or a chain link icon, or a section sign (§). |
| Position | After the heading | Or in the left margin, the way many docs sites do it. On narrow screens it sits just before the text. |
| Colour | blue (#2563eb) | The link and the dot in the copied message. |
| Copy the link when clicked | on | Off still scrolls and updates the address bar, without copying. |
| Copied message | "Link copied" | Your own words, for example in your site's language. |
| Pages to skip | none | Slugs, separated by commas. |

Changes apply to a page the next time it is published.

## Accessibility

- Each link is a real `<a href="#id">` with an accessible name, "Link to
  section: <heading>", so it works without JavaScript and with screen readers.
- It appears on keyboard focus, with a visible focus ring, not only on hover.
  On touch screens, which have no hover, it is always shown, faintly.
- The copied message is announced through a polite live region.
- Smooth scrolling only for visitors who have not asked for reduced motion.
- Headings get `scroll-margin-top`, so a sticky header does not cover them.
- Hidden when printing.

## Permissions

| Permission | Why |
|---|---|
| `storage` | To keep its settings. Refuse it and the plugin still works with the defaults. |

No network access, no third-party requests, no fonts or images: about 2 KB of
inline CSS and JavaScript per page. The clipboard is only written when a
visitor clicks a link, and only on HTTPS, which browsers require for it.

## Build

```sh
./build.sh                  # tools/build-plugin.sh heading-anchors: reproducible release build
cargo test                  # unit tests
stride plugin test .        # the registry's checks
```
