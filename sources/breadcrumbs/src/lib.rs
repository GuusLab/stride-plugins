//! Breadcrumbs: a Stride plugin.
//!
//! It puts a trail like "Home › Blog › This post" above the content of every
//! page but the home page, built from the page's slug, and adds matching
//! BreadcrumbList JSON-LD so search engines can show the trail in results.
//!
//! Permissions, both optional:
//! - `storage` keeps the settings. Refused, the plugin runs with the defaults.
//! - `read-pages` looks up the titles of the pages above this one ("blog" is
//!   called "Journal") and whether they are published, so only real pages are
//!   linked. Refused, parent crumbs are named after their slug ("blog" reads
//!   "Blog") and shown as plain text, because a link to a page that may not
//!   exist is worse than no link.
//!
//! No script, no fonts, no third-party requests: about 1 KB of inline CSS.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
    serde_json,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-bc";
/// Teal 700: the store accent darkened until small text on white passes
/// WCAG AA (5.5:1). The accent itself, #0d9488, only reaches 3.7:1.
const DEFAULT_COLOR: &str = "#0f766e";
const DEFAULT_HOME: &str = "Home";

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
    if !applies(&page.html, &page.slug, &settings) {
        return Ok(Json(PageRendered { html: page.html }));
    }
    let titles = published_titles(&page.site_id);
    let origin = if settings.structured_data { site_origin(&page.site_id) } else { None };
    let html = build(&page.html, &page.slug, &settings, titles.as_ref(), origin.as_deref())
        .unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

