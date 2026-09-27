//! Heading Anchors: a Stride plugin.
//!
//! It puts a small link beside every section heading (H2 and H3 by default)
//! that fades in on hover or keyboard focus. Clicking it scrolls smoothly to
//! the section, puts the section in the address bar and copies the link, with
//! a short "Link copied" message. Headings without an id get a stable one made
//! from their text, the same way Table of Contents makes them, so both plugins
//! agree on every link. No third-party requests.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-ha";
const DEFAULT_COLOR: &str = "#2563eb";
const DEFAULT_COPIED: &str = "Link copied";
const MAX_SLUG: usize = 64;

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = match kv::get::<JsonValue>(SETTINGS_KEY) {
        Ok(Some(value)) => Settings::from_json(&value),
        Ok(None) => Settings::default(),
        // Refused storage is an answer, not a fault: run with the defaults.
        Err(error) if error.is_permission_denied() => Settings::default(),
        Err(error) => {
            stride_pdk::log("info", &format!("settings unreadable, using defaults: {error}"));
            Settings::default()
        }
    };
    let html = build(&page.html, &page.slug, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_heading_anchors(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let stored = kv::get::<JsonValue>(SETTINGS_KEY);
    let current = match &stored {
        Ok(Some(value)) => Settings::from_json(value),
        _ => Settings::default(),
    };
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: current.to_values(),
            message: match stored {
                Err(error) if error.is_permission_denied() => {
                    "This plugin was not granted storage, so these settings cannot be saved. \
                     It still adds heading links with the defaults shown here."
                        .to_owned()
                }
                _ => String::new(),
            },
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let submitted = match Settings::from_values(&request.values) {
                Ok(settings) => settings,
                Err(error) => {
                    return Ok(Json(PanelResponse {
                        values: current.to_values(),
                        message: String::new(),
                        error,
                    }));
                }
            };
            if let Err(error) = kv::set(SETTINGS_KEY, &submitted.to_json()) {
                return Ok(Json(PanelResponse {
                    values: submitted.to_values(),
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is \
                         nowhere to keep these settings. Nothing was saved; the defaults \
                         stay in effect."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                }));
            }
            let message = if submitted.enabled {
                "Saved. Publish the site again to update pages that are already live."
            } else {
                "Saved. Heading links are off. Publish the site again to remove them from \
                 pages that are already live."
            };
            Ok(Json(PanelResponse {
                values: submitted.to_values(),
                message: message.to_owned(),
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Hash,
    Link,
    Section,
}

impl Symbol {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "hash" => Some(Symbol::Hash),
            "link" => Some(Symbol::Link),
            "section" => Some(Symbol::Section),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Symbol::Hash => "hash",
            Symbol::Link => "link",
            Symbol::Section => "section",
        }
    }
    fn markup(self) -> &'static str {
        match self {
            Symbol::Hash => "#",
            Symbol::Section => "\u{a7}",
            Symbol::Link => {
                "<svg viewBox=\"0 0 24 24\" width=\"1em\" height=\"1em\" focusable=\"false\">\
<path d=\"M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1.2 1.2M14 10a4.5 4.5 0 0 0-6.4 0l-3 3\
a4.5 4.5 0 0 0 6.4 6.4l1.2-1.2\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" \
stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// The deepest heading level that gets a link: 2, 3 or 4.
    pub depth: u8,
    pub symbol: Symbol,
    pub before: bool,
    /// `#rrggbb`, or empty for the default blue.
    pub color: String,
    pub copy: bool,
    /// Plain text, or empty for "Link copied".
    pub copied_text: String,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            depth: 3,
            symbol: Symbol::Hash,
            before: false,
            color: String::new(),
            copy: true,
            copied_text: String::new(),
            exclude: Vec::new(),
        }
    }
}

fn parse_levels(value: &str) -> Option<u8> {
    match value {
        "h2" => Some(2),
        "h2-h3" => Some(3),
        "h2-h4" => Some(4),
        _ => None,
    }
}

fn levels_name(depth: u8) -> &'static str {
    match depth {
        2 => "h2",
        4 => "h2-h4",
        _ => "h2-h3",
    }
}

/// Trimmed, one line, at most 60 characters.
fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect()
}

impl Settings {
    /// Whatever is stored, never fail: a field that is missing or the wrong
    /// shape falls back to its default.
    pub fn from_json(value: &JsonValue) -> Self {
        let d = Settings::default();
        let s = |key: &str| value.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| value.get(key).and_then(JsonValue::as_bool);
        Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            depth: s("levels").and_then(parse_levels).unwrap_or(d.depth),
            symbol: s("symbol").and_then(Symbol::parse).unwrap_or(d.symbol),
            before: s("position") == Some("before"),
            color: s("color").and_then(valid_color).unwrap_or_default(),
            copy: b("copy").unwrap_or(d.copy),
            copied_text: s("copiedText").map(clean_text).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("levels".into(), levels_name(self.depth).into());
        map.insert("symbol".into(), self.symbol.name().into());
        map.insert("position".into(), self.position().into());
        map.insert("color".into(), self.color.clone().into());
        map.insert("copy".into(), self.copy.into());
        map.insert("copiedText".into(), self.copied_text.clone().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("levels".into(), levels_name(self.depth).into());
        values.insert("symbol".into(), self.symbol.name().into());
        values.insert("position".into(), self.position().into());
        if !self.color.is_empty() {
            values.insert("color".into(), self.color.clone().into());
        }
        values.insert("copy".into(), self.copy.into());
        values.insert("copied-text".into(), self.copied_text.clone().into());
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| values.get(key).and_then(JsonValue::as_bool);
        let color = match s("color").map(str::trim) {
            None | Some("") => String::new(),
            Some(color) => valid_color(color).ok_or_else(|| {
                "The colour has to look like #2563eb. Nothing was saved.".to_owned()
            })?,
        };
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            depth: s("levels").and_then(parse_levels).unwrap_or(d.depth),
            symbol: s("symbol").and_then(Symbol::parse).unwrap_or(d.symbol),
            before: s("position") == Some("before"),
            color,
            copy: b("copy").unwrap_or(d.copy),
            copied_text: s("copied-text").map(clean_text).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn position(&self) -> &'static str {
        if self.before { "before" } else { "after" }
    }

    fn effective_color(&self) -> &str {
        if self.color.is_empty() { DEFAULT_COLOR } else { &self.color }
    }

    fn effective_copied(&self) -> &str {
        if self.copied_text.is_empty() { DEFAULT_COPIED } else { &self.copied_text }
    }
}

/// `#rgb` or `#rrggbb`, normalised to lowercase `#rrggbb`. Anything else is
/// refused, which is what keeps the value safe to put into CSS.
pub fn valid_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        6 => Some(format!("#{hex}")),
        3 => Some(hex.chars().fold(String::from("#"), |mut out, c| {
            out.push(c);
            out.push(c);
            out
        })),
        _ => None,
    }
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

// ------------------------------------------------------------ the plain work

/// The page with heading links, or `None` to leave it exactly as it was:
/// disabled, excluded, already done, no headings or not a whole document.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let slug = slug.trim_matches('/');
    if settings.exclude.iter().any(|excluded| excluded == slug) {
        return None;
    }
    let bytes = html.as_bytes();
    let (from, to) = content_region(html)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= to)?;
    let mut used = existing_ids(html);
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let symbol = settings.symbol.markup();
    let mut count = 0;

    let mut at = from;
    while at < to {
        let Some(offset) = bytes[at..to].iter().position(|&b| b == b'<') else { break };
        let start = at + offset;
        let level = match heading_level(&bytes[start..to]) {
            Some(level) if (2..=settings.depth).contains(&level) => level,
            _ => {
                at = skip_raw_text(bytes, start, to);
                continue;
            }
        };
        let Some(open_end) = tag_end(bytes, start) else { break };
        let close_tag = [b'<', b'/', b'h', b'0' + level];
        let Some(close) = find_ci(&bytes[open_end..to], &close_tag).map(|o| open_end + o) else {
            break;
        };
        at = close;
        let inner = &html[open_end..close];
        // A link inside a link is invalid HTML; such a heading already links.
        if find_tag(inner.as_bytes(), 0, b"a").is_some() {
            continue;
        }
        let text = heading_text(inner);
        if text.is_empty() {
            continue;
        }
        let id = match attribute(&html[start..open_end], "id") {
            Some(id) if !id.is_empty() && !id.contains(char::is_whitespace) => {
                id.replace('"', "&quot;")
            }
            Some(id) if !id.is_empty() => continue,
            _ => {
                let id = unique(slugify(&text), &mut used);
                // Right after "<h2", before any other attribute.
                inserts.push((start + 3, format!(" id=\"{id}\"")));
                id
            }
        };
        let label = text.replace('"', "&quot;");
        let class = if settings.before { "stride-ha stride-ha-before" } else { "stride-ha" };
        let link = format!(
            "<a class=\"{class}\" href=\"#{id}\" aria-label=\"Link to section: {label}\">\
<span aria-hidden=\"true\">{symbol}</span></a>"
        );
        if settings.before {
            inserts.push((open_end, link));
        } else {
            // After trailing whitespace, so the link hugs the last word.
            let trimmed = open_end + inner.trim_end().len();
            inserts.push((trimmed, link));
        }
        count += 1;
    }
    if count == 0 {
        return None;
    }

    let mut tail = String::new();
    if settings.copy {
        tail.push_str(&format!(
            "<div class=\"stride-ha-toast\" role=\"status\" aria-live=\"polite\" data-text=\"{}\"></div>",
            escape(settings.effective_copied())
        ));
    }
    tail.push_str("<script>");
    tail.push_str(SCRIPT);
    tail.push_str("</script>");
    inserts.push((body_close, tail));
    let style = style(settings);
    match find_ci(bytes, b"</head>").filter(|&at| at < from) {
        Some(head) => inserts.push((head, style)),
        None => inserts.push((from, style)),
    }

    // Back to front, so earlier offsets stay true.
    inserts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = html.to_owned();
    for (at, text) in inserts {
        out.insert_str(at, &text);
    }
    Some(out)
}

/// Smooth scroll, address bar and copy. Delegated, so it is inert when there
/// are no links; modified clicks (new tab, and so on) are left to the browser.
const SCRIPT: &str = "(function(){var d=document,t=d.querySelector('.stride-ha-toast'),h,k;\
d.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('a.stride-ha');\
if(!a||e.button||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;\
var g=d.getElementById(decodeURIComponent(a.hash.slice(1)));if(!g)return;e.preventDefault();\
g.scrollIntoView({behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'auto':'smooth',block:'start'});\
if(location.hash!==a.hash)history.pushState(null,'',a.hash);\
var n=navigator.clipboard;if(!t||!n||!window.isSecureContext)return;\
n.writeText(a.href).then(function(){clearTimeout(h);clearTimeout(k);t.textContent=t.getAttribute('data-text');\
t.classList.add('stride-ha-show');h=setTimeout(function(){t.classList.remove('stride-ha-show');\
k=setTimeout(function(){t.textContent=''},300)},1800)},function(){})})})();";

fn style(settings: &Settings) -> String {
    let color = settings.effective_color();
    let depth = settings.depth;
    let headings = match depth {
        2 => "h2",
        3 => "h2,h3",
        _ => "h2,h3,h4",
    };
    let before = if settings.before {
        format!(
            ":where({headings}):has(>.stride-ha-before){{position:relative}}\
.stride-ha-before{{position:absolute;right:100%;margin:0 .3em 0 0;padding:0 .1em}}\
@media (max-width:720px){{.stride-ha-before{{position:static;margin:0 .3em 0 0}}}}"
        )
    } else {
        String::new()
    };
    format!(
        "<style>:where({headings}):has(>.stride-ha){{scroll-margin-top:5rem}}\
.stride-ha{{margin-left:.4em;padding:0 .12em;border-radius:.2em;color:{color};\
text-decoration:none;font-weight:600;opacity:0;transition:opacity .15s}}\
.stride-ha svg{{width:.78em;height:.78em;vertical-align:-.06em}}\
:where({headings}):hover>.stride-ha,.stride-ha:focus-visible{{opacity:1}}\
.stride-ha:hover{{text-decoration:underline;text-underline-offset:.15em}}\
.stride-ha:focus-visible{{outline:2px solid {color};outline-offset:2px}}\
@media (hover:none){{.stride-ha{{opacity:.5}}}}{before}\
.stride-ha-toast{{position:fixed;left:50%;bottom:24px;z-index:2147483000;\
transform:translate(-50%,8px);display:flex;align-items:center;gap:8px;\
padding:10px 16px;border-radius:999px;background:#111827;color:#fff;\
font:500 14px/1.4 system-ui,-apple-system,'Segoe UI',sans-serif;\
box-shadow:0 6px 20px rgba(0,0,0,.2);opacity:0;visibility:hidden;pointer-events:none;\
transition:opacity .2s,transform .2s,visibility .2s}}\
.stride-ha-toast::before{{content:'';width:8px;height:8px;border-radius:50%;background:{color};\
box-shadow:0 0 0 3px rgba(255,255,255,.18)}}\
.stride-ha-toast.stride-ha-show{{opacity:1;visibility:visible;transform:translate(-50%,0)}}\
@media (prefers-reduced-motion:reduce){{.stride-ha,.stride-ha-toast{{transition:none}}}}\
@media print{{.stride-ha,.stride-ha-toast{{display:none}}}}</style>"
    )
}

/// The byte range to look for headings in: the inside of `<main>`, or the
/// inside of `<body>` when there is no main. `None` for a fragment.
fn content_region(html: &str) -> Option<(usize, usize)> {
    let bytes = html.as_bytes();
    let body = find_tag(bytes, 0, b"body")?;
    let body_end = tag_end(bytes, body)?;
    if let Some(main) = find_tag(bytes, body_end, b"main") {
        let open_end = tag_end(bytes, main)?;
        let close = find_ci(&bytes[open_end..], b"</main").map_or(bytes.len(), |o| open_end + o);
        return Some((open_end, close));
    }
    let close = rfind_ci(bytes, b"</body").filter(|&c| c >= body_end)?;
    Some((body_end, close))
}

fn unique(base: String, used: &mut Vec<String>) -> String {
    let mut candidate = base.clone();
    let mut n = 2;
    while used.iter().any(|id| id == &candidate) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    used.push(candidate.clone());
    candidate
}

/// `needle` must be ASCII, which keeps the answer on a char boundary.
fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

fn rfind_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

/// `Some(n)` when `tag` opens `<hn`.
fn heading_level(tag: &[u8]) -> Option<u8> {
    if tag.len() < 4 || tag[0] != b'<' || !tag[1].eq_ignore_ascii_case(&b'h') {
        return None;
    }
    let level = tag[2];
    if !(b'1'..=b'6').contains(&level) {
        return None;
    }
    matches!(tag[3], b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c').then_some(level - b'0')
}

/// From a `<` that is not a heading: past a `<script>`/`<style>`/`<template>`
/// element entirely, otherwise just past the `<`.
fn skip_raw_text(bytes: &[u8], start: usize, to: usize) -> usize {
    for name in [&b"script"[..], b"style", b"template", b"nav"] {
        if find_tag(&bytes[start..to], 0, name) == Some(0) {
            let mut close = b"</".to_vec();
            close.extend_from_slice(name);
            return find_ci(&bytes[start..to], &close).map_or(to, |o| start + o + close.len());
        }
    }
    start + 1
}

/// The first `<name` followed by whitespace, `>` or `/`, from `from`.
fn find_tag(bytes: &[u8], from: usize, name: &[u8]) -> Option<usize> {
    let mut needle = b"<".to_vec();
    needle.extend_from_slice(name);
    let mut at = from;
    while let Some(offset) = find_ci(&bytes[at..], &needle) {
        let hit = at + offset;
        match bytes.get(hit + needle.len()) {
            Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c') => return Some(hit),
            None => return None,
            _ => at = hit + 1,
        }
    }
    None
}

/// The offset just past the `>` that closes the tag starting at `start`,
/// honouring quoted attribute values.
fn tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
        } else if b == b'"' || b == b'\'' {
            quote = b;
        } else if b == b'>' {
            return Some(i + 1);
        }
    }
    None
}

