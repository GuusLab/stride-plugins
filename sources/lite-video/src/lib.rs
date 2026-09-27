//! Lite Video: YouTube and Vimeo embeds that load only when a visitor asks.
//!
//! Every `<iframe>` pointing at a YouTube or Vimeo player is replaced by a
//! poster drawn with CSS: the video's title and a play button. The original
//! frame is kept, untouched apart from its address, inside a `<template>`,
//! whose content the browser parses but never loads. Pressing play swaps the
//! poster for that frame with autoplay on, so one click starts the video.
//!
//! YouTube frames are moved to youtube-nocookie.com, which sets no cookies
//! until the video plays. The play button is a real link to the video, so
//! without JavaScript it still leads there.
//!
//! Permissions: `storage`, for the settings. Without it the plugin still
//! works, with its defaults.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};
use util::{escape, find_ci, insert_before_body_end, insert_in_head, is_colour, readable_on};

const SETTINGS: &str = "settings";
const DONE: &str = "class=\"lv-style\"";
const SKIP: &str = "data-lite-video-skip";

// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub colour: String,
    pub note: bool,
    pub thumbnails: bool,
    pub remember: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            colour: "#DC2626".to_owned(),
            note: true,
            thumbnails: false,
            remember: false,
        }
    }
}

impl Settings {
    /// Refused or missing storage falls back to the defaults, which are
    /// meant to be good enough on their own.
    fn load() -> Self {
        match kv::get::<Map<String, JsonValue>>(SETTINGS) {
            Ok(Some(values)) => Settings::from_values(&values),
            Ok(None) => Settings::default(),
            Err(error) => {
                if !error.is_permission_denied() {
                    stride_pdk::log("warn", &format!("settings unreadable, using defaults: {error}"));
                }
                Settings::default()
            }
        }
    }

    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Settings::default();
        let flag = |name: &str, fallback: bool| util::flag(values, name, fallback);
        let colour = util::text(values, "colour");
        Settings {
            enabled: flag("enabled", d.enabled),
            colour: if is_colour(&colour) { colour.to_ascii_uppercase() } else { d.colour },
            note: flag("note", d.note),
            thumbnails: flag("thumbnails", d.thumbnails),
            remember: flag("remember", d.remember),
        }
    }

    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut v = Map::new();
        v.insert("enabled".into(), self.enabled.into());
        v.insert("colour".into(), self.colour.clone().into());
        v.insert("note".into(), self.note.into());
        v.insert("thumbnails".into(), self.thumbnails.into());
        v.insert("remember".into(), self.remember.into());
        v
    }
}

// ------------------------------------------------------------------ videos

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Provider {
    YouTube,
    Vimeo,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Provider::YouTube => "YouTube",
            Provider::Vimeo => "Vimeo",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    pub provider: Provider,
    pub id: String,
    /// The player address to load on play, still HTML-escaped as written.
    pub src: String,
}

fn valid_id(id: &str, provider: Provider) -> bool {
    match provider {
        Provider::YouTube => (6..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        Provider::Vimeo => (1..=15).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()),
    }
}

/// A YouTube or Vimeo player address, or `None` for anything else. Hosts
/// match exactly, and userinfo or a port rule a URL out, so a lookalike such
/// as `youtube.com@evil.example` is never treated as a video.
pub fn parse_player(src: &str) -> Option<Video> {
    let src = src.trim();
    let rest = src.strip_prefix("https://").or_else(|| src.strip_prefix("//"))?;
    let split = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(split);
    if host.contains(['@', ':']) {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let (provider, tail, nocookie) = match host.as_str() {
        "www.youtube-nocookie.com" | "youtube-nocookie.com" => (Provider::YouTube, path.strip_prefix("/embed/")?, true),
        "www.youtube.com" | "youtube.com" | "m.youtube.com" => (Provider::YouTube, path.strip_prefix("/embed/")?, false),
        "player.vimeo.com" => (Provider::Vimeo, path.strip_prefix("/video/")?, true),
        _ => return None,
    };
    let end = tail.find(['/', '?', '#', '&']).unwrap_or(tail.len());
    let id = &tail[..end];
    if !valid_id(id, provider) {
        return None;
    }
    let src = if nocookie && host != "youtube-nocookie.com" {
        format!("https://{host}{path}")
    } else if provider == Provider::YouTube {
        format!("https://www.youtube-nocookie.com{path}")
    } else {
        format!("https://{host}{path}")
    };
    Some(Video { provider, id: id.to_owned(), src })
}

fn watch_url(v: &Video) -> String {
    match v.provider {
        Provider::YouTube => format!("https://www.youtube.com/watch?v={}", v.id),
        Provider::Vimeo => format!("https://vimeo.com/{}", v.id),
    }
}

// ------------------------------------------------------------------ tags

/// The raw value of `name` in an opening tag, or its span in `tag`.
fn attribute(tag: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = tag.as_bytes();
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before_ok = at > 0 && bytes[at - 1].is_ascii_whitespace();
        let mut i = at + name.len();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if !before_ok || bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        return match bytes.get(i) {
            Some(&q @ (b'"' | b'\'')) => {
                let end = tag[i + 1..].find(q as char)? + i + 1;
                Some((i + 1, end))
            }
            Some(_) => {
                let end = tag[i..].find(|c: char| c.is_ascii_whitespace() || c == '>').map_or(tag.len(), |e| e + i);
                Some((i, end))
            }
            None => None,
        };
    }
    None
}

