# Cookie Consent

A GDPR/AVG cookie banner for every published page of a Stride site. It is
accessible, speaks Dutch or English depending on the page, makes no requests
to anyone, and fits in about 4 KB of inline HTML, CSS and JavaScript.

## What it does

- Adds a banner with **Accept all**, **Reject all** (equally prominent, as the
  GDPR asks) and **Settings**, which opens per-category choices:
  *Necessary* (always on), *Analytics* and *Marketing*.
- Picks the text from `<html lang>`: pages whose language starts with `nl` get
  Dutch, every other page English.
- Remembers the choice for 180 days in a first-party cookie,
  `stride_consent` (`n`, plus `a` for analytics and/or `m` for marketing). A
  returning visitor sees no banner and no flash of one: it is `hidden` until
  the script decides.
- Shows a small **Cookie settings** button after a choice, so a visitor can
  change their mind (optional). Any element with `data-cookie-settings` opens
  the banner too, for example a link in your footer.
- Accessible: a labelled, non-modal dialog region, real buttons,
  `aria-expanded` on Settings, visible focus rings, readable button text
  (dark or light, chosen from the button colour's contrast).

## For other scripts

```html
<html data-cookie-consent="necessary analytics">   <!-- or "pending" -->
```

```js
document.addEventListener('stride:consent', (e) => {
  if (e.detail.analytics) loadAnalytics();   // {necessary, analytics, marketing}
});
StrideConsent.get();   // the same object, or null before a choice
StrideConsent.open();  // show the banner again
```

The event fires on page load when a choice is already stored and again every
time the visitor saves one. Scripts that run later can read the attribute.

## Settings

*Site settings → Cookie consent*:

| Setting | Default |
| --- | --- |
| Show the cookie banner | on |
| Layout | floating card (or full-width bar) |
| Text (English), Text (Dutch) | a neutral default text |
| Privacy policy link | none; a `/path` or `http(s)://` address |
| Background, text and button colour | `#0F172A`, `#F8FAFC`, `#0EA5E9` |
| Show a small "Cookies" button after a choice | on |

Text is escaped; colours must be hex; links may not be `javascript:` or
`data:` URLs.

## Permissions

- **`storage`** — to keep the panel's settings (one key per site). If it is
  refused, the banner still appears with its default text and colours, and
  the panel says why it cannot save.

No other permission: the plugin reads nothing from the site and changes no
content, it only adds the banner to the HTML being served.

## Building

```sh
tools/build-plugin.sh cookie-consent   # from the registry root, or ./build.sh here
cargo test                              # unit tests on the host
```