/// Slug to title for every published page, or `None` when the host would not
/// say (`read-pages` refused, or anything else going wrong).
fn published_titles(site_id: &str) -> Option<Vec<(String, String)>> {
    let mut input = Map::new();
    input.insert("siteId".into(), site_id.into());
    input.insert("kind".into(), "page".into());
    input.insert("status".into(), "published".into());
    match stride_pdk::action::<_, JsonValue>("documents.list", &JsonValue::Object(input)) {
        Ok(output) => Some(
            output
                .get("documents")
                .and_then(JsonValue::as_array)
                .map(|docs| {
                    docs.iter()
                        .filter_map(|doc| {
                            let slug = doc.get("slug")?.as_str()?.trim_matches('/');
                            let title = doc.get("title")?.as_str()?.trim();
                            Some((slug.to_owned(), title.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Err(error) if error.is_permission_denied() => None,
        Err(error) => {
            stride_pdk::log("info", &format!("page titles unavailable: {error}"));
            None
        }
    }
}

/// `https://example.com`, from the site's primary domain, so the structured
/// data can carry absolute URLs. `None` while the site has no domain, or when
/// the host would not say.
fn site_origin(site_id: &str) -> Option<String> {
    let mut input = Map::new();
    input.insert("siteId".into(), site_id.into());
    let site = stride_pdk::action::<_, JsonValue>("sites.get", &JsonValue::Object(input)).ok()?;
    origin_of(site.get("primaryDomain")?.as_str()?)
}

/// A domain or origin as `scheme://host[:port]`, or `None` when it does not
/// look like one. Only these characters reach the page.
pub fn origin_of(domain: &str) -> Option<String> {
    let domain = domain.trim().trim_end_matches('/');
    let (scheme, host) = match domain.split_once("://") {
        Some((scheme, host)) if scheme == "https" || scheme == "http" => (scheme, host),
        Some(_) => return None,
        None => ("https", domain),
    };
    let ok = !host.is_empty()
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b));
    ok.then(|| format!("{scheme}://{}", host.to_ascii_lowercase()))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_breadcrumbs(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                     It still shows breadcrumbs with the defaults shown here."
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
                "Saved. Breadcrumbs are off. Publish the site again to remove them from \
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
pub enum Separator {
    Chevron,
    Slash,
    Arrow,
    Dot,
}

impl Separator {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "chevron" => Some(Separator::Chevron),
            "slash" => Some(Separator::Slash),
            "arrow" => Some(Separator::Arrow),
            "dot" => Some(Separator::Dot),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Separator::Chevron => "chevron",
            Separator::Slash => "slash",
            Separator::Arrow => "arrow",
            Separator::Dot => "dot",
        }
    }
    /// As a CSS string escape, so the style block stays ASCII.
    fn css(self) -> &'static str {
        match self {
            Separator::Chevron => "\\203A",
            Separator::Slash => "/",
            Separator::Arrow => "\\2192",
            Separator::Dot => "\\00B7",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Empty for "Home".
    pub home_label: String,
    pub separator: Separator,
    pub pills: bool,
    /// `#rrggbb`, or empty for the default teal.
    pub color: String,
    pub show_current: bool,
    pub structured_data: bool,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            home_label: String::new(),
            separator: Separator::Chevron,
            pills: false,
            color: String::new(),
            show_current: true,
            structured_data: true,
            exclude: Vec::new(),
        }
    }
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
            home_label: s("homeLabel").map(clean_label).unwrap_or_default(),
            separator: s("separator").and_then(Separator::parse).unwrap_or(d.separator),
            pills: s("style") == Some("pills"),
            color: s("color").and_then(valid_color).unwrap_or_default(),
            show_current: b("showCurrent").unwrap_or(d.show_current),
            structured_data: b("structuredData").unwrap_or(d.structured_data),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("homeLabel".into(), self.home_label.clone().into());
        map.insert("separator".into(), self.separator.name().into());
        map.insert("style".into(), self.style().into());
        map.insert("color".into(), self.color.clone().into());
        map.insert("showCurrent".into(), self.show_current.into());
        map.insert("structuredData".into(), self.structured_data.into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("home-label".into(), self.home_label.clone().into());
        values.insert("separator".into(), self.separator.name().into());
        values.insert("style".into(), self.style().into());
        if !self.color.is_empty() {
            values.insert("color".into(), self.color.clone().into());
        }
        values.insert("show-current".into(), self.show_current.into());
        values.insert("structured-data".into(), self.structured_data.into());
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
                "The link colour has to look like #0f766e. Nothing was saved.".to_owned()
            })?,
        };
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            home_label: s("home-label").map(clean_label).unwrap_or_default(),
            separator: s("separator").and_then(Separator::parse).unwrap_or(d.separator),
            pills: s("style") == Some("pills"),
            color,
            show_current: b("show-current").unwrap_or(d.show_current),
            structured_data: b("structured-data").unwrap_or(d.structured_data),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn style(&self) -> &'static str {
        if self.pills { "pills" } else { "plain" }
    }

    fn effective_color(&self) -> &str {
        if self.color.is_empty() { DEFAULT_COLOR } else { &self.color }
    }

    fn effective_home(&self) -> &str {
        if self.home_label.is_empty() { DEFAULT_HOME } else { &self.home_label }
    }
}

/// One line, trimmed, at most 40 characters.
fn clean_label(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect()
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

/// "contact, thank-you /about" into ["contact", "thank-you", "about"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
}

// ------------------------------------------------------------ the plain work

/// One step of the trail. `path` is the slug it links to ("" is the home
/// page); `None` means plain text: the current page, or a parent that is not
/// a published page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub name: String,
    pub path: Option<String>,
}

/// Whether this page gets breadcrumbs at all. Cheap, so the hook can skip the
/// page lookup on pages it will leave alone.
pub fn applies(html: &str, slug: &str, settings: &Settings) -> bool {
    let slug = slug.trim_matches('/');
    settings.enabled
        && !slug.is_empty()
        && slug != "home"
        && !settings.exclude.iter().any(|s| s == slug)
        && !html.contains(MARKER)
}