fn value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    attribute(tag, name).map(|(a, b)| &tag[a..b])
}

/// Undo the entities an escaped attribute holds; `&amp;` last, so that
/// `&amp;lt;` stays the text `&lt;`.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#38;", "&")
        .replace("&amp;", "&")
}

const PLAY: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"M8 5.5v13a1 1 0 0 0 1.5.86l10.6-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z\"/></svg>";

/// The poster that stands in for one frame. `tag` is the original opening
/// tag; it goes into the template with only its address changed.
fn poster(tag: &str, v: &Video, s: &Settings) -> String {
    let title = value(tag, "title").map(unescape).map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    let label = match &title {
        Some(t) => format!("Play video: {t}"),
        None => format!("Play video on {}", v.provider.name()),
    };
    let class = value(tag, "class").unwrap_or_default();
    let style = value(tag, "style").unwrap_or_default();
    // A frame sized by width and height attributes keeps its proportions.
    let ratio = match (value(tag, "width").and_then(|w| w.trim().parse::<u32>().ok()), value(tag, "height").and_then(|h| h.trim().parse::<u32>().ok())) {
        (Some(w), Some(h)) if w >= 100 && h >= 50 => format!("aspect-ratio:{w}/{h};max-width:{w}px;"),
        _ => String::new(),
    };
    let (a, b) = attribute(tag, "src").unwrap_or((0, 0));
    let frame = format!("{}{}{}", &tag[..a], escape(&unescape(&v.src)), &tag[b..]);
    let image = if s.thumbnails && v.provider == Provider::YouTube {
        format!(
            "<img class=\"lv-img\" src=\"https://i.ytimg.com/vi/{}/hqdefault.jpg\" alt=\"\" loading=\"lazy\" decoding=\"async\" referrerpolicy=\"no-referrer\">",
            v.id
        )
    } else {
        String::new()
    };
    let heading = title.as_deref().map(|t| format!("<span class=\"lv-title\">{}</span>", escape(t))).unwrap_or_default();
    let note = if s.note {
        format!("<span class=\"lv-note\">Plays from {}</span>", v.provider.name())
    } else {
        String::new()
    };
    let style = format!("{ratio}{style}");
    let style = if style.is_empty() { String::new() } else { format!(" style=\"{style}\"") };
    format!(
        "<div class=\"lv{sep}{class}\"{style} data-lv>{image}\
<a class=\"lv-play\" href=\"{watch}\" target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"{label}\">\
{heading}<span class=\"lv-btn\">{PLAY}</span>{note}</a>\
<template>{frame}</iframe></template></div>",
        sep = if class.is_empty() { "" } else { " " },
        watch = escape(&watch_url(v)),
        label = escape(&label),
    )
}

fn style(s: &Settings) -> String {
    format!(
        "<style {DONE}>\
.lv{{--lv:{c};--lv-fg:{fg};position:relative;display:block;box-sizing:border-box;width:100%;aspect-ratio:16/9;overflow:hidden;border-radius:12px;\
background:radial-gradient(120% 90% at 50% 50%,color-mix(in srgb,var(--lv) 22%,#1f2937),#0b0f17);color:#fff}}\
.lv-img{{position:absolute;inset:0;width:100%;height:100%;object-fit:cover}}\
.lv-play{{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;color:inherit;text-decoration:none;\
background:linear-gradient(180deg,rgba(0,0,0,.55),transparent 35%,transparent 70%,rgba(0,0,0,.45))}}\
.lv-title{{position:absolute;top:0;left:0;right:0;padding:1rem 1.25rem;font-weight:600;font-size:clamp(.95rem,2.2vw,1.2rem);line-height:1.35;\
overflow:hidden;text-overflow:ellipsis;white-space:nowrap;text-shadow:0 1px 3px rgba(0,0,0,.5)}}\
.lv-btn{{display:flex;align-items:center;justify-content:center;width:clamp(56px,11%,84px);aspect-ratio:1;border-radius:50%;background:var(--lv);color:var(--lv-fg);\
box-shadow:0 6px 24px rgba(0,0,0,.35);transition:transform .15s ease}}\
.lv-btn svg{{width:44%;height:44%;fill:currentColor}}\
.lv-play:hover .lv-btn{{transform:scale(1.08)}}\
.lv-play:focus-visible{{outline:none}}.lv-play:focus-visible .lv-btn{{outline:3px solid #fff;outline-offset:4px}}\
.lv-note{{position:absolute;bottom:.8rem;left:0;right:0;text-align:center;font-size:.8rem;opacity:.85}}\
.lv>iframe{{display:block;width:100%;height:100%;border:0}}\
@media (prefers-reduced-motion:reduce){{.lv-btn{{transition:none}}}}\
@media print{{.lv-btn,.lv-note{{display:none}}}}\
</style>",
        c = s.colour,
        fg = readable_on(&s.colour),
    )
}

