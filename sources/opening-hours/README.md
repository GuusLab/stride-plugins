# Opening Hours

Opening hours as a table and a live "Open now · closes at 17:00" badge, placed
where an editor types `[opening-hours]` or `[open-now]`.

## What it adds to a page

- `[opening-hours]` becomes a table (a paragraph holding only the code is
  replaced whole, since a table may not sit inside `<p>`); `[open-now]`
  becomes the badge. Optionally the badge or table also goes in the footer.
- The table is plain HTML. A small script works out "open now" in the
  business's time zone when the page is viewed, fills in the badge and marks
  today's row with `aria-current="date"`.
- When a page has both codes, the table leaves out its own badge.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show opening hours | on | Master switch. |
| Monday … Sunday | 09:00-17:00, Sat 10:00-16:00, Sun closed | `09:00-17:00`, two ranges with a comma, `closed`, or `24h`. |
| Note under the hours | empty | |
| Your time zone | Amsterdam | Eight common zones. |
| Language | English | Or Dutch, German, French. |
| Also show in the footer | No | Or the badge, or the full table. |
| Colour for Open now | `#15803D` | |

Hours past midnight (`22:00-02:00`) count until the end of that day.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep your hours. Without it nothing is shown. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh opening-hours
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