/// The trail for `slug`, home first and the current page last. `titles` is
/// every published page's slug and title, when the host told us.
pub fn trail(
    html: &str,
    slug: &str,
    settings: &Settings,
    titles: Option<&Vec<(String, String)>>,
) -> Vec<Crumb> {
    let slug = slug.trim_matches('/');
    let lookup = |path: &str| {
        titles.and_then(|all| {
            all.iter().find(|(s, _)| s == path).map(|(_, title)| title.clone())
        })
    };
    let mut crumbs = vec![Crumb { name: settings.effective_home().to_owned(), path: Some(String::new()) }];
    let segments: Vec<&str> = slug.split('/').filter(|s| !s.is_empty()).collect();
    for i in 0..segments.len().saturating_sub(1) {
        let path = segments[..=i].join("/");
        match lookup(&path) {
            Some(title) if !title.is_empty() => crumbs.push(Crumb { name: title, path: Some(path) }),
            Some(_) => crumbs.push(Crumb { name: humanize(segments[i]), path: Some(path) }),
            None => crumbs.push(Crumb { name: humanize(segments[i]), path: None }),
        }
    }
    let current = lookup(slug)
        .filter(|t| !t.is_empty())
        .or_else(|| first_heading(html))
        .or_else(|| document_title(html))
        .unwrap_or_else(|| humanize(segments.last().copied().unwrap_or(slug)));
    crumbs.push(Crumb { name: current, path: None });
    crumbs
}

/// The page with the trail (and JSON-LD) added, or `None` to leave it alone:
/// disabled, excluded, the home page, already done, or not a whole document.
pub fn build(
    html: &str,
    slug: &str,
    settings: &Settings,
    titles: Option<&Vec<(String, String)>>,
    origin: Option<&str>,
) -> Option<String> {
    if !applies(html, slug, settings) {
        return None;
    }
    let bytes = html.as_bytes();
    let body = open_tag_end(bytes, b"<body", 0)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);
    // Above the page content: inside <main> when the theme has one, else
    // after the site header, else at the top of the body.
    let at = open_tag_end(bytes, b"<main", body)
        .or_else(|| find_ci(&bytes[body..], b"</header>").map(|rel| body + rel + 9))
        .filter(|&at| at <= body_close)
        .unwrap_or(body);

    let crumbs = trail(html, slug, settings, titles);
    let nav = nav(&crumbs, settings);
    let mut head = style(settings);
    // Leave structured data to whoever already wrote a BreadcrumbList.
    if settings.structured_data && !html.contains("BreadcrumbList") {
        let base = origin.map(|o| format!("{o}/")).unwrap_or_else(|| base_url(html, slug));
        head.push_str(&json_ld(&crumbs, &base));
    }

    let mut out = String::with_capacity(html.len() + head.len() + nav.len());
    match head_close {
        Some(close) => {
            out.push_str(&html[..close]);
            out.push_str(&head);
            out.push_str(&html[close..at]);
        }
        None => {
            out.push_str(&html[..at]);
            out.push_str(&head);
        }
    }
    out.push_str(&nav);
    out.push_str(&html[at..]);
    Some(out)
}

fn nav(crumbs: &[Crumb], settings: &Settings) -> String {
    let mut out = String::from(
        "<nav class=\"stride-bc\" aria-label=\"Breadcrumb\"><ol class=\"stride-bc-list\">",
    );
    let last = crumbs.len() - 1;
    for (i, crumb) in crumbs.iter().enumerate() {
        if i == last && !settings.show_current {
            break;
        }
        out.push_str("<li class=\"stride-bc-item\">");
        let name = escape(&crumb.name);
        match (&crumb.path, i == last) {
            (Some(path), false) => {
                out.push_str("<a class=\"stride-bc-link\" href=\"/");
                out.push_str(&escape(path));
                out.push_str("\">");
                out.push_str(&name);
                out.push_str("</a>");
            }
            (_, true) => {
                out.push_str("<span class=\"stride-bc-current\" aria-current=\"page\">");
                out.push_str(&name);
                out.push_str("</span>");
            }
            (None, false) => {
                out.push_str("<span class=\"stride-bc-text\">");
                out.push_str(&name);
                out.push_str("</span>");
            }
        }
        out.push_str("</li>");
    }
    out.push_str("</ol></nav>");
    out
}

/// The absolute address of the home page, from the page's canonical link, or
/// "/" when it has none (search engines resolve that against the page URL).
pub fn base_url(html: &str, slug: &str) -> String {
    let canonical = find_canonical(html).unwrap_or_default();
    let canonical = canonical.trim_end_matches('/');
    let slug = slug.trim_matches('/');
    match canonical.strip_suffix(slug).filter(|base| base.ends_with('/')) {
        Some(base) if base.starts_with("https://") || base.starts_with("http://") => base.to_owned(),
        _ => "/".to_owned(),
    }
}