fn script(remember: bool) -> String {
    format!(
        "<script>(function(){{var K='stride-lite-video',r={remember},ok=0;\
if(r)try{{ok=localStorage.getItem(K)==='1'}}catch(e){{}}\
function show(p,go){{var t=p.querySelector('template'),f=t&&t.content.querySelector('iframe');if(!f)return;f=f.cloneNode(true);\
if(go){{f.src=f.src+(f.src.indexOf('?')<0?'?':'&')+'autoplay=1';f.removeAttribute('loading')}}\
while(p.firstChild)p.removeChild(p.firstChild);p.removeAttribute('data-lv');p.style.background='#000';p.appendChild(f);if(go)f.focus()}}\
document.querySelectorAll('.lv[data-lv]').forEach(function(p){{if(ok)return show(p);var a=p.querySelector('.lv-play');if(!a)return;\
a.setAttribute('role','button');a.removeAttribute('target');\
a.addEventListener('click',function(e){{e.preventDefault();if(r)try{{localStorage.setItem(K,'1')}}catch(x){{}}show(p,1)}});\
a.addEventListener('keydown',function(e){{if(e.key===' '){{e.preventDefault();a.click()}}}})}})}})();</script>"
    )
}

// ------------------------------------------------------------------ render

pub fn render(html: &str, s: &Settings) -> Option<String> {
    if !s.enabled || html.contains(DONE) {
        return None;
    }
    let start = find_ci(html, "<body").and_then(|at| html[at..].find('>').map(|g| at + g + 1)).unwrap_or(0);
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() + 2048);
    out.push_str(&html[..start]);
    let mut pos = start;
    let mut count = 0;
    while let Some(found) = lower[pos..].find("<iframe") {
        let at = pos + found;
        let Some(gt) = html[at..].find('>').map(|g| at + g) else { break };
        let Some(close) = lower[gt..].find("</iframe>").map(|c| gt + c) else { break };
        let tag = &html[at..=gt];
        let video = (!tag.contains(SKIP))
            .then(|| value(tag, "src"))
            .flatten()
            .and_then(|src| parse_player(&unescape(src)));
        out.push_str(&html[pos..at]);
        match video {
            Some(v) => {
                // A self-closing slash would end up inside the template tag.
                let tag = tag.strip_suffix("/>").map(|t| format!("{t}>")).unwrap_or_else(|| tag.to_owned());
                out.push_str(&poster(&tag, &v, s));
                count += 1;
            }
            None => out.push_str(&html[at..close + 9]),
        }
        pos = close + 9;
    }
    if count == 0 {
        return None;
    }
    out.push_str(&html[pos..]);
    insert_in_head(&mut out, &style(s));
    insert_before_body_end(&mut out, &script(s.remember));
    Some(out)
}

// ------------------------------------------------------------------ exports

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let html = render(&page.html, &Settings::load()).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

