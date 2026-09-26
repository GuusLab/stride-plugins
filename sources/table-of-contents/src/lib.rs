//! Table of Contents: a Stride plugin.
//!
//! It reads the `h2` and `h3` headings inside a page's `<main>`, gives every
//! one of them an anchor id, and puts a linked table of contents in front of
//! the first of them: at the top of the article, inside the same column as the
//! text. With the sidebar style it sticks in the left margin on wide screens.
//! It works entirely from the HTML the host hands it, and needs no JavaScript
//! on the published page: links are plain `#fragment` links.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

/// Every setting lives in one JSON object under one key: one host call per
/// render instead of seven.
const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-toc";
const TITLE_ID: &str = "stride-toc-title";

const DEFAULT_TITLE: &str = "On this page";
const DEFAULT_MIN: u32 = 3;
const MIN_MIN: u32 = 1;
const MAX_MIN: u32 = 20;
const MAX_TITLE: usize = 60;
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
pub fn panel_table_of_contents(
    Json(request): Json<PanelRequest>,
) -> FnResult<Json<PanelResponse>> {
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
                     It still adds a table of contents with the defaults shown here."
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
                format!(
                    "Saved. Pages with at least {} headings get a table of contents. \
                     Publish the site again to update pages that are already live.",
                    submitted.min
                )
            } else {
                "Saved. The table of contents is off. Publish the site again to remove it \
                 from pages that are already live."
                    .to_owned()
            };
            Ok(Json(PanelResponse {
                values: submitted.to_values(),
                message,
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Include `h3` as well as `h2`.
    pub subsections: bool,
    pub min: u32,
    pub title: String,
    pub sidebar: bool,
    pub numbered: bool,
    /// `#rrggbb`, or empty for the text colour of the page.
    pub accent: String,
    /// Slugs that never get a table of contents.
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            subsections: true,
            min: DEFAULT_MIN,
            title: DEFAULT_TITLE.to_owned(),
            sidebar: false,
            numbered: false,
            accent: String::new(),
            exclude: Vec::new(),
        }
    }
}

impl Settings {
    /// Whatever is stored, never fail: a field that is missing or the wrong
    /// shape falls back to its default.
    pub fn from_json(value: &JsonValue) -> Self {
        let d = Settings::default();
        let get = |key: &str| value.get(key);
        Settings {
            enabled: get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            subsections: get("levels")
                .and_then(JsonValue::as_str)
                .map(|levels| levels != "h2")
                .unwrap_or(d.subsections),
            min: get("min")
                .and_then(JsonValue::as_u64)
                .filter(|n| (u64::from(MIN_MIN)..=u64::from(MAX_MIN)).contains(n))
                .map(|n| n as u32)
                .unwrap_or(d.min),
            title: get("title")
                .and_then(JsonValue::as_str)
                .map(clean_title)
                .filter(|title| !title.is_empty())
                .unwrap_or(d.title),
            sidebar: get("style").and_then(JsonValue::as_str) == Some("sidebar"),
            numbered: get("numbered").and_then(JsonValue::as_bool).unwrap_or(d.numbered),
            accent: get("accent")
                .and_then(JsonValue::as_str)
                .and_then(valid_color)
                .unwrap_or_default(),
            exclude: get("exclude")
                .and_then(JsonValue::as_str)
                .map(parse_slugs)
                .unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("levels".into(), self.levels().into());
        map.insert("min".into(), self.min.into());
        map.insert("title".into(), self.title.clone().into());
        map.insert("style".into(), self.style().into());
        map.insert("numbered".into(), self.numbered.into());
        map.insert("accent".into(), self.accent.clone().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("levels".into(), self.levels().into());
        values.insert("min-headings".into(), self.min.into());
        values.insert("title".into(), self.title.clone().into());
        values.insert("style".into(), self.style().into());
        values.insert("numbered".into(), self.numbered.into());
        if !self.accent.is_empty() {
            values.insert("accent".into(), self.accent.clone().into());
        }
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let min = values
            .get("min-headings")
            .and_then(JsonValue::as_i64)
            .unwrap_or(i64::from(d.min));
        if min < i64::from(MIN_MIN) || min > i64::from(MAX_MIN) {
            return Err(format!(
                "The minimum number of headings has to be between {MIN_MIN} and {MAX_MIN}. \
                 Nothing was saved."
            ));
        }
        let accent = match values.get("accent").and_then(JsonValue::as_str).map(str::trim) {
            None | Some("") => String::new(),
            Some(color) => valid_color(color).ok_or_else(|| {
                "The accent colour has to look like #7c3aed. Nothing was saved.".to_owned()
            })?,
        };
        let title = values
            .get("title")
            .and_then(JsonValue::as_str)
            .map(clean_title)
            .filter(|title| !title.is_empty())
            .unwrap_or(d.title);
        Ok(Settings {
            enabled: values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            subsections: values.get("levels").and_then(JsonValue::as_str) != Some("h2"),
            min: min as u32,
            title,
            sidebar: values.get("style").and_then(JsonValue::as_str) == Some("sidebar"),
            numbered: values.get("numbered").and_then(JsonValue::as_bool).unwrap_or(d.numbered),
            accent,
            exclude: values
                .get("exclude")
                .and_then(JsonValue::as_str)
                .map(parse_slugs)
                .unwrap_or_default(),
        })
    }

    fn levels(&self) -> &'static str {
        if self.subsections { "h2-h3" } else { "h2" }
    }

    fn style(&self) -> &'static str {
        if self.sidebar { "sidebar" } else { "box" }
    }
}