fn find_canonical(html: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<link") {
        let at = from + rel;
        let end = at + html[at..].find('>')?;
        let tag = &html[at..end];
        if attr(tag, "rel").is_some_and(|rel| rel.eq_ignore_ascii_case("canonical")) {
            return attr(tag, "href").map(|href| decode(&href));
        }
        from = end;
    }
    None
}

/// A double- or single-quoted attribute's value in one tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before_ok = lower[..at].ends_with(|c: char| c.is_ascii_whitespace());
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value_at = tag.len() - rest.len() + 1;
        let value = tag[value_at..].trim_start();
        let quote = value.chars().next()?;
        if quote == '"' || quote == '\'' {
            let inner = &value[1..];
            return Some(inner[..inner.find(quote)?].to_owned());
        }
        return Some(value.split(|c: char| c.is_ascii_whitespace()).next()?.to_owned());
    }
    None
}

fn json_ld(crumbs: &[Crumb], base: &str) -> String {
    let last = crumbs.len() - 1;
    let items: Vec<JsonValue> = crumbs
        .iter()
        .enumerate()
        // Google wants a URL on every crumb but the last; a parent that is
        // not a page has none, so it is left out of the list.
        .filter(|(i, crumb)| *i == last || crumb.path.is_some())
        .enumerate()
        .map(|(position, (i, crumb))| {
            let mut item = Map::new();
            item.insert("@type".into(), "ListItem".into());
            item.insert("position".into(), (position as u64 + 1).into());
            item.insert("name".into(), crumb.name.clone().into());
            let path = if i == last { None } else { crumb.path.as_deref() };
            if let Some(path) = path {
                item.insert("item".into(), format!("{base}{path}").into());
            }
            JsonValue::Object(item)
        })
        .collect();
    let mut list = Map::new();
    list.insert("@context".into(), "https://schema.org".into());
    list.insert("@type".into(), "BreadcrumbList".into());
    list.insert("itemListElement".into(), JsonValue::Array(items));
    let json = serde_json::to_string(&JsonValue::Object(list)).unwrap_or_default();
    // Nothing in a script element may close it: escape every '<'.
    format!(
        "<script type=\"application/ld+json\">{}</script>",
        json.replace('<', "\\u003c")
    )
}

fn style(settings: &Settings) -> String {
    let color = settings.effective_color();
    let sep = settings.separator.css();
    let pills = if settings.pills {
        format!(
            ".stride-bc-link,.stride-bc-text,.stride-bc-current{{display:inline-block;\
padding:3px 11px;border-radius:999px;background:color-mix(in srgb,{color} 9%,transparent)}}\
.stride-bc-current{{background:color-mix(in srgb,currentColor 7%,transparent)}}\
.stride-bc-link:hover{{background:color-mix(in srgb,{color} 16%,transparent);text-decoration:none}}"
        )
    } else {
        String::new()
    };
    format!(
        "<style>.stride-bc{{box-sizing:border-box;width:100%;max-width:72rem;margin:0 auto;padding:16px 24px 0;\
font-size:.875rem;line-height:1.5}}\
.stride-bc-list{{display:flex;flex-wrap:wrap;align-items:center;gap:4px 8px;margin:0;padding:0;\
list-style:none}}\
.stride-bc-item{{display:inline-flex;align-items:center;gap:8px;min-width:0;margin:0}}\
.stride-bc-item+.stride-bc-item::before{{content:\"{sep}\";content:\"{sep}\"/\"\";opacity:.45}}\
.stride-bc-link{{color:{color};text-decoration:none;font-weight:500;border-radius:4px}}\
.stride-bc-link:hover{{text-decoration:underline;text-underline-offset:3px}}\
.stride-bc-link:focus-visible{{outline:2px solid {color};outline-offset:2px}}\
.stride-bc-text{{opacity:.8}}\
.stride-bc-current{{max-width:40ch;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}\
{pills}@media print{{.stride-bc{{display:none}}}}</style>"
    )
}

