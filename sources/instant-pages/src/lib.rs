//! Instant Pages: the browser starts loading the page behind a link while the
//! visitor is still pointing at it, so the click feels instant, and pages
//! cross-fade into each other instead of flashing white.
//!
//! Both are standards the browser does itself — Speculation Rules and
//! cross-document View Transitions — declared in two small tags. No script
//! runs, nothing is sent anywhere, and a browser that does not know them
//! simply navigates as it always did.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 5] = ["enabled", "mode", "eagerness", "transition", "skip-links"];
const MARKER: &str = "id=\"stride-instant-pages\"";

/// Addresses that do something when visited (sign out, pay, call back) or
/// are not pages. Loading one early could act for the visitor, so they are
/// never loaded ahead, whatever the settings say.
const NEVER: [&str; 11] = [
    "/admin*", "/api/*", "/members/*", "/account*", "/checkout*", "/webhooks/*", "/mcp*", "/media/*",
    "/feed.xml", "/sitemap.xml", "/robots.txt",
];

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    /// `prerender` (load and render) or `prefetch` (download only).
    mode: String,
    /// `moderate` (pointer rests on a link) or `conservative` (pointer or finger goes down).
    eagerness: String,
    /// `fade`, `slide` or `none`.
    transition: String,
    /// Extra address patterns never to load ahead, like `/shop/cart`.
    skip_links: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            mode: "prerender".into(),
            eagerness: "moderate".into(),
            transition: "fade".into(),
            skip_links: Vec::new(),
        }
    }
}

fn pattern_list(text: &str) -> Vec<String> {
    text.split([',', '\n', ' '])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| if p.starts_with('/') { p.to_owned() } else { format!("/{p}") })
        // Only what a URL pattern can hold; anything else is dropped rather than escaped.
        .filter(|p| p.len() <= 200 && p.chars().all(|c| c.is_ascii_alphanumeric() || "/-_.*~".contains(c)))
        .collect()
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let pick = |name: &str, allowed: &[&str], fallback: &str| {
            let v = text(values, name);
            if allowed.contains(&v.as_str()) { v } else { fallback.to_owned() }
        };
        Config {
            enabled: flag(values, "enabled", d.enabled),
            mode: pick("mode", &["prerender", "prefetch"], &d.mode),
            eagerness: pick("eagerness", &["moderate", "conservative"], &d.eagerness),
            transition: pick("transition", &["fade", "slide", "none"], &d.transition),
            skip_links: pattern_list(&text(values, "skip-links")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("mode".into(), self.mode.clone().into());
        values.insert("eagerness".into(), self.eagerness.clone().into());
        values.insert("transition".into(), self.transition.clone().into());
        values.insert("skip-links".into(), self.skip_links.join(", ").into());
        values
    }
}

fn rules(config: &Config) -> String {
    let mut not: Vec<String> = NEVER.iter().chain(config.skip_links.iter().map(String::as_str).collect::<Vec<_>>().iter())
        .map(|p| format!("{{\"href_matches\":\"{p}\"}}"))
        .collect();
    // Links that ask not to be followed, download something, open elsewhere,
    // or carry a query (a search, a filter, a one-time link). A `?` inside an
    // href pattern starts the pattern's own query part, so this is a selector.
    not.push("{\"selector_matches\":\"[rel~=nofollow],[download],[target=_blank],[data-no-instant],[href*=\\\"?\\\"]\"}".into());
    format!(
        "<script type=\"speculationrules\" {MARKER}>{{\"{mode}\":[{{\"source\":\"document\",\"where\":{{\"and\":[\
         {{\"href_matches\":\"/*\"}},{}]}},\"eagerness\":\"{eagerness}\"}}]}}</script>",
        not.iter().map(|n| format!("{{\"not\":{n}}}")).collect::<Vec<_>>().join(","),
        mode = config.mode,
        eagerness = config.eagerness,
    )
}

