# Lite Video

YouTube and Vimeo videos that load only when a visitor presses play.

Every YouTube or Vimeo player on a published page is replaced by a poster
drawn with CSS: the video's title and a round play button. No frame, script,
image or font comes from YouTube or Vimeo until a visitor presses play. Then
the real player loads, with autoplay, in the same place.

- YouTube videos play from `youtube-nocookie.com`, even if the page asked for
  `youtube.com`.
- The original frame, with its sandbox and permissions, is kept in a
  `<template>`; only its address changes. A template's content is never
  loaded by the browser.
- The play button is a real link to the video, so without JavaScript it
  still leads there. It has a label naming the video, a visible focus ring,
  and works with Enter and Space.
- Frames with `width` and `height` attributes keep their proportions; others
  are 16:9.
- Other frames (maps, forms) are left alone, and so is any frame carrying
  `data-lite-video-skip`.

Changes apply to a page the next time it is published.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| Use lite videos | on | Off shows the normal players again. |
| Play button colour | `#DC2626` | Any hex colour; the play icon is black or white, whichever reads better. |
| Explain what pressing play does | on | A short "Plays from YouTube" line on the poster. |
| Show YouTube thumbnails | off | Uses the video's picture from `i.ytimg.com` as the poster. YouTube then sees every visitor before they press play, so it is off by default. Vimeo has no thumbnail address that works without its API, so Vimeo posters stay plain. |
| Remember the visitor's choice | off | After a visitor plays one video, players load straight away on their next visits. Kept in their own browser (`localStorage`) only. |

## Permissions

- `storage`: to keep the settings above. If it is refused the plugin still
  works, with the defaults, and the settings panel says why it cannot save.

No `http`, no page writes: the whole job happens in `on_page_render`.

## Limits

- Only `<iframe>` players are handled, not plain links to a video.
- The poster shows the title from the frame's `title` attribute; without one
  it says "Play video on YouTube".
- Autoplay after the press depends on the browser; if it is blocked, the
  visitor presses play once more inside the player.

## Build

```sh
./build.sh                                   # the registry's reproducible build
stride plugin test .
```