/// "summer-sale_2025" into "Summer sale 2025".
pub fn humanize(segment: &str) -> String {
    let words = segment.replace(['-', '_'], " ");
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => segment.to_owned(),
    }
}

/// The text of the first `<h1>`, tags stripped and entities decoded.
fn first_heading(html: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let mut from = 0;
    loop {
        let at = from + find_ci(&bytes[from..], b"<h1")?;
        match bytes.get(at + 3) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                let start = open_tag_end(bytes, b"<h1", at)?;
                let end = start + find_ci(&bytes[start..], b"</h1>")?;
                let text = text_of(&html[start..end]);
                return (!text.is_empty()).then_some(text);
            }
            _ => from = at + 3,
        }
    }
}

/// The `<title>`, minus a " | Site name" style suffix.
fn document_title(html: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let start = open_tag_end(bytes, b"<title", 0)?;
    let end = start + find_ci(&bytes[start..], b"</title>")?;
    let text = text_of(&html[start..end]);
    let text = [" | ", " \u{2013} ", " \u{2014} ", " - ", " \u{00b7} "]
        .iter()
        .find_map(|sep| text.split_once(sep).map(|(page, _)| page.trim().to_owned()))
        .unwrap_or(text);
    (!text.is_empty()).then_some(text)
}

/// Visible text of an HTML fragment, on one line, capped at 120 characters.
fn text_of(fragment: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let text = decode(&out);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > 120 {
        let cut: String = text.chars().take(119).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        text
    }
}