#[plugin_fn]
pub fn panel_lite_video(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse { values: Settings::load().to_values(), ..PanelResponse::default() })),
        PanelEvent::Submit => {
            let values = request.values.clone();
            let colour = util::text(&values, "colour");
            if !colour.is_empty() && !is_colour(&colour) {
                return Ok(Json(PanelResponse {
                    values,
                    message: String::new(),
                    error: "The play button colour has to be a hex colour like #DC2626. Nothing was saved.".to_owned(),
                }));
            }
            let settings = Settings::from_values(&values);
            if let Err(error) = kv::set(SETTINGS, &settings.to_values()) {
                let error = if error.is_permission_denied() {
                    "This plugin was not granted the storage permission, so it cannot keep settings. Videos still load on click, with the default look.".to_owned()
                } else {
                    format!("The settings could not be saved: {error}")
                };
                return Ok(Json(PanelResponse { values, message: String::new(), error }));
            }
            Ok(Json(PanelResponse {
                values: settings.to_values(),
                message: "Saved. Publish a page again to see the change.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIDE: &str = "<iframe class=\"e1\" src=\"https://www.youtube-nocookie.com/embed/aqz-KE-bpKQ\" title=\"Atelier &lt;in&gt; motion\" loading=\"lazy\" sandbox=\"allow-scripts allow-same-origin\" allowfullscreen></iframe>";

    fn page(body: &str) -> String {
        format!("<html><head><title>t</title></head><body><main>{body}</main></body></html>")
    }

    #[test]
    fn players() {
        let v = parse_player("https://www.youtube.com/embed/aqz-KE-bpKQ?start=10&rel=0").unwrap();
        assert_eq!(v.provider, Provider::YouTube);
        assert_eq!(v.src, "https://www.youtube-nocookie.com/embed/aqz-KE-bpKQ?start=10&rel=0");
        let v = parse_player("https://player.vimeo.com/video/76979871?h=abc").unwrap();
        assert_eq!((v.provider, v.id.as_str()), (Provider::Vimeo, "76979871"));
        for bad in [
            "https://www.youtube.com@evil.example/embed/aqz-KE-bpKQ",
            "https://www.youtube.com.evil.example/embed/aqz-KE-bpKQ",
            "https://www.youtube.com:8080/embed/aqz-KE-bpKQ",
            "http://www.youtube.com/embed/aqz-KE-bpKQ",
            "https://www.youtube.com/embed/<script>",
            "https://player.vimeo.com/video/abc",
            "https://www.google.com/maps/embed?pb=1",
        ] {
            assert!(parse_player(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn frame_becomes_poster_with_no_third_party_request() {
        let out = render(&page(STRIDE), &Settings::default()).unwrap();
        let outside = out.replace(&out[out.find("<template>").unwrap()..out.find("</template>").unwrap()], "");
        assert!(!outside.contains("<iframe"));
        assert!(!outside.contains("ytimg"));
        assert!(out.contains("<template><iframe class=\"e1\" src=\"https://www.youtube-nocookie.com/embed/aqz-KE-bpKQ\""));
        assert!(out.contains("sandbox=\"allow-scripts allow-same-origin\""));
        assert!(out.contains("aria-label=\"Play video: Atelier &lt;in&gt; motion\""));
        assert!(out.contains("class=\"lv e1\""));
        assert!(out.contains("href=\"https://www.youtube.com/watch?v=aqz-KE-bpKQ\""));
        assert!(out.contains(DONE));
        assert!(render(&out, &Settings::default()).is_none());
    }

    #[test]
    fn query_ampersands_stay_escaped() {
        let tag = "<iframe src=\"https://www.youtube.com/embed/aqz-KE-bpKQ?a=1&amp;b=2\"></iframe>";
        let out = render(&page(tag), &Settings::default()).unwrap();
        assert!(out.contains("embed/aqz-KE-bpKQ?a=1&amp;b=2\""));
    }

    #[test]
    fn other_frames_and_head_are_left_alone() {
        let map = "<iframe src=\"https://www.google.com/maps/embed?pb=1\"></iframe>";
        assert!(render(&page(map), &Settings::default()).is_none());
        let skip = "<iframe data-lite-video-skip src=\"https://player.vimeo.com/video/1\"></iframe>";
        assert!(render(&page(skip), &Settings::default()).is_none());
        let mixed = format!("{map}<p>x</p>{STRIDE}");
        let out = render(&page(&mixed), &Settings::default()).unwrap();
        assert!(out.contains(map) && out.contains("<p>x</p>"));
    }

    #[test]
    fn settings() {
        let s = Settings { thumbnails: true, note: false, ..Settings::default() };
        let out = render(&page(STRIDE), &s).unwrap();
        assert!(out.contains("i.ytimg.com/vi/aqz-KE-bpKQ/hqdefault.jpg"));
        assert!(!out.contains("lv-note\">"));
        assert!(render(&page(STRIDE), &Settings { enabled: false, ..Settings::default() }).is_none());
        let mut v = Map::new();
        v.insert("colour".into(), "javascript:".into());
        assert_eq!(Settings::from_values(&v).colour, "#DC2626");
    }

    #[test]
    fn sized_frames_keep_their_shape() {
        let tag = "<iframe width=\"560\" height=\"315\" src=\"https://player.vimeo.com/video/76979871\"></iframe>";
        let out = render(&page(tag), &Settings::default()).unwrap();
        assert!(out.contains("aspect-ratio:560/315;max-width:560px;"));
        assert!(out.contains("Play video on Vimeo"));
    }
}
