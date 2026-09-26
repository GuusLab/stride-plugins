//! Custom Code: snippets pasted in the site settings, placed in the head, at
//! the top of the body or at the end of the body of every published page.
//!
//! This is the one first-party plugin that inserts what it is given without
//! escaping it, because running the pasted code is the whole point: an
//! analytics tag, a verification meta tag, a chat widget. Only people who can
//! edit the site settings can paste code, and the panel says so plainly.
//!
//! One permission (`storage`, for the snippets), one hook, one panel. With
//! storage refused the page is served unchanged.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 6] = ["enabled", "head", "body-start", "body-end", "pages", "page-list"];
/// Written around every snippet, so a page that already holds them (a second
/// render of the same HTML) is left alone and the source says who put it there.
const MARK: &str = "stride:custom-code";
const MAX_SNIPPET: usize = 8192;

#[derive(Debug, Default, Clone, PartialEq)]
struct Config {
    enabled: bool,
    head: String,
    body_start: String,
    body_end: String,
    /// `all`, `only` or `except`.
    pages: String,
    page_list: Vec<String>,
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        Config {
            enabled: flag(values, "enabled", true),
            head: text(values, "head"),
            body_start: text(values, "body-start"),
            body_end: text(values, "body-end"),
            pages: match text(values, "pages").as_str() {
                "only" => "only".into(),
                "except" => "except".into(),
                _ => "all".into(),
            },
            page_list: slug_list(&text(values, "page-list")),
        }
    }

    fn applies_to(&self, slug: &str) -> bool {
        match self.pages.as_str() {
            "only" => slug_matches(&self.page_list, slug),
            "except" => !slug_matches(&self.page_list, slug),
            _ => true,
        }
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
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let config = match load_values() {
        Ok(values) => Config::from_values(&values),
        Err(error) => {
            stride_pdk::log("warn", &format!("no custom code added: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

fn wrap(place: &str, code: &str) -> String {
    format!("\n<!-- {MARK}:{place} -->\n{code}\n<!-- /{MARK}:{place} -->\n")
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || !config.applies_to(slug) || html.contains(&format!("<!-- {MARK}:")) {
        return html;
    }
    let mut html = html;
    if !config.head.is_empty() {
        insert_in_head(&mut html, &wrap("head", &config.head));
    }
    if !config.body_start.is_empty() {
        insert_after_body_start(&mut html, &wrap("body-start", &config.body_start));
    }
    if !config.body_end.is_empty() {
        insert_before_body_end(&mut html, &wrap("body-end", &config.body_end));
    }
    html
}

/// Why a snippet would break the page it is pasted into, if it would.
fn problem(place: &str, code: &str) -> Option<String> {
    let lower = code.to_ascii_lowercase();
    if code.len() > MAX_SNIPPET {
        return Some(format!("The {place} code is longer than {MAX_SNIPPET} characters. Load a larger script from a file instead."));
    }
    for tag in ["<html", "</html", "<head", "</head", "<body", "</body"] {
        if lower.contains(tag) {
            return Some(format!(
                "The {place} code contains {tag}>. Paste only the snippet itself, not a whole page."
            ));
        }
    }
    if lower.contains(&format!("<!-- {MARK}")) {
        return Some(format!("The {place} code contains this plugin's own markers. Remove them and save again."));
    }
    let opened = lower.matches("<script").count();
    let closed = lower.matches("</script>").count();
    if opened != closed {
        return Some(format!(
            "The {place} code has {opened} <script> and {closed} </script>. Close every script tag, or the rest of the page stops showing."
        ));
    }
    None
}

#[plugin_fn]
pub fn panel_custom_code(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let mut values = load_values().unwrap_or_default();
            values.entry("enabled").or_insert(JsonValue::Bool(true));
            values.entry("pages").or_insert_with(|| "all".into());
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: String| Ok(Json(PanelResponse { error, ..PanelResponse::default() }));
            for (place, code) in [
                ("head", &config.head),
                ("start of body", &config.body_start),
                ("end of body", &config.body_end),
            ] {
                if let Some(problem) = problem(place, code) {
                    return refuse(problem);
                }
            }
            if config.pages != "all" && config.page_list.is_empty() {
                return refuse("List at least one page, or choose \"Every page\".".to_owned());
            }

            let mut values = Map::new();
            values.insert("enabled".into(), config.enabled.into());
            values.insert("head".into(), config.head.clone().into());
            values.insert("body-start".into(), config.body_start.clone().into());
            values.insert("body-end".into(), config.body_end.clone().into());
            values.insert("pages".into(), config.pages.clone().into());
            values.insert("page-list".into(), config.page_list.join(", ").into());
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so it cannot keep your code.".to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            let empty = config.head.is_empty() && config.body_start.is_empty() && config.body_end.is_empty();
            Ok(Json(PanelResponse {
                values,
                message: if !config.enabled || empty {
                    "Saved. No code is added to your pages.".to_owned()
                } else {
                    "Saved. Publish your pages again to add the code to them.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>x</title></head><body class=\"a\"><p>x</p></body></html>";

    fn config() -> Config {
        Config {
            enabled: true,
            head: "<meta name=\"verify\" content=\"1\">".into(),
            body_start: "<noscript>n</noscript>".into(),
            body_end: "<script>var a=1</script>".into(),
            pages: "all".into(),
            page_list: vec![],
        }
    }

    #[test]
    fn places_each_snippet_once() {
        let out = render(PAGE.into(), "home", &config());
        let head = out.find("<meta name=\"verify\"").unwrap();
        assert!(head < out.find("</head>").unwrap());
        let start = out.find("<noscript>").unwrap();
        assert!(start > out.find("<body class=\"a\">").unwrap() && start < out.find("<p>x").unwrap());
        let end = out.find("<script>var a").unwrap();
        assert!(end > out.find("<p>x").unwrap() && end < out.find("</body>").unwrap());
        assert_eq!(render(out.clone(), "home", &config()), out);
    }

    #[test]
    fn respects_page_lists() {
        let mut c = config();
        c.pages = "only".into();
        c.page_list = slug_list("blog/*, contact");
        assert_eq!(render(PAGE.into(), "home", &c), PAGE);
        assert_ne!(render(PAGE.into(), "blog/hello", &c), PAGE);
        assert_ne!(render(PAGE.into(), "contact", &c), PAGE);
        c.pages = "except".into();
        assert_ne!(render(PAGE.into(), "home", &c), PAGE);
        assert_eq!(render(PAGE.into(), "contact", &c), PAGE);
    }

    #[test]
    fn off_and_empty_change_nothing() {
        let mut c = config();
        c.enabled = false;
        assert_eq!(render(PAGE.into(), "home", &c), PAGE);
        assert_eq!(render(PAGE.into(), "home", &Config { enabled: true, pages: "all".into(), ..Config::default() }), PAGE);
    }

    #[test]
    fn works_without_head_or_body() {
        let out = render("<p>bare</p>".into(), "home", &config());
        assert!(out.starts_with("<noscript>") || out.contains("<meta"));
        assert!(out.contains("<script>var a=1</script>"));
    }

    #[test]
    fn refuses_snippets_that_break_pages() {
        assert!(problem("head", "<script>unclosed").is_some());
        assert!(problem("head", "<html><body>x</body></html>").is_some());
        assert!(problem("head", &"a".repeat(MAX_SNIPPET + 1)).is_some());
        assert!(problem("head", "<script async src=\"https://x.test/a.js\"></script>").is_none());
    }
}
