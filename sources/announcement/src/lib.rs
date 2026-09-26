//! Announcement Bar: one line at the top of every page, edited in the site
//! settings.
//!
//! One permission (`storage`, for the settings), one hook, one panel. When
//! storage is refused the page is served unchanged and the panel says why.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

// Each setting under its own key; `message` and `tone` are the keys the
// original example used, so existing settings carry over.
const FIELDS: [&str; 6] = ["message", "link-text", "link-url", "tone", "colour", "dismissible"];
const MARKER: &str = "id=\"stride-announcement\"";

#[derive(Debug, Default, Clone)]
struct Config {
    message: String,
    link_text: String,
    link_url: String,
    tone: String,
    colour: String,
    dismissible: bool,
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let text = |name: &str| {
            values
                .get(name)
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        Config {
            message: text("message"),
            link_text: text("link-text"),
            link_url: text("link-url"),
            tone: text("tone"),
            colour: text("colour"),
            dismissible: values
                .get("dismissible")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
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
    let values = match load_values() {
        Ok(values) => values,
        Err(error) => {
            // Refused storage is not a fault: show nothing, carry on.
            stride_pdk::log("warn", &format!("no announcement shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered {
        html: render(page.html, &Config::from_values(&values)),
    }))
}

fn render(html: String, config: &Config) -> String {
    if config.message.trim().is_empty() || html.contains(MARKER) {
        return html;
    }
    let (style, bar) = bar(config);
    // Right after the opening <body>, or at the very top without one.
    let at = find_ci(&html, "<body").and_then(|at| html[at..].find('>').map(|c| at + c + 1));
    let mut out = match at {
        Some(at) => {
            let mut out = html;
            out.insert_str(at, &bar);
            out
        }
        None => return format!("{style}{bar}{html}"),
    };
    // The stylesheet belongs in <head>; next to the bar only without one.
    match find_ci(&out, "</head>") {
        Some(head) => out.insert_str(head, &style),
        None => out.insert_str(at.unwrap_or(0), &style),
    }
    out
}

fn bar(config: &Config) -> (String, String) {
    let background = match config.tone.as_str() {
        "loud" => "#1d4ed8",
        "warm" => "#c2410c",
        "custom" if is_colour(&config.colour) => config.colour.as_str(),
        _ => "#111827",
    };
    let foreground = readable_on(background);
    let message = escape(&config.message);
    let url = safe_url(&config.link_url);

    let content = match (&url, config.link_text.is_empty()) {
        (Some(url), true) => format!("<a href=\"{}\">{message}</a>", escape(url)),
        (Some(url), false) => format!(
            "<span>{message}</span> <a href=\"{}\">{}<span aria-hidden=\"true\"> &rarr;</span></a>",
            escape(url),
            escape(&config.link_text)
        ),
        (None, _) => format!("<span>{message}</span>"),
    };

    let (close, script) = if config.dismissible {
        let key = format!("stride-announcement:{:08x}", fnv(&config.message));
        (
            "<button type=\"button\" aria-label=\"Close announcement\">&times;</button>".to_owned(),
            format!(
                "<script>(function(){{var b=document.getElementById('stride-announcement'),k='{key}';\
                 try{{if(localStorage.getItem(k)){{b.hidden=true;return}}}}catch(e){{}}\
                 b.querySelector('button').addEventListener('click',function(){{b.hidden=true;\
                 try{{localStorage.setItem(k,'1')}}catch(e){{}}}})}})()</script>"
            ),
        )
    } else {
        (String::new(), String::new())
    };

    let style = format!(
        "<style>#stride-announcement{{background:{background};color:{foreground};\
         font:500 14px/1.45 system-ui,-apple-system,\"Segoe UI\",sans-serif;text-align:center;\
         padding:10px 48px;position:relative;letter-spacing:.01em}}\
         #stride-announcement[hidden]{{display:none}}\
         #stride-announcement p{{margin:0;max-width:72rem;margin-inline:auto}}\
         #stride-announcement a{{color:inherit;font-weight:650;text-decoration:underline;\
         text-underline-offset:3px;text-decoration-thickness:1px}}\
         #stride-announcement a:hover{{text-decoration-thickness:2px}}\
         #stride-announcement a:focus-visible,#stride-announcement button:focus-visible\
         {{outline:2px solid currentColor;outline-offset:2px;border-radius:3px}}\
         #stride-announcement button{{position:absolute;right:8px;top:50%;transform:translateY(-50%);\
         width:32px;height:32px;border:0;border-radius:6px;background:transparent;color:inherit;\
         font:400 22px/1 system-ui,sans-serif;cursor:pointer;opacity:.8}}\
         #stride-announcement button:hover{{opacity:1;background:rgba(127,127,127,.2)}}\
         @media (max-width:600px){{#stride-announcement{{padding-left:16px;text-align:left;\
         padding-right:{}px}}}}</style>",
        if config.dismissible { 48 } else { 16 }
    );
    (
        style,
        format!(
            "<div {MARKER} role=\"region\" aria-label=\"Announcement\"><p>{content}</p>{close}</div>{script}"
        ),
    )
}

#[plugin_fn]
pub fn panel_announcement(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let mut values = load_values().unwrap_or_default();
            values.entry("tone").or_insert_with(|| "quiet".into());
            values.entry("colour").or_insert_with(|| "#f97316".into());
            values.entry("dismissible").or_insert(JsonValue::Bool(false));
            Ok(Json(PanelResponse {
                values,
                ..PanelResponse::default()
            }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: String| {
                Ok(Json(PanelResponse {
                    error,
                    ..PanelResponse::default()
                }))
            };
            if !config.link_url.is_empty() && safe_url(&config.link_url).is_none() {
                return refuse(
                    "The link address must start with https://, http://, mailto:, / or #."
                        .to_owned(),
                );
            }
            if !config.link_url.is_empty() && config.message.is_empty() {
                return refuse("Add a message for the link to belong to.".to_owned());
            }
            let tone = match config.tone.as_str() {
                "loud" | "warm" | "custom" => config.tone.clone(),
                _ => "quiet".to_owned(),
            };
            if tone == "custom" && !is_colour(&config.colour) {
                return refuse("Pick a bar colour for the style \"My own colour\".".to_owned());
            }
            let colour = if is_colour(&config.colour) {
                config.colour.to_lowercase()
            } else {
                "#f97316".to_owned()
            };

            let mut values = Map::new();
            values.insert("message".into(), config.message.clone().into());
            values.insert("link-text".into(), config.link_text.clone().into());
            values.insert("link-url".into(), config.link_url.clone().into());
            values.insert("tone".into(), tone.into());
            values.insert("colour".into(), colour.into());
            values.insert("dismissible".into(), config.dismissible.into());

            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere \
                         to keep the announcement."
                            .to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: if config.message.is_empty() {
                    "The announcement bar is off.".to_owned()
                } else {
                    "Saved. It is on every page of this site.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

/// Only schemes that cannot run script; `//host` is refused as ambiguous.
fn safe_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with('#')
        || (lower.starts_with('/') && !lower.starts_with("//"));
    ok.then(|| url.to_owned())
}

fn is_colour(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Dark or light text by WCAG relative luminance, whichever contrasts more.
fn readable_on(colour: &str) -> &'static str {
    let channel = |i: usize| {
        let v = u8::from_str_radix(&colour[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
    // Contrast with white vs with #111827 (L ~= 0.0116).
    if (1.05 / (l + 0.05)) >= ((l + 0.05) / 0.0616) { "#fff" } else { "#111827" }
}

fn fnv(text: &str) -> u32 {
    text.bytes()
        .fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193))
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
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
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(message: &str) -> Config {
        Config { message: message.into(), ..Config::default() }
    }

    #[test]
    fn nothing_without_a_message() {
        let html = "<html><body><p>x</p></body></html>".to_owned();
        assert_eq!(render(html.clone(), &config("  ")), html);
    }

    #[test]
    fn escapes_and_inserts_after_body_once() {
        let out = render("<html><head></head><BODY class=a><p>x</p>".into(), &config("<b>Sale</b>"));
        assert!(out.contains("<BODY class=a><div id=\"stride-announcement\""));
        assert!(out.contains("<style>#stride-announcement{background:#111827"));
        assert!(out.find("<style>") < out.find("</head>"));
        assert!(out.contains("&lt;b&gt;Sale&lt;/b&gt;"));
        assert_eq!(render(out.clone(), &config("again")), out);
    }

    #[test]
    fn refuses_script_urls() {
        assert!(safe_url("javascript:alert(1)").is_none());
        assert!(safe_url("//evil.test").is_none());
        assert!(safe_url("/shop").is_some());
    }

    #[test]
    fn readable_text() {
        assert_eq!(readable_on("#111827"), "#fff");
        assert_eq!(readable_on("#fde047"), "#111827");
        assert_eq!(readable_on("#c2410c"), "#fff");
    }
}