/// The value of attribute `name` in an opening tag, as written.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 1;
    // Skip the tag name.
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' {
            return None;
        }
        let name_start = i;
        while i < bytes.len() && !matches!(bytes[i], b'=' | b'>' | b'/') && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let attr = &tag[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                let value_start = i + 1;
                let end = tag[value_start..].find(quote as char).map_or(bytes.len(), |o| value_start + o);
                value = tag[value_start..end].to_owned();
                i = (end + 1).min(bytes.len());
            } else {
                let value_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                value = tag[value_start..i].to_owned();
            }
        }
        if attr.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
}

/// Every `id` already on the page, so a new one never collides.
fn existing_ids(html: &str) -> Vec<String> {
    let bytes = html.as_bytes();
    let mut ids = Vec::new();
    let mut at = 0;
    while let Some(offset) = bytes[at..].iter().position(|&b| b == b'<') {
        let start = at + offset;
        let Some(end) = tag_end(bytes, start) else { break };
        let tag = &html[start..end];
        if tag.len() > 2 && tag.as_bytes()[1].is_ascii_alphabetic() {
            if let Some(id) = attribute(tag, "id") {
                ids.push(id);
            }
        }
        at = end;
    }
    ids
}

/// The heading's inner HTML with every tag removed and whitespace collapsed.
/// What is left is text as the page wrote it (entities intact) with no `<`,
/// which is safe to put inside our own `<a>`.
pub fn heading_text(inner: &str) -> String {
    let mut text = String::with_capacity(inner.len());
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            other => text.push(other),
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace('<', "&lt;")
}

