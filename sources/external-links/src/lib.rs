//! External Links: links to other sites open in a new tab, carry the right
//! `rel` for safety and search engines, and get a small arrow so readers know
//! they are leaving.
//!
//! Everything happens in the HTML at publish time: no JavaScript. The site's
//! own address is remembered from `on_publish` (which reports the page's full
//! address once the site has a domain), read from the page's canonical link or
//! `og:url` when it has one, and the panel can name more domains that count as
//! "this site".

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Published, kv};
use util::*;

const FIELDS: [&str; 6] = ["enabled", "new-tab", "nofollow", "icon", "own-domains", "skip-domains"];
const STYLE_MARKER: &str = "id=\"stride-external-links\"";
/// Where the host learned from `on_publish` is kept. Not a panel field.
const SITE_HOST: &str = "site-host";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    new_tab: bool,
    /// `none`, `nofollow` or `sponsored`.
    nofollow: String,
    icon: bool,
    own_domains: Vec<String>,
    skip_domains: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            new_tab: true,
            nofollow: "none".into(),
            icon: true,
            own_domains: Vec::new(),
            skip_domains: Vec::new(),
        }
    }
}

fn domain_list(text: &str) -> Vec<String> {
    text.split([',', '\n', ' '])
        .filter_map(|d| host_of(d).or_else(|| Some(d.trim().trim_start_matches("*.").to_ascii_lowercase())))
        .map(|d| d.trim_start_matches("www.").to_owned())
        .filter(|d| !d.is_empty() && d.contains('.'))
        .collect()
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            new_tab: flag(values, "new-tab", d.new_tab),
            nofollow: match text(values, "nofollow").as_str() {
                "nofollow" => "nofollow".into(),
                "sponsored" => "sponsored".into(),
                _ => d.nofollow,
            },
            icon: flag(values, "icon", d.icon),
            own_domains: domain_list(&text(values, "own-domains")),
            skip_domains: domain_list(&text(values, "skip-domains")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("new-tab".into(), self.new_tab.into());
        values.insert("nofollow".into(), self.nofollow.clone().into());
        values.insert("icon".into(), self.icon.into());
        values.insert("own-domains".into(), self.own_domains.join(", ").into());
        values.insert("skip-domains".into(), self.skip_domains.join(", ").into());
        values
    }
}

fn load_values() -> Result<Map<String, JsonValue>, stride_pdk::HostError> {
    let mut values = Map::new();
    for name in FIELDS {
        if let Some(value) = kv::get::<JsonValue>(name)? {
            values.insert(name.to_owned(), value);
        }
    }
    Ok(values)
}