/// The handful of character references a renderer writes. Anything else is
/// left as it is, and escaped again on the way out like any other text.
fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The index just past the `>` of the first `<name…>` tag at or after `from`
/// (`name` includes the `<`), skipping longer names like `<mainly`.
fn open_tag_end(bytes: &[u8], name: &[u8], from: usize) -> Option<usize> {
    let mut from = from;
    while let Some(rel) = find_ci(&bytes[from..], name) {
        let at = from + rel;
        let after = at + name.len();
        match bytes.get(after) {
            Some(b'>') => return Some(after + 1),
            Some(c) if c.is_ascii_whitespace() || *c == b'/' => {
                let mut quote = 0u8;
                for (i, &c) in bytes[after..].iter().enumerate() {
                    match c {
                        b'"' | b'\'' if quote == 0 => quote = c,
                        c if c == quote => quote = 0,
                        b'>' if quote == 0 => return Some(after + i + 1),
                        _ => {}
                    }
                }
                return None;
            }
            _ => from = after,
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>Spring menu | Bistro</title>\
<link rel=\"canonical\" href=\"https://bistro.example/blog/spring-menu\"></head>\
<body><header>Nav</header><main class=\"m\"><h1>Our <em>spring</em> menu &amp; wines</h1>\
</main></body></html>";

    fn titles() -> Vec<(String, String)> {
        vec![("blog".into(), "Journal".into()), ("blog/spring-menu".into(), "Spring <menu>".into())]
    }

    #[test]
    fn trail_from_titles_and_fallbacks() {
        let s = Settings::default();
        let t = titles();
        let with = trail(PAGE, "blog/spring-menu", &s, Some(&t));
        assert_eq!(with[1], Crumb { name: "Journal".into(), path: Some("blog".into()) });
        assert_eq!(with[2].name, "Spring <menu>");
        let without = trail(PAGE, "blog/spring-menu", &s, None);
        assert_eq!(without[1], Crumb { name: "Blog".into(), path: None });
        assert_eq!(without[2].name, "Our spring menu & wines");
        let bare = "<html><body><p>x</p></body></html>";
        assert_eq!(trail(bare, "summer-sale_2025", &s, None)[1].name, "Summer sale 2025");
        let titled = "<html><head><title>About us \u{2013} Bistro</title></head><body></body></html>";
        assert_eq!(trail(titled, "about", &s, None)[1].name, "About us");
    }

    #[test]
    fn inserts_nav_in_main_with_escaped_names() {
        let t = titles();
        let html = build(PAGE, "blog/spring-menu", &Settings::default(), Some(&t), None).unwrap();
        assert!(html.contains("<main class=\"m\"><nav class=\"stride-bc\" aria-label=\"Breadcrumb\">"));
        assert!(html.contains("<a class=\"stride-bc-link\" href=\"/\">Home</a>"));
        assert!(html.contains("href=\"/blog\">Journal</a>"));
        assert!(html.contains("aria-current=\"page\">Spring &lt;menu&gt;</span>"));
        assert!(html.contains("</style><script type=\"application/ld+json\">"));
        assert!(html.contains("\"item\":\"https://bistro.example/blog\""));
        assert!(html.contains("Spring \\u003cmenu>"));
        assert!(!html.contains("<menu>"));
    }

    #[test]
    fn placement_fallbacks() {
        let s = Settings::default();
        let h = build("<html><body><header>x</header><p>y</p></body></html>", "a", &s, None, None).unwrap();
        assert!(h.contains("</header><style>"));
        assert!(h.contains("</script><nav class=\"stride-bc\""));
        let b = build("<html><head></head><body><p>y</p></body></html>", "a", &s, None, None).unwrap();
        assert!(b.contains("<body><nav"));
    }

    #[test]
    fn skips_home_excluded_disabled_done_and_fragments() {
        let s = Settings::default();
        assert!(build(PAGE, "home", &s, None, None).is_none());
        assert!(build(PAGE, "", &s, None, None).is_none());
        let once = build(PAGE, "blog/spring-menu", &s, None, None).unwrap();
        assert!(build(&once, "blog/spring-menu", &s, None, None).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "about", &off, None, None).is_none());
        let skip = Settings { exclude: vec!["about".into()], ..Settings::default() };
        assert!(build(PAGE, "about", &skip, None, None).is_none());
        assert!(build("<p>no body</p>", "x", &s, None, None).is_none());
    }

    #[test]
    fn options() {
        let s = Settings {
            home_label: "Start".into(),
            show_current: false,
            structured_data: false,
            pills: true,
            separator: Separator::Slash,
            ..Settings::default()
        };
        let html = build(PAGE, "blog/spring-menu", &s, None, None).unwrap();
        assert!(html.contains(">Start</a>"));
        assert!(!html.contains("aria-current"));
        assert!(!html.contains("ld+json"));
        assert!(html.contains("border-radius:999px"));
        let existing = PAGE.replace("</head>", "<script>\"BreadcrumbList\"</script></head>");
        let d = build(&existing, "blog/spring-menu", &Settings::default(), None, None).unwrap();
        assert_eq!(d.matches("BreadcrumbList").count(), 1);
    }

    #[test]
    fn origins() {
        assert_eq!(origin_of("Bistro.example"), Some("https://bistro.example".into()));
        assert_eq!(origin_of("http://localhost:8080/"), Some("http://localhost:8080".into()));
        assert_eq!(origin_of("javascript:alert(1)"), None);
        assert_eq!(origin_of("x.example/\"><b>"), None);
        let html = build(PAGE, "menu", &Settings::default(), None, Some("https://a.example")).unwrap();
        assert!(html.contains("\"item\":\"https://a.example/\""));
    }

    #[test]
    fn base_urls() {
        assert_eq!(base_url(PAGE, "blog/spring-menu"), "https://bistro.example/");
        assert_eq!(base_url("<html></html>", "a"), "/");
        let other = "<link href='https://x.example/other' rel=canonical>";
        assert_eq!(base_url(other, "a"), "/");
        let json = json_ld(&trail(PAGE, "blog/spring-menu", &Settings::default(), None), "/");
        // The parent without a page is left out, and positions stay 1..n.
        assert!(json.contains("\"name\":\"Our spring menu & wines\",\"position\":2"));
        assert!(!json.contains("Blog"));
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            home_label: "Start".into(),
            separator: Separator::Arrow,
            pills: true,
            color: "#0ea5e9".into(),
            show_current: false,
            structured_data: false,
            exclude: vec!["contact".into(), "thanks".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        let mut bad = Map::new();
        bad.insert("color".into(), "red;}".into());
        assert!(Settings::from_values(&bad).is_err());
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
        assert_eq!(valid_color("#ABC"), Some("#aabbcc".into()));
    }
}
