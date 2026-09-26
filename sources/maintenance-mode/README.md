# Maintenance Mode

Shows visitors a clean coming-soon or maintenance page while you build the
site. Editors who are signed in to this Stride still see the real pages, with
a small "Maintenance mode is on" badge in the corner.

With no configuration at all, installing and enabling the plugin puts up a
dark "Something new is on its way" page. Pages whose `<html lang>` starts with
`nl` get Dutch default text.

## How it works

`on_page_render` adds three things to every page:

- In `<head>`: `<meta name="robots" content="noindex">`, plus a stylesheet
  that hides every child of `<body>` except the maintenance page.
- At the start of `<body>`: the maintenance page itself, a full-screen
  `<main>` with a heading, your message, the launch date and a contact line.
- A script of about 1 KB. It runs the countdown and asks the server
  (`POST /api/actions/system.whoami`, same origin) who the visitor is. Only an
  account that can edit pages takes the curtain down: `*` or a `documents:`
  scope. Site members and anonymous visitors can't.

The page is hidden before any script runs, so it **fails closed**. Without
JavaScript, or if that request fails, visitors see only the maintenance page.
Nothing is loaded from third parties.

## Settings

Found under Settings → Plugins → Maintenance mode.

| Setting | Default | What it does |
| --- | --- | --- |
| Show the maintenance page to visitors | on | Turn it off to open the site. |
| Kind of page | Coming soon | "Coming soon" or "Maintenance (back shortly)". This changes the default headline, message and label. |
| Site name | empty | Shown small above the headline. |
| Headline, Message | empty | Empty uses the default text for the chosen kind of page. A blank line in the message starts a new paragraph. |
| Launch date | empty | `2026-11-01` or `2026-11-01 09:00`, in the visitor's local time. Shows "Launching 1 November 2026" and a live countdown, which hides itself once the moment has passed. The date is checked when you save. |
| Contact line | empty | An e-mail address becomes a `mailto:` link. Any other text, such as a phone number, is shown exactly as written. |
| Look | Dark | Dark or light. |
| Accent colour | `#64748B` | Used for the background glow, the status dot and link underlines. Text colours don't change, so contrast stays AA. |
| Remind editors that maintenance mode is on | on | Shows the badge to signed-in editors. |

Everything you type is HTML-escaped. The countdown uses `role="timer"`, so it
isn't announced every second, and the date is also written out as plain text.
The entrance animation respects `prefers-reduced-motion`.

## Permissions

- **`storage`**: keeps the panel's settings under one key.

It asks for nothing else. If `storage` is refused, the plugin still shows its
default maintenance page. A site that installed this to stay private shouldn't
become public just because a permission was narrowed. The panel then says it
cannot save. To take the page down, disable or remove the plugin.

## Limits

- **This is a curtain, not access control.** The real page is still in the
  HTML source; it is only hidden. Don't put anything confidential on the site
  while this is on.
- **Editors are only recognised on the same origin as the admin.** If the
  public site is on a different domain than the one you sign in on, the
  session cookie isn't sent, and editors see the maintenance page too. Preview
  in the editor instead.
- **Only rendered pages are covered.** Routes that don't go through the page
  hook, such as `/sitemap.xml`, `/feed.xml` and `/blog`, are served as usual.
- **The HTTP status is still 200.** A plugin can't change it. The `noindex`
  meta tag keeps search engines from indexing the placeholder.
- **Changes can take up to a minute to appear.** Pages are cached for 60
  seconds.

## Build

```sh
./build.sh            # or, from the registry root: tools/build-plugin.sh maintenance-mode
stride plugin test .
```