fn transition(config: &Config) -> &'static str {
    match config.transition.as_str() {
        "none" => "",
        "slide" => "<style id=\"stride-instant-pages-transition\">@view-transition{navigation:auto}\
            @media (prefers-reduced-motion:no-preference){::view-transition-old(root){animation:sip-out .22s ease-in both}\
            ::view-transition-new(root){animation:sip-in .28s ease-out both}}\
            @keyframes sip-out{to{opacity:0;transform:translateX(-24px)}}\
            @keyframes sip-in{from{opacity:0;transform:translateX(24px)}}\
            @media (prefers-reduced-motion:reduce){@view-transition{navigation:none}}</style>",
        _ => "<style id=\"stride-instant-pages-transition\">@view-transition{navigation:auto}\
            @media (prefers-reduced-motion:reduce){@view-transition{navigation:none}}</style>",
    }
}

fn render(html: String, config: &Config) -> String {
    if !config.enabled || html.contains(MARKER) {
        return html;
    }
    let mut html = html;
    insert_in_head(&mut html, &format!("{}{}", transition(config), rules(config)));
    html
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
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let config = match load_values() {
        Ok(values) => Config::from_values(&values),
        Err(error) if error.is_permission_denied() => Config::default(),
        Err(error) => {
            stride_pdk::log("warn", &format!("instant pages left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &config) }))
}

#[plugin_fn]
pub fn panel_instant_pages(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let raw = text(&request.values, "skip-links");
            let config = Config::from_values(&request.values);
            let given = raw.split([',', '\n', ' ']).filter(|p| !p.trim().is_empty()).count();
            if given != config.skip_links.len() {
                return Ok(Json(PanelResponse {
                    error: "Write each address like /shop/cart or /downloads/*, using only letters, digits, dashes, dots and a * at the end.".to_owned(),
                    ..PanelResponse::default()
                }));
            }
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
                message: "Saved. Publish your pages again to update them.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stride_pdk::serde_json;

    const PAGE: &str = "<html><head><title>x</title></head><body><a href=\"/about\">About</a></body></html>";

    fn rules_json(html: &str) -> serde_json::Value {
        let start = html.find(MARKER).unwrap() + MARKER.len() + 1;
        let end = start + html[start..].find("</script>").unwrap();
        serde_json::from_str(&html[start..end]).unwrap()
    }

    #[test]
    fn adds_valid_rules_and_a_transition_in_head_once() {
        let out = render(PAGE.into(), &Config::default());
        assert!(out.find("@view-transition{navigation:auto}").unwrap() < out.find("</head>").unwrap());
        let rules = rules_json(&out);
        let rule = &rules["prerender"][0];
        assert_eq!(rule["eagerness"], "moderate");
        let and = rule["where"]["and"].as_array().unwrap();
        assert!(and.iter().any(|c| c["not"]["href_matches"] == "/members/*"));
        assert!(and.iter().any(|c| c["not"]["selector_matches"].as_str().is_some_and(|s| s.contains("[href*=\"?\"]"))));
        assert_eq!(render(out.clone(), &Config::default()), out);
    }

    #[test]
    fn prefetch_without_transition_and_extra_skips() {
        let c = Config {
            mode: "prefetch".into(),
            transition: "none".into(),
            skip_links: pattern_list("shop/cart, /downloads/*"),
            ..Config::default()
        };
        let out = render(PAGE.into(), &c);
        assert!(!out.contains("@view-transition"));
        let rules = rules_json(&out);
        let and = rules["prefetch"][0]["where"]["and"].as_array().unwrap().clone();
        assert!(and.iter().any(|c| c["not"]["href_matches"] == "/shop/cart"));
        assert!(and.iter().any(|c| c["not"]["href_matches"] == "/downloads/*"));
    }

    #[test]
    fn patterns_cannot_break_the_json_or_the_tag() {
        assert_eq!(pattern_list("/a\"b, </script>, /ok-*"), vec!["/ok-*".to_owned()]);
    }

    #[test]
    fn off_changes_nothing() {
        let c = Config { enabled: false, ..Config::default() };
        assert_eq!(render(PAGE.into(), &c), PAGE);
    }
}