fn clean_title(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect()
}

/// `#rgb` or `#rrggbb`, lowercased. Anything else is refused, which is what
/// keeps the value safe to put into CSS.
pub fn valid_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if (hex.len() == 3 || hex.len() == 6) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_ascii_lowercase()))
    } else {
        None
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

// ------------------------------------------------------------ the plain work

/// One heading that goes into the table.
#[derive(Debug, Clone, PartialEq)]
struct Heading {
    level: u8,
    /// The id, as it goes into `href="#…"` (attribute-safe).
    id: String,
    /// The link text: the heading's own text, tags stripped, still HTML.
    text: String,
    /// Where the heading's `<` is, in the original HTML.
    start: usize,
    /// Where ` id="…"` has to go when the heading has none.
    insert_id_at: Option<usize>,
}

/// The page with a table of contents, or `None` to leave it exactly as it was.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let slug = slug.trim_matches('/');
    if settings.exclude.iter().any(|excluded| excluded == slug) {
        return None;
    }
    let (from, to) = content_region(html)?;
    let mut used = existing_ids(html);
    if used.iter().any(|id| id == TITLE_ID) {
        return None;
    }
    let headings = find_headings(html, from, to, settings.subsections, &mut used);
    if headings.is_empty() || headings.len() < settings.min as usize {
        return None;
    }
    let nav = render_nav(&headings, settings);
    let mut inserts: Vec<(usize, String)> = headings
        .iter()
        .filter_map(|h| h.insert_id_at.map(|at| (at, format!(" id=\"{}\"", h.id))))
        .collect();
    inserts.push((headings[0].start, nav));
    if let Some(head) = find_ci(html.as_bytes(), b"</head>") {
        inserts.push((head, style(settings)));
    } else {
        // No head: put the style in front of the nav, which is still valid
        // enough for a fragment and keeps the table legible.
        inserts.push((headings[0].start, style(settings)));
    }
    // Back to front, so earlier offsets stay true. At the same offset the
    // style was pushed after the nav, so sorting stably by descending offset
    // puts the style first.
    inserts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = html.to_owned();
    for (at, text) in inserts {
        out.insert_str(at, &text);
    }
    Some(out)
}

/// The byte range to look for headings in: the inside of `<main>`, or the
/// inside of `<body>` when there is no main. `None` for a fragment.
fn content_region(html: &str) -> Option<(usize, usize)> {
    let bytes = html.as_bytes();
    if let Some(main) = find_tag(bytes, 0, b"main") {
        let open_end = tag_end(bytes, main)?;
        let close = find_ci(&bytes[open_end..], b"</main").map_or(bytes.len(), |o| open_end + o);
        return Some((open_end, close));
    }
    let body = find_tag(bytes, 0, b"body")?;
    let open_end = tag_end(bytes, body)?;
    let close = find_ci(&bytes[open_end..], b"</body").map_or(bytes.len(), |o| open_end + o);
    Some((open_end, close))
}

fn find_headings(
    html: &str,
    from: usize,
    to: usize,
    subsections: bool,
    used: &mut Vec<String>,
) -> Vec<Heading> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        let Some(offset) = bytes[at..to].iter().position(|&b| b == b'<') else { break };
        let start = at + offset;
        let level = match heading_level(&bytes[start..to]) {
            Some(2) => 2,
            Some(3) if subsections => 3,
            Some(_) | None => {
                // Skip what is never visible prose.
                at = skip_raw_text(bytes, start, to);
                continue;
            }
        };
        let Some(open_end) = tag_end(bytes, start) else { break };
        let close_tag: &[u8] = if level == 2 { b"</h2" } else { b"</h3" };
        let Some(close) = find_ci(&bytes[open_end..to], close_tag).map(|o| open_end + o) else {
            break;
        };
        let text = heading_text(&html[open_end..close]);
        at = close;
        if text.is_empty() {
            continue;
        }
        let open_tag = &html[start..open_end];
        let (id, insert_id_at) = match attribute(open_tag, "id") {
            Some(id) if !id.is_empty() => (id.replace('"', "&quot;"), None),
            _ => {
                let id = unique(slugify(&text), used);
                // Right after "<h2", before any other attribute.
                (id, Some(start + 3))
            }
        };
        out.push(Heading { level, id, text, start, insert_id_at });
    }
    out
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

