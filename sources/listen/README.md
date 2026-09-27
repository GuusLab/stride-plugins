# Listen

A "Listen · 4 min" button under the title of long pages that reads the page
aloud with the device's own voices, highlighting the paragraph being read.

## What it adds to a page

- A small toolbar right after the first `<h1>` (hidden until a voice for the
  page's language is found), a stylesheet and an inline script.
- Uses the Web Speech API: nothing is sent anywhere. Text is spoken a
  sentence at a time, at most 220 characters, because some browsers stop a
  long utterance after about fifteen seconds.
- Reads headings, paragraphs, list items, quotes and captions inside `<main>`,
  skipping the menu, header, footer, forms and hidden content.

Stride runs page hooks each time a page is served, so settings take effect
right away.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Show a Listen button | on | Master switch. |
| Only on pages with at least this many words | 250 | |
| Highlight the paragraph being read | on | And keep it in view. |
| Button colour | `#1D4ED8` | Text colour is picked for contrast. |
| Language of the button | The language of the page | The voice always follows the page. |
| Pages, Page list | Every page except `home, contact` | Or every page, or only the listed ones. |

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the settings. Without it the defaults apply. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh listen
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
