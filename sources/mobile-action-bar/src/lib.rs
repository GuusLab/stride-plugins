//! Mobile Action Bar: a bar fixed to the bottom of the screen on phones, with
//! the three or four things a visitor of a local business wants — call, mail,
//! get directions, book — one thumb away.
//!
//! Plain links and inline icons, no JavaScript. The page gets bottom padding
//! on the same screens so the bar never hides the end of it.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 10] = [
    "enabled", "phone", "email", "address", "button-text", "button-url", "language", "colour", "screens", "skip-pages",
];
const MARKER: &str = "id=\"stride-action-bar\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    phone: String,
    email: String,
    address: String,
    button_text: String,
    button_url: String,
    language: String,
    colour: String,
    /// `phones` or `all`.
    screens: String,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            phone: String::new(),
            email: String::new(),
            address: String::new(),
            button_text: String::new(),
            button_url: String::new(),
            language: "en".into(),
            colour: "#0f766e".into(),
            screens: "phones".into(),
            skip_pages: Vec::new(),
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let colour = text(values, "colour").to_ascii_lowercase();
        let language = text(values, "language");
        Config {
            enabled: flag(values, "enabled", d.enabled),
            phone: text(values, "phone"),
            email: text(values, "email"),
            address: text(values, "address"),
            button_text: text(values, "button-text"),
            button_url: text(values, "button-url"),
            language: if ["en", "nl", "de", "fr"].contains(&language.as_str()) { language } else { d.language },
            colour: if is_colour(&colour) { colour } else { d.colour },
            screens: if text(values, "screens") == "all" { "all".into() } else { d.screens },
            skip_pages: slug_list(&text(values, "skip-pages")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("phone".into(), self.phone.clone().into());
        values.insert("email".into(), self.email.clone().into());
        values.insert("address".into(), self.address.clone().into());
        values.insert("button-text".into(), self.button_text.clone().into());
        values.insert("button-url".into(), self.button_url.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("screens".into(), self.screens.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
    }
}

/// `+31 (0)20 123 45 67` -> `+31201234567`; `None` when it is not a number.
fn tel(phone: &str) -> Option<String> {
    let phone = phone.replace("(0)", "");
    let mut out = String::new();
    for (i, c) in phone.chars().enumerate() {
        match c {
            '0'..='9' => out.push(c),
            '+' if i == 0 || out.is_empty() => out.push(c),
            ' ' | '-' | '.' | '(' | ')' | '/' => {}
            _ => return None,
        }
    }
    (out.trim_start_matches('+').len() >= 6).then_some(out)
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.chars().all(|c| !c.is_whitespace() && !c.is_control() && !"<>\"'(),;:[]\\".contains(c) || c == '@')
}

/// Percent-encodes a value for a URL query.
fn query(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn labels(language: &str) -> [&'static str; 4] {
    match language {
        "nl" => ["Bellen", "Mailen", "Route", "Snelle acties"],
        "de" => ["Anrufen", "E-Mail", "Route", "Schnellaktionen"],
        "fr" => ["Appeler", "E-mail", "Itinéraire", "Actions rapides"],
        _ => ["Call", "Email", "Directions", "Quick actions"],
    }
}

const ICON_CALL: &str = "<path d=\"M5 3h3l2 5-2.5 1.5a11 11 0 0 0 5 5L14 12l5 2v3a2 2 0 0 1-2 2A16 16 0 0 1 3 5a2 2 0 0 1 2-2\"/>";
const ICON_MAIL: &str = "<rect x=\"3\" y=\"5\" width=\"18\" height=\"14\" rx=\"2\"/><path d=\"m3 7 9 6 9-6\"/>";
const ICON_MAP: &str = "<path d=\"M12 21s-7-5.5-7-11a7 7 0 0 1 14 0c0 5.5-7 11-7 11z\"/><circle cx=\"12\" cy=\"10\" r=\"2.5\"/>";
const ICON_GO: &str = "<path d=\"M8 3v3M16 3v3M4 8h16M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z\"/><path d=\"m9 14 2 2 4-4\"/>";

fn icon(path: &str) -> String {
    format!(
        "<svg viewBox=\"0 0 24 24\" width=\"22\" height=\"22\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" \
         stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{path}</svg>"
    )
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
            stride_pdk::log("warn", &format!("no action bar shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &Config::from_values(&values)) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || slug_matches(&config.skip_pages, slug) || html.contains(MARKER) {
        return html;
    }
    let [call, mail, route, region] = labels(&config.language);
    let mut actions = Vec::new();
    if let Some(number) = tel(&config.phone) {
        actions.push(format!("<a href=\"tel:{number}\">{}<span>{call}</span></a>", icon(ICON_CALL)));
    }
    if valid_email(&config.email) {
        actions.push(format!("<a href=\"mailto:{}\">{}<span>{mail}</span></a>", escape(&config.email), icon(ICON_MAIL)));
    }
    if !config.address.is_empty() {
        actions.push(format!(
            "<a href=\"https://www.google.com/maps/search/?api=1&amp;query={}\" target=\"_blank\" rel=\"noopener\">{}<span>{route}</span></a>",
            query(&config.address),
            icon(ICON_MAP)
        ));
    }
    if let (Some(url), false) = (safe_url(&config.button_url), config.button_text.is_empty()) {
        actions.push(format!(
            "<a class=\"sab-main\" href=\"{}\">{}<span>{}</span></a>",
            escape(&url),
            icon(ICON_GO),
            escape(&config.button_text)
        ));
    }
    if actions.is_empty() {
        return html;
    }
    let fg = readable_on(&config.colour);
    let media = if config.screens == "all" { "all" } else { "(max-width:768px)" };
    let style = format!(
        "<style>#stride-action-bar{{display:none}}@media {media}{{\
         #stride-action-bar{{display:flex;position:fixed;left:0;right:0;bottom:0;z-index:2147483000;\
         background:#fff;border-top:1px solid rgba(0,0,0,.08);box-shadow:0 -6px 24px rgba(0,0,0,.08);\
         padding:8px max(8px,env(safe-area-inset-right)) max(8px,env(safe-area-inset-bottom)) max(8px,env(safe-area-inset-left));gap:6px}}\
         #stride-action-bar a{{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:3px;\
         min-height:52px;border-radius:12px;color:#1f2937;text-decoration:none;\
         font:600 12.5px/1.2 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         #stride-action-bar a svg{{color:{accent}}}\
         #stride-action-bar a:active{{background:rgba(0,0,0,.05)}}\
         #stride-action-bar a:focus-visible{{outline:2px solid {accent};outline-offset:2px}}\
         #stride-action-bar a.sab-main{{flex:1.6;flex-direction:row;gap:8px;background:{accent};color:{fg};font-size:15px}}\
         #stride-action-bar a.sab-main svg{{color:inherit}}\
         body{{padding-bottom:calc(76px + env(safe-area-inset-bottom))}}}}\
         @media (min-width:769px){{#stride-action-bar{{max-width:560px;margin:0 auto;left:16px;right:16px;bottom:16px;\
         border:1px solid rgba(0,0,0,.08);border-radius:18px}}}}\
         @media print{{#stride-action-bar{{display:none}}}}</style>",
        accent = config.colour,
    );
    let bar = format!("<nav {MARKER} aria-label=\"{region}\">{}</nav>", actions.join(""));
    let mut html = html;
    insert_in_head(&mut html, &style);
    insert_before_body_end(&mut html, &bar);
    html
}

#[plugin_fn]
pub fn panel_mobile_action_bar(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: &str| Ok(Json(PanelResponse { error: error.to_owned(), ..PanelResponse::default() }));
            if !config.phone.is_empty() && tel(&config.phone).is_none() {
                return refuse("The phone number can only hold digits, spaces, dashes and a + at the start, for example +31 20 123 4567.");
            }
            if !config.email.is_empty() && !valid_email(&config.email) {
                return refuse("The email address does not look right. Check it for typos, for example hello@example.com.");
            }
            if !config.button_url.is_empty() && safe_url(&config.button_url).is_none() {
                return refuse("The button link must start with https://, http://, mailto:, tel:, / or #.");
            }
            if config.button_url.is_empty() != config.button_text.is_empty() {
                return refuse("Fill in both the button text and its link, or leave both empty.");
            }
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(&if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere to keep your details.".to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            let empty = config.phone.is_empty() && config.email.is_empty() && config.address.is_empty() && config.button_text.is_empty();
            Ok(Json(PanelResponse {
                values,
                message: if !config.enabled || empty {
                    "Saved. No action bar is shown.".to_owned()
                } else {
                    "Saved. Publish your pages again to show the bar.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head></head><body><main>Hi</main></body></html>";

    fn config() -> Config {
        Config {
            phone: "+31 (0)20 123 45 67".into(),
            email: "hello@atelier.test".into(),
            address: "NDSM-plein 28, Amsterdam".into(),
            button_text: "Book a visit".into(),
            button_url: "/book".into(),
            ..Config::default()
        }
    }

    #[test]
    fn renders_all_actions_once() {
        let out = render(PAGE.into(), "home", &config());
        assert!(out.contains("href=\"tel:+31201234567\""));
        assert!(out.contains("href=\"mailto:hello@atelier.test\""));
        assert!(out.contains("query=NDSM-plein+28%2C+Amsterdam"));
        assert!(out.contains("<a class=\"sab-main\" href=\"/book\">"));
        assert!(out.find("<nav id=\"stride-action-bar\"").unwrap() > out.find("</main>").unwrap());
        assert!(out.find("<style>").unwrap() < out.find("</head>").unwrap());
        assert_eq!(render(out.clone(), "home", &config()), out);
    }

    #[test]
    fn nothing_to_show_changes_nothing() {
        assert_eq!(render(PAGE.into(), "home", &Config::default()), PAGE);
        let skip = Config { skip_pages: slug_list("home"), ..config() };
        assert_eq!(render(PAGE.into(), "home", &skip), PAGE);
    }

    #[test]
    fn bad_input_is_dropped_not_rendered() {
        let c = Config {
            phone: "call me".into(),
            email: "x\"onmouseover=\"alert(1)@a.b".into(),
            button_text: "<b>Go</b>".into(),
            button_url: "javascript:alert(1)".into(),
            ..Config::default()
        };
        assert_eq!(render(PAGE.into(), "home", &c), PAGE);
    }

    #[test]
    fn dutch_labels() {
        let c = Config { language: "nl".into(), ..config() };
        let out = render(PAGE.into(), "home", &c);
        assert!(out.contains("<span>Bellen</span>") && out.contains("aria-label=\"Snelle acties\""));
    }

    #[test]
    fn phone_and_email_rules() {
        assert_eq!(tel("020-1234567").as_deref(), Some("0201234567"));
        assert_eq!(tel("+1 (555) 010-0199").as_deref(), Some("+15550100199"));
        assert!(tel("12+34").is_none());
        assert!(valid_email("a.b@c.nl"));
        assert!(!valid_email("a b@c.nl") && !valid_email("ab@cnl") && !valid_email("<a>@b.c"));
    }
}