fn unique(base: String, used: &mut Vec<String>) -> String {
    let mut candidate = base.clone();
    let mut n = 2;
    while used.iter().any(|id| id == &candidate) || candidate == TITLE_ID {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    used.push(candidate.clone());
    candidate
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

fn render_nav(headings: &[Heading], settings: &Settings) -> String {
    let variant = if settings.sidebar { "side" } else { "box" };
    let mut out = format!(
        "<nav class=\"{MARKER} {MARKER}--{variant}\" aria-labelledby=\"{TITLE_ID}\">\
         <p class=\"{MARKER}__title\" id=\"{TITLE_ID}\">{}</p><ol>",
        escape(&settings.title)
    );
    let mut open_sub = false;
    let mut first = true;
    for heading in headings {
        let link = format!("<a href=\"#{}\">{}</a>", heading.id, heading.text);
        if heading.level == 3 && !first {
            if !open_sub {
                out.push_str("<ol>");
                open_sub = true;
            } else {
                out.push_str("</li>");
            }
            out.push_str("<li>");
            out.push_str(&link);
        } else {
            if open_sub {
                out.push_str("</li></ol>");
                open_sub = false;
            }
            if !first {
                out.push_str("</li>");
            }
            out.push_str("<li>");
            out.push_str(&link);
        }
        first = false;
    }
    if open_sub {
        out.push_str("</li></ol>");
    }
    out.push_str("</li></ol></nav>");
    out
}

fn style(settings: &Settings) -> String {
    let accent = if settings.accent.is_empty() {
        "currentColor"
    } else {
        settings.accent.as_str()
    };
    let list = if settings.numbered {
        ".stride-toc ol{list-style:decimal;padding-left:1.6em}.stride-toc ol ol{list-style:lower-alpha}\
.stride-toc li::marker{color:var(--toc-a);font-variant-numeric:tabular-nums}"
    } else {
        ".stride-toc ol{list-style:none;padding-left:0}.stride-toc ol ol{padding-left:1em}"
    };
    let side_pad = if settings.numbered { "2.4em" } else { "1rem" };
    format!(
        "<style>\
.stride-toc{{--toc-a:{accent};margin:0 0 .5rem;padding:1rem 1.25rem;font-size:.95em;line-height:1.45;\
border:1px solid color-mix(in srgb,currentColor 14%,transparent);border-left:3px solid var(--toc-a);\
border-radius:6px;background:color-mix(in srgb,currentColor 3%,transparent)}}\
.stride-toc__title{{margin:0 0 .5rem;font-size:.78em;font-weight:700;letter-spacing:.08em;text-transform:uppercase;color:var(--toc-a);opacity:.75}}\
.stride-toc ol{{margin:0}}.stride-toc li{{margin:.3em 0}}{list}\
.stride-toc a{{color:inherit;text-decoration:none;border-bottom:1px solid transparent}}\
.stride-toc a:hover{{color:var(--toc-a);border-bottom-color:currentColor}}\
.stride-toc a:focus-visible{{outline:2px solid var(--toc-a);outline-offset:2px;border-radius:2px}}\
h2[id],h3[id]{{scroll-margin-top:1.5rem}}\
@media (prefers-reduced-motion:no-preference){{html{{scroll-behavior:smooth}}}}\
@media (min-width:1300px){{.stride-toc--side{{position:sticky;top:2rem;align-self:flex-start;z-index:1;\
width:15rem;height:0;margin:0 0 0 -18rem;padding:0;border:0;border-radius:0;background:none;overflow:visible}}\
.stride-toc--side>ol{{padding-left:{side_pad};border-left:2px solid color-mix(in srgb,var(--toc-a) 35%,transparent)}}\
.stride-toc--side>.stride-toc__title{{padding-left:calc(1rem + 2px)}}}}\
@media print{{.stride-toc--side{{display:none}}}}\
</style>"
    )
}

/// `needle` must be ASCII, which keeps the answer on a char boundary.
fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>T</title></head><body>\
<header><nav><h2>Site</h2></nav></header><main><section><div class=\"c\">\
<h1 class=\"t\">Guide</h1><p>Intro.</p><h2 class=\"_s14\">What a drawing is for</h2><p>a</p>\
<h3>Scale &amp; paper</h3><p>b</p><h2 class=\"_s14\" id=\"keep\">Mistakes</h2><h3>One</h3>\
<h2>What a drawing is for</h2></div></section></main><footer><h2>Footer</h2></footer></body></html>";

    fn out(html: &str, settings: &Settings) -> String {
        build(html, "guide", settings).expect("a table of contents")
    }

    #[test]
    fn builds_a_nested_table_before_the_first_heading() {
        let html = out(PAGE, &Settings::default());
        assert!(html.contains("<p>Intro.</p><nav class=\"stride-toc stride-toc--box\""), "{html}");
        assert!(html.contains(
            "<ol><li><a href=\"#what-a-drawing-is-for\">What a drawing is for</a>\
<ol><li><a href=\"#scale-and-paper\">Scale &amp; paper</a></li></ol></li>\
<li><a href=\"#keep\">Mistakes</a><ol><li><a href=\"#one\">One</a></li></ol></li>\
<li><a href=\"#what-a-drawing-is-for-2\">What a drawing is for</a></li></ol></nav>"
        ), "{html}");
        assert!(html.contains("<h2 id=\"what-a-drawing-is-for\" class=\"_s14\">"));
        assert!(html.contains("<h2 class=\"_s14\" id=\"keep\">"));
        assert!(html.contains("<h2 id=\"what-a-drawing-is-for-2\">"));
        assert!(html.contains("<style>.stride-toc{") && html.contains("</style></head>"));
        // Header and footer headings are not the page's content.
        assert!(!html.contains("#site") && !html.contains("#footer"));
    }

    #[test]
    fn is_idempotent() {
        let once = out(PAGE, &Settings::default());
        assert_eq!(build(&once, "guide", &Settings::default()), None);
    }

    #[test]
    fn respects_the_minimum_and_levels() {
        let s = Settings { min: 6, ..Settings::default() };
        assert_eq!(build(PAGE, "guide", &s), None);
        let s = Settings { subsections: false, min: 3, ..Settings::default() };
        let html = out(PAGE, &s);
        assert!(!html.contains("#one") && !html.contains("<h3 id="));
    }

    #[test]
    fn skips_excluded_and_disabled_pages() {
        let s = Settings { exclude: parse_slugs("home, /guide/"), ..Settings::default() };
        assert_eq!(build(PAGE, "guide", &s), None);
        let s = Settings { enabled: false, ..Settings::default() };
        assert_eq!(build(PAGE, "guide", &s), None);
    }

    #[test]
    fn falls_back_to_body_and_refuses_fragments() {
        let page = "<html><body><h2>A</h2><h2>B</h2><h2>C</h2></body></html>";
        let html = out(page, &Settings::default());
        assert!(html.starts_with("<html><body><style>") || html.contains("<body><style>"), "{html}");
        assert_eq!(build("<h2>A</h2><h2>B</h2><h2>C</h2>", "x", &Settings::default()), None);
    }

    #[test]
    fn escapes_the_title_and_strips_inner_tags() {
        let s = Settings { title: "<b>\"x\"</b>".into(), ..Settings::default() };
        let page = "<html><head></head><body><main><h2><a href=\"/x\">Linked</a> <em>it</em></h2>\
<h2>B</h2><h2>C</h2></main></body></html>";
        let html = out(page, &s);
        assert!(html.contains("&lt;b&gt;&quot;x&quot;&lt;/b&gt;"));
        assert!(html.contains("<a href=\"#linked-it\">Linked it</a>"), "{html}");
    }

    #[test]
    fn avoids_existing_ids() {
        let page = "<html><head></head><body><div id=\"a\"></div><main><h2>A</h2><h2>A</h2><h2>2024</h2>\
<h2>!!!</h2></main></body></html>";
        let html = out(page, &Settings::default());
        assert!(html.contains("<h2 id=\"a-2\">A</h2><h2 id=\"a-3\">A</h2>"), "{html}");
        assert!(html.contains("id=\"s-2024\"") && html.contains("id=\"section\""));
    }

    #[test]
    fn sidebar_and_accent() {
        let s = Settings { sidebar: true, accent: "#7c3aed".into(), ..Settings::default() };
        let html = out(PAGE, &s);
        assert!(html.contains("stride-toc--side") && html.contains("--toc-a:#7c3aed"));
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings { sidebar: true, numbered: true, min: 4, ..Settings::default() };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        let stored: JsonValue = stride_pdk::serde_json::json!({"min": "x", "accent": "red;}</style>"});
        assert_eq!(Settings::from_json(&stored), Settings::default());
        let mut values = Map::new();
        values.insert("min-headings".into(), 0.into());
        assert!(Settings::from_values(&values).is_err());
        let mut values = Map::new();
        values.insert("accent".into(), "url(x)".into());
        assert!(Settings::from_values(&values).is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(valid_color("#ABC"), Some("#abc".into()));
        assert_eq!(valid_color("#12345g"), None);
        assert_eq!(valid_color("red"), None);
    }
}