/// "What a drawing is for" into "what-a-drawing-is-for".
pub fn slugify(text: &str) -> String {
    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&amp;", " and ")
        .replace("&lt;", " ")
        .replace("&gt;", " ")
        .replace("&quot;", "")
        .replace("&#39;", "")
        .replace("&#x27;", "");
    let mut slug = String::new();
    for c in decoded.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if (c.is_whitespace() || c == '-' || c == '_') && !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        // Everything else, apostrophes and punctuation included, is dropped.
    }
    let slug: String = slug.trim_end_matches('-').chars().take(MAX_SLUG).collect();
    let slug = slug.trim_end_matches('-').to_owned();
    if slug.is_empty() {
        "section".to_owned()
    } else if slug.starts_with(|c: char| c.is_ascii_digit()) {
        format!("s-{slug}")
    } else {
        slug
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>T</title></head><body>\
<nav><h2>Menu</h2></nav><main><h1>Guide</h1><h2>What a drawing is for</h2><p>a</p>\
<h3 class=\"x\">Scale &amp; paper </h3><h2 id=\"keep\">Mistakes</h2><h4>Deep</h4>\
<h2>What a drawing is for</h2><h2><a href=\"/x\">Linked</a></h2>\
<script>var s='<h2>no</h2>'</script></main></body></html>";

    #[test]
    fn adds_ids_links_style_and_script() {
        let html = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(html.contains("<h2 id=\"what-a-drawing-is-for\">What a drawing is for<a class=\"stride-ha\" href=\"#what-a-drawing-is-for\" aria-label=\"Link to section: What a drawing is for\"><span aria-hidden=\"true\">#</span></a></h2>"), "{html}");
        assert!(html.contains("<h3 id=\"scale-and-paper\" class=\"x\">Scale &amp; paper<a"), "{html}");
        assert!(html.contains("<h2 id=\"keep\">Mistakes<a class=\"stride-ha\" href=\"#keep\""));
        assert!(html.contains("id=\"what-a-drawing-is-for-2\""));
        assert!(html.contains("<h4>Deep</h4>"));
        assert!(html.contains("<nav><h2>Menu</h2></nav>"));
        assert!(html.contains("<h2><a href=\"/x\">Linked</a></h2>"));
        assert!(html.contains("var s='<h2>no</h2>'"));
        assert!(html.contains("</style></head>"));
        assert!(html.contains("data-text=\"Link copied\"></div><script>"));
        assert!(html.contains("</script></body></html>"));
    }

    #[test]
    fn is_idempotent_and_respects_settings() {
        let once = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(build(&once, "guide", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "guide", &off).is_none());
        let skip = Settings { exclude: vec!["guide".into()], ..Settings::default() };
        assert!(build(PAGE, "/guide/", &skip).is_none());
        let only_h2 = Settings { depth: 2, copy: false, ..Settings::default() };
        let html = build(PAGE, "guide", &only_h2).unwrap();
        assert!(html.contains("<h3 class=\"x\">Scale &amp; paper </h3>"));
        assert!(!html.contains("stride-ha-toast\" role"));
        let deep = Settings { depth: 4, before: true, symbol: Symbol::Section, ..Settings::default() };
        let html = build(PAGE, "guide", &deep).unwrap();
        assert!(html.contains("<h4 id=\"deep\"><a class=\"stride-ha stride-ha-before\" href=\"#deep\""), "{html}");
        assert!(html.contains("\u{a7}</span>"));
    }

    #[test]
    fn escapes_what_it_writes() {
        let page = "<html><head></head><body><h2>Say \"hi\" &lt;now&gt;</h2></body></html>";
        let s = Settings { copied_text: "Copied <b>\"".into(), ..Settings::default() };
        let html = build(page, "x", &s).unwrap();
        assert!(html.contains("aria-label=\"Link to section: Say &quot;hi&quot; &lt;now&gt;\""), "{html}");
        assert!(html.contains("data-text=\"Copied &lt;b&gt;&quot;\""));
        assert!(html.contains("id=\"say-hi-now\""));
    }

    #[test]
    fn refuses_fragments_and_empty_pages() {
        assert!(build("<h2>no body</h2>", "x", &Settings::default()).is_none());
        let none = "<html><head></head><body><main><p>x</p></main></body></html>";
        assert!(build(none, "x", &Settings::default()).is_none());
    }

    #[test]
    fn slugs_match_table_of_contents() {
        assert_eq!(slugify("What a drawing is for"), "what-a-drawing-is-for");
        assert_eq!(slugify("2024 plans"), "s-2024-plans");
        assert_eq!(slugify("Café &amp; bar"), "café-and-bar");
        assert_eq!(slugify("!!!"), "section");
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            depth: 4,
            symbol: Symbol::Link,
            before: true,
            color: "#0ea5e9".into(),
            copy: false,
            copied_text: "Link gekopieerd".into(),
            exclude: vec!["home".into(), "contact".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        let mut bad = Map::new();
        bad.insert("color".into(), "url(x)".into());
        assert!(Settings::from_values(&bad).is_err());
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
    }
}
