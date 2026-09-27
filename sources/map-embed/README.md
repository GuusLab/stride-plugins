# Privacy Map

An OpenStreetMap of one place that loads only after a visitor clicks
**Show map**. Until then the page shows a placeholder drawn with CSS: the
place name, the address and the button. Nothing is fetched from another site
before the click, so there is nothing to ask consent for.

## What it adds to a page

- `[map]` on a page becomes the map. A paragraph holding only the code is
  replaced whole, since a section may not sit inside `<p>`.
- An element with a `data-privacy-map` attribute (for example
  `<div data-privacy-map></div>` in a template) is replaced by the map too.
- If a page is chosen in the settings and has no code or marker, the map is
  added at the end of its content: before `</main>`, else before the footer,
  else before `</body>`.
- **Show map** is a real link to openstreetmap.org, so without JavaScript it
  still leads to the map. With JavaScript (under a kilobyte, inline) it swaps
  the placeholder for an `<iframe>` of OpenStreetMap's embed, titled
  "Map of <place name>", with `referrerpolicy="no-referrer"`, and moves focus
  to it. Space and Enter both work on the button.
- A note under the button says the map loads from OpenStreetMap, which then
  sees the visitor's IP address. An "Open in OpenStreetMap" link sits under
  the map. The button and note are left off printed pages.
- Without a location, `[map]` codes and markers are removed so visitors never
  see them, and nothing else changes.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show the map | on | Master switch. |
| Place name | empty | Heading on the placeholder and the map's accessible name. |
| Address | empty | One line per line; shown on the placeholder. |
| Location on the map | empty | Coordinates (`52.3731, 4.8922`), a `geo:` URI, or an openstreetmap.org link (`mlat`/`mlon` or `#map=zoom/lat/lon`). A value that is not a place is refused. |
| Zoom | 16 | 3 (a country) to 19 (a building). |
| Height in pixels | 360 | 200 to 720. |
| Also add it at the end of this page | empty | A slug like `contact`. |
| Remember the visitor's choice | off | After one click, later visits show maps straight away. Kept in the visitor's `localStorage` under `stride-privacy-map`, nowhere else. |
| Button colour | `#C2410C` | Text on it is white or dark, whichever contrasts more. |

There is no geocoding: turning an address into coordinates would mean a
request to a third party from your server, and a permission to make it.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it no map is shown, because there is no location to show; pages are left as they are. |

It asks for nothing else: no network access (the map is loaded by the
visitor's browser, after the click) and no `write-pages` (pages are changed
only as they are served, never in the stored document).

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh map-embed
cargo test          # unit tests
stride plugin test .
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
