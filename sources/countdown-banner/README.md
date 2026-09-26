# Countdown Banner

A bar that counts down to a deadline, with an optional button, and hides itself
(or thanks your visitors) when the time is up.

## What it adds to a page

- A bar right after `<body>` (or floating at the bottom), a stylesheet in
  `<head>`, and a small script before `</body>` that runs the clock.
- The end time is entered as wall time in a chosen time zone and converted in
  the browser with `Intl`, so daylight saving time is handled.
- Without JavaScript the bar shows the message and "Ends 31 October 2026,
  18:00" in the chosen language.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show the countdown | on | Master switch. |
| Message | empty | Nothing is shown while it is empty. |
| Ends at | empty | `YYYY-MM-DD HH:MM`; a date alone ends at 23:59. |
| Time zone of that time | Amsterdam | Twelve common zones. |
| Button text, Button link | empty | Both or neither. |
| When the time is up | Hide the bar | Or show the message below. |
| Message after the end | empty | |
| Language of the clock | English | Or Dutch, German, French. |
| Position | Top | Or floating at the bottom. |
| Colour | `#111827` | Text colour is picked for contrast. |
| Skip these pages | empty | Comma-separated slugs; a trailing `*` matches every page under it. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the countdown. Without it nothing is shown. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh countdown-banner
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
