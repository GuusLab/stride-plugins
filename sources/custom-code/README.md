# Custom Code

Your own snippets in the head or body of every published page: analytics,
verification tags, fonts, a tag manager or a chat widget. No theme edits.

## What it adds to a page

- The **head** snippet just before `</head>`, the **start of body** snippet
  right after `<body …>`, the **end of body** snippet just before `</body>`.
- Each snippet between `<!-- stride:custom-code:… -->` comments, so the page
  source says where it came from. A page that already has them is left alone.
- The code is inserted **as pasted, not escaped**. That is the point of the
  plugin, and the reason the panel says to paste only code you trust.

Changes to the settings apply to a page the next time it is published.

## Settings

On the plugin's own page under **Extensions, Installed** (a `site-settings`
panel).

| Setting | Default | What it does |
| --- | --- | --- |
| Add the code to pages | on | Master switch; turning it off keeps your code. |
| In the head | empty | Up to 8192 characters. |
| At the start of the body | empty | Up to 8192 characters. |
| At the end of the body | empty | Up to 8192 characters. |
| Pages | Every page | Or only the listed pages, or every page except them. |
| Page list | empty | Comma-separated slugs; a trailing `*` matches every page under it. |

Saving refuses a snippet with unbalanced `<script>` tags, one that contains
`<html>`, `<head>` or `<body>` (a whole page pasted by mistake), or one over
the length limit, with a message that says how to fix it.

## Permissions

| Permission | Why |
| --- | --- |
| `storage` | To keep the snippets. Without it no code is added and saving says why. |

## Building

```sh
./build.sh          # runs ../../tools/build-plugin.sh custom-code
cargo test          # unit tests
```

The toolchain is pinned in `rust-toolchain.toml`, `Cargo.lock` is committed,
and the build is reproducible byte for byte.