#[plugin_fn]
pub fn on_publish(Json(event): Json<Published>) -> FnResult<Json<JsonValue>> {
    // `url` is absolute once the site has a primary domain. Remember that host,
    // so links to the site's own address are not treated as leaving it.
    if let Some(host) = host_of(&event.url) {
        if kv::get::<String>(SITE_HOST).ok().flatten().as_deref() != Some(host.as_str()) {
            let _ = kv::set(SITE_HOST, &host);
        }
    }
    Ok(Json(JsonValue::Object(Map::new())))
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let config = match load_values() {
        Ok(values) => {
            let mut config = Config::from_values(&values);
            if let Ok(Some(host)) = kv::get::<String>(SITE_HOST) {
                config.own_domains.push(host);
            }
            config
        }
        Err(error) if error.is_permission_denied() => Config::default(),
        Err(error) => {
            stride_pdk::log("warn", &format!("external links left as they were: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &config) }))
}

/// `https://Www.Example.com:443/a` -> `example.com`.
fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    let host = host.trim_start_matches("www.");
    (!host.is_empty()).then(|| host.to_owned())
}

/// `shop.example.com` belongs to `example.com`, and to itself.
fn belongs(host: &str, domains: &[String]) -> bool {
    domains.iter().any(|d| host == d || host.ends_with(&format!(".{d}")))
}

/// The page's own host, from `<link rel="canonical">` or `og:url`.
fn own_host(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let head_end = lower.find("</head>").unwrap_or(lower.len());
    let mut at = 0;
    while let Some(start) = lower[at..head_end].find('<').map(|i| at + i) {
        let end = match lower[start..head_end].find('>') {
            Some(e) => start + e,
            None => break,
        };
        let tag = &html[start..=end];
        let attrs = parse_attrs(tag);
        let get = |n: &str| attrs.iter().find(|a| a.0 == n).map(|a| a.1.as_str());
        let is_canonical = tag.to_ascii_lowercase().starts_with("<link") && get("rel").is_some_and(|r| r.eq_ignore_ascii_case("canonical"));
        let is_og = tag.to_ascii_lowercase().starts_with("<meta") && get("property") == Some("og:url");
        if let Some(url) = if is_canonical { get("href") } else if is_og { get("content") } else { None } {
            if let Some(host) = host_of(url) {
                return Some(host);
            }
        }
        at = end + 1;
    }
    None
}

/// Attributes of one start tag as (lowercase name, raw value). Values are
/// returned as written; only the ones this plugin adds are ever re-escaped.
fn parse_attrs(tag: &str) -> Vec<(String, String)> {
    let inner = tag.trim_start_matches('<').trim_end_matches('>').trim_end_matches('/');
    let mut chars = inner.char_indices().peekable();
    // Skip the tag name.
    while chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
        chars.next();
    }
    let mut attrs = Vec::new();
    loop {
        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
            chars.next();
        }
        let Some(&(start, _)) = chars.peek() else { break };
        while chars.peek().is_some_and(|(_, c)| !c.is_whitespace() && *c != '=') {
            chars.next();
        }
        let end = chars.peek().map(|(i, _)| *i).unwrap_or(inner.len());
        let name = inner[start..end].to_ascii_lowercase();
        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek().is_some_and(|(_, c)| *c == '=') {
            chars.next();
            while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                chars.next();
            }
            match chars.peek().map(|(_, c)| *c) {
                Some(q @ ('"' | '\'')) => {
                    chars.next();
                    for (_, c) in chars.by_ref() {
                        if c == q {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }
        if name.is_empty() {
            chars.next();
            continue;
        }
        attrs.push((name, value));
    }
    attrs
}

/// Where the start tag that begins at `start` ends, honouring quotes.
fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in html[start..].char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(start + i),
            _ => {}
        }
    }
    None
}

const ICON: &str = "<svg class=\"sxl-icon\" aria-hidden=\"true\" viewBox=\"0 0 12 12\" width=\"0.7em\" height=\"0.7em\">\
<path d=\"M4 2h6v6M10 2 3 9\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

fn render(html: String, config: &Config) -> String {
    if !config.enabled || html.contains(STYLE_MARKER) {
        return html;
    }
    let mut own = config.own_domains.clone();
    own.extend(own_host(&html));
    let lower = html.to_ascii_lowercase();
    let body = lower.find("<body").unwrap_or(0);
    let raw = raw_text_ranges(&lower);

    let mut out = String::with_capacity(html.len() + 1024);
    let mut copied = 0;
    let mut changed = false;
    let mut at = body;
    while let Some(found) = lower[at..].find("<a").map(|i| at + i) {
        // Skip what is not a link: `<abbr>`, `<aside>`, and anything in a script or style.
        let next = lower[found + 2..].chars().next();
        if !matches!(next, Some(c) if c.is_whitespace() || c == '>') {
            at = found + 2;
            continue;
        }
        if let Some(&(_, end)) = raw.iter().find(|(s, e)| *s < found && found < *e) {
            at = end;
            continue;
        }
        let Some(end) = tag_end(&html, found) else { break };
        let tag = &html[found..=end];
        let attrs = parse_attrs(tag);
        let href = attrs.iter().find(|a| a.0 == "href").map(|a| a.1.as_str()).unwrap_or("");
        let external = host_of(href).filter(|h| !belongs(h, &own) && !belongs(h, &config.skip_domains));
        if external.is_none() {
            at = end + 1;
            continue;
        }
        let close = lower[end..].find("</a>").map(|i| end + i);
        out.push_str(&html[copied..found]);
        out.push_str(&rewrite(&attrs, config));
        copied = end + 1;
        if let Some(close) = close {
            let inner = &lower[end + 1..close];
            let new_tab = config.new_tab && !attrs.iter().any(|a| a.0 == "target");
            out.push_str(&html[copied..close]);
            if config.icon && !inner.contains("<img") && !inner.contains("<svg") && !strip_tags(inner).trim().is_empty() {
                out.push_str(ICON);
            }
            if new_tab {
                out.push_str("<span class=\"sxl-sr\"> (opens in a new tab)</span>");
            }
            copied = close;
            at = close;
        } else {
            at = end + 1;
        }
        changed = true;
    }
    if !changed {
        return html;
    }
    out.push_str(&html[copied..]);
    insert_in_head(
        &mut out,
        &format!(
            "<style {STYLE_MARKER}>.sxl-icon{{display:inline-block;margin-left:.2em;vertical-align:.05em;opacity:.75}}\
             .sxl-sr{{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;\
             clip:rect(0 0 0 0);white-space:nowrap;border:0}}</style>"
        ),
    );
    out
}

/// Byte ranges of `<script>`, `<style>`, `<textarea>` and `<template>`
/// elements, found once per page: a link-shaped string inside one is text.
fn raw_text_ranges(lower: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for name in ["script", "style", "textarea", "template"] {
        let (open, close) = (format!("<{name}"), format!("</{name}"));
        let mut at = 0;
        while let Some(start) = lower[at..].find(&open).map(|i| at + i) {
            let end = lower[start..].find(&close).map(|i| start + i).unwrap_or(lower.len());
            ranges.push((start, end));
            at = end.max(start + 1);
        }
    }
    ranges
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn rewrite(attrs: &[(String, String)], config: &Config) -> String {
    let has_target = attrs.iter().any(|a| a.0 == "target");
    let new_tab = config.new_tab && !has_target;
    let opens_elsewhere = new_tab || attrs.iter().any(|a| a.0 == "target" && a.1 == "_blank");
    let mut rel: Vec<String> = attrs
        .iter()
        .find(|a| a.0 == "rel")
        .map(|a| a.1.split_whitespace().map(str::to_ascii_lowercase).collect())
        .unwrap_or_default();
    let mut add = |token: &str| {
        if !rel.iter().any(|r| r == token) {
            rel.push(token.to_owned());
        }
    };
    if opens_elsewhere {
        add("noopener");
        add("noreferrer");
    }
    match config.nofollow.as_str() {
        "nofollow" => add("nofollow"),
        "sponsored" => {
            add("nofollow");
            add("sponsored");
        }
        _ => {}
    }
    add("external");

    let mut tag = String::from("<a");
    for (name, value) in attrs {
        if name == "rel" {
            continue;
        }
        tag.push(' ');
        tag.push_str(name);
        tag.push_str("=\"");
        // Re-quote a value as it was written; a bare `"` could not have been in it.
        tag.push_str(&value.replace('"', "&quot;"));
        tag.push('"');
    }
    if new_tab {
        tag.push_str(" target=\"_blank\"");
    }
    tag.push_str(" rel=\"");
    tag.push_str(&escape(&rel.join(" ")));
    tag.push_str("\">");
    tag
}

#[plugin_fn]
pub fn panel_external_links(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return Ok(Json(PanelResponse {
                        error: if error.is_permission_denied() {
                            "This plugin was not granted the storage permission, so it keeps the default settings.".to_owned()
                        } else {
                            error.to_string()
                        },
                        ..PanelResponse::default()
                    }));
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: "Saved. Publish your pages again to update their links.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> String {
        format!("<html><head><link rel=\"canonical\" href=\"https://www.atelier.test/journal\"></head><body>{body}</body></html>")
    }

    #[test]
    fn external_links_open_in_a_new_tab_with_an_arrow() {
        let out = render(page("<p><a href=\"https://example.org/x\">Read</a></p>"), &Config::default());
        assert!(out.contains("<a href=\"https://example.org/x\" target=\"_blank\" rel=\"noopener noreferrer external\">Read<svg class=\"sxl-icon\""));
        assert!(out.contains("(opens in a new tab)</span></a>"));
        assert!(out.find("<style id=\"stride-external-links\"").unwrap() < out.find("</head>").unwrap());
        assert_eq!(render(out.clone(), &Config::default()), out);
    }

    #[test]
    fn own_links_are_left_alone() {
        let body = "<a href=\"/contact\">c</a><a href=\"https://atelier.test/a\">a</a><a href=\"https://shop.atelier.test\">s</a><a href=\"#top\">t</a><a href=\"mailto:x@y.z\">m</a>";
        let html = page(body);
        assert_eq!(render(html.clone(), &Config::default()), html);
    }

    #[test]
    fn existing_attributes_are_kept_and_rel_merged() {
        let config = Config { nofollow: "sponsored".into(), icon: false, ..Config::default() };
        let out = render(page("<a class='btn' href='https://ex.org' rel='me' target='_self'>x</a>"), &config);
        assert!(out.contains("<a class=\"btn\" href=\"https://ex.org\" target=\"_self\" rel=\"me nofollow sponsored external\">x</a>"), "{out}");
    }

    #[test]
    fn images_get_no_arrow_and_scripts_are_untouched() {
        let body = "<a href=\"https://ex.org\"><img src=\"a.png\" alt=\"A\"></a><script>var s='<a href=\"https://ex.org\">';</script>";
        let out = render(page(body), &Config::default());
        assert!(!out.contains("<img src=\"a.png\" alt=\"A\"><svg"));
        assert!(out.contains("var s='<a href=\"https://ex.org\">';"));
        assert!(!out.contains("<abbr"));
    }

    #[test]
    fn skipped_and_extra_own_domains() {
        let config = Config {
            own_domains: domain_list("https://ateliernoord.nl"),
            skip_domains: domain_list("instagram.com"),
            ..Config::default()
        };
        let html = page("<a href=\"https://ateliernoord.nl/x\">a</a><a href=\"https://www.instagram.com/a\">i</a>");
        assert_eq!(render(html.clone(), &config), html);
    }

    #[test]
    fn hosts() {
        assert_eq!(host_of("https://Www.Example.com:8080/a?b").as_deref(), Some("example.com"));
        assert_eq!(host_of("/local"), None);
        assert!(belongs("shop.example.com", &["example.com".into()]));
        assert!(!belongs("notexample.com", &["example.com".into()]));
    }
}
