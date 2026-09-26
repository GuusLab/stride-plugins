# WhatsApp Chat

A floating "chat with us on WhatsApp" button for every published page of a
Stride site. It opens WhatsApp (app or web) with a chat to your number and a
message already typed. Nothing is loaded from WhatsApp or anyone else: the
button is a plain link to `https://wa.me/<number>?text=...`, and the whole
thing is about 3 KB of inline HTML and CSS.

## What it does

- A round button in the bottom-right or bottom-left corner, in WhatsApp green
  or your own colour, with an optional label such as "Questions? Chat with us"
  (hidden on phones so it never covers the page).
- A preset message the visitor can still edit before sending. Without one,
  a short default is used: English, or Dutch on pages with `<html lang="nl">`.
- Optional opening hours: pick the days (Mon-Fri, Mon-Sat or every day), the
  opening and closing time and your time zone. Outside those hours the button
  is hidden. The check runs in the visitor's browser (about 500 bytes of
  script, rechecked every minute) against *your* time zone, so a visitor
  abroad sees the same hours you do. An unknown time zone shows the button
  rather than hiding it.
- CSS-only animation: a soft pulse ring three times after load and a small
  lift on hover. Both are off for visitors with `prefers-reduced-motion`.
- Accessible: a real link with a visible focus ring, an accessible name that
  contains the visible label (or "Chat with us on WhatsApp" when there is no
  label), the icon hidden from screen readers. The default button is WhatsApp's
  own white-on-green logo; with your own colour the logo turns dark or light
  for at least 3:1 contrast. Hidden when printing.

## Settings

*Site settings → WhatsApp chat*:

| Setting | Default |
| --- | --- |
| WhatsApp number | none; the button stays hidden until you enter one |
| Preset message | "Hi! I have a question." / "Hallo! Ik heb een vraag." |
| Label next to the button | none (round button only) |
| Position | bottom right |
| Button colour | `#25D366` (WhatsApp green, always with the white WhatsApp logo) |
| Gentle pulse animation | on |
| Only show during opening hours | off |
| Open on / Opens at / Closes at | Monday to Friday, 09:00 to 17:00 |
| Time zone | `Europe/Amsterdam` |

The number must include its country code (`+31 6 12345678`, `0031 6 ...`);
a national number such as `06 ...` is refused with a clear message, because
wa.me cannot know which country it belongs to. Times are `HH:MM`, 24-hour,
and closing must be after opening. Colours must be hex. The label and every
other value are escaped; the message is URL-encoded.

## Permissions

- **`storage`** - to keep the panel's settings (one key, `config`). That is
  the only permission. If it is refused, the plugin cannot remember a number,
  so it leaves pages untouched, and saving the panel explains why.

No `read-pages`, no `network`: the plugin only adds its button to the HTML
it is handed.

## Build

```
./build.sh          # or, from the registry root: tools/build-plugin.sh whatsapp-chat
stride plugin test .
```

The toolchain is pinned in `rust-toolchain.toml` and `Cargo.lock` is
committed, so the module is reproducible byte for byte.

## Trademark

WhatsApp and the WhatsApp logo are trademarks of Meta Platforms, Inc. This
plugin is not affiliated with, endorsed or sponsored by Meta or WhatsApp. It
uses the WhatsApp logo (the simple-icons glyph, CC0) only to link to WhatsApp,
as Meta's brand guidelines allow.
