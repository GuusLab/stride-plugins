//! Print Friendly: a Stride plugin.
//!
//! It adds a print stylesheet to every page: navigation, footers, sidebars,
//! cookie banners and embeds are hidden, text is black on white, link
//! addresses are written after their links and headings, images, tables and
//! code blocks do not split awkwardly across pages. Optionally it adds a
//! "Print this page" button, after the content or in a corner.
//!
//! Everything is inline: about 1.5 KB of CSS that only applies on paper and a
//! few hundred bytes of JavaScript that runs when the visitor prints. No
//! third-party requests. It asks for `storage` only, to keep its settings;
//! without it the plugin still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-pf";
const DEFAULT_LABEL: &str = "Print this page";

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
pub fn panel_print_friendly(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                     Pages still print cleanly with the defaults shown here."
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
            let message = if submitted.enabled || submitted.button {
                "Saved. Publish the site again to update pages that are already live."
            } else {
                "Saved. The plugin now leaves pages alone. Publish the site again to update \
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

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The print stylesheet.
    pub enabled: bool,
    pub show_urls: bool,
    /// Extra selectors to hide on paper, each already checked.
    pub hide: Vec<String>,
    pub button: bool,
    /// Plain text, or empty for the default label. Escaped on output.
    pub label: String,
    pub corner: bool,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            show_urls: true,
            hide: Vec::new(),
            button: false,
            label: String::new(),
            corner: false,
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
            show_urls: b("showUrls").unwrap_or(d.show_urls),
            hide: s("hide").and_then(|h| parse_selectors(h).ok()).unwrap_or_default(),
            button: b("button").unwrap_or(d.button),
            label: s("buttonLabel").map(clean_label).unwrap_or_default(),
            corner: s("buttonPosition") == Some("corner"),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("showUrls".into(), self.show_urls.into());
        map.insert("hide".into(), self.hide.join(", ").into());
        map.insert("button".into(), self.button.into());
        map.insert("buttonLabel".into(), self.label.clone().into());
        map.insert("buttonPosition".into(), self.position().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("show-urls".into(), self.show_urls.into());
        values.insert("hide".into(), self.hide.join(", ").into());
        values.insert("button".into(), self.button.into());
        values.insert("button-label".into(), self.label.clone().into());
        values.insert("button-position".into(), self.position().into());
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| values.get(key).and_then(JsonValue::as_bool);
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            show_urls: b("show-urls").unwrap_or(d.show_urls),
            hide: parse_selectors(s("hide").unwrap_or(""))?,
            button: b("button").unwrap_or(d.button),
            label: s("button-label").map(clean_label).unwrap_or_default(),
            corner: s("button-position") == Some("corner"),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn position(&self) -> &'static str {
        if self.corner { "corner" } else { "end" }
    }

    fn effective_label(&self) -> &str {
        if self.label.is_empty() { DEFAULT_LABEL } else { &self.label }
    }
}

/// One line of plain text, at most 40 characters.
fn clean_label(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect()
}

/// ".newsletter, #comments" into [".newsletter", "#comments"]. Only the
/// characters selectors need are allowed, which is what keeps the value from
/// ever leaving its CSS rule (no braces, semicolons, `<`, `@` or backslash).
pub fn parse_selectors(text: &str) -> Result<Vec<String>, String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || " -_.#[]=\"':(),>+~*^$|".contains(c);
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in text.chars() {
        if !allowed(c) {
            return Err(format!(
                "\"{c}\" cannot be used in \"Also hide when printing\". Use CSS selectors \
                 such as .newsletter or #comments, separated by commas. Nothing was saved."
            ));
        }
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    out.push(current);
    if depth != 0 {
        return Err("A bracket in \"Also hide when printing\" is not closed. Nothing was saved."
            .to_owned());
    }
    Ok(out
        .into_iter()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .take(40)
        .collect())
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
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

// ------------------------------------------------------------ the plain work

/// The page with the print stylesheet (and button) added, or `None` to leave
/// it alone: nothing to do, excluded, already done, or not a whole document.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !(settings.enabled || settings.button) || settings.exclude.iter().any(|s| s == slug) {
        return None;
    }
    if html.contains(MARKER) {
        return None;
    }
    let bytes = html.as_bytes();
    let body = body_open_end(bytes)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);
    // After the content: just before the last `</main>`, else before `</body>`.
    let main_close = rfind_ci(&bytes[..body_close], b"</main>").filter(|&at| at >= body);

    let style = style(settings);
    let button = if settings.button { button(settings) } else { String::new() };
    let script = format!("<script>{}</script>", script(settings));

    let mut out = String::with_capacity(html.len() + style.len() + button.len() + script.len());
    match head_close {
        Some(at) => {
            out.push_str(&html[..at]);
            out.push_str(&style);
            out.push_str(&html[at..body]);
        }
        None => {
            out.push_str(&html[..body]);
            out.push_str(&style);
        }
    }
    match (settings.corner, main_close) {
        (true, _) => {
            out.push_str(&button);
            out.push_str(&html[body..body_close]);
        }
        (false, Some(at)) => {
            out.push_str(&html[body..at]);
            out.push_str(&button);
            out.push_str(&html[at..body_close]);
        }
        (false, None) => {
            out.push_str(&html[body..body_close]);
            out.push_str(&button);
        }
    }
    out.push_str(&script);
    out.push_str(&html[body_close..]);
    Some(out)
}

/// The button starts `hidden`: without JavaScript it could not print, so it
/// only appears once the script has wired it up.
fn button(settings: &Settings) -> String {
    let class = if settings.corner { "stride-pf stride-pf-corner" } else { "stride-pf" };
    format!(
        "<div class=\"{class}\" hidden><button type=\"button\" class=\"stride-pf-btn\">\
<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\" focusable=\"false\">\
<path d=\"M7 9V3h10v6M7 17H5a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-2\
M7 14h10v7H7z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linejoin=\"round\"/>\
</svg><span>{}</span></button></div>",
        escape(settings.effective_label())
    )
}

/// Selectors hidden on paper by default: site chrome, overlays and things
/// that do nothing on paper.
const HIDE: &str = "nav,[role=navigation],footer,[role=contentinfo],aside,\
[role=complementary],dialog,[aria-modal=true],[role=dialog],[role=alertdialog],iframe,video,audio,\
embed,object,form,button,[class*=cookie],[id*=cookie],[class*=consent],[id*=consent],\
.stride-pf";

fn style(settings: &Settings) -> String {
    let mut css = String::from("<style>");
    if settings.button {
        css.push_str(
            ".stride-pf{display:flex;justify-content:center;margin:2.5rem 1rem}\
.stride-pf[hidden]{display:none}\
.stride-pf-corner{position:fixed;right:24px;bottom:24px;margin:0;z-index:2147482000}\
.stride-pf-btn{display:inline-flex;align-items:center;gap:.5em;min-height:44px;\
padding:.55em 1.15em;border:1px solid #d6d3d1;border-radius:999px;background:#fff;color:#292524;\
font:inherit;font-size:.95rem;font-weight:600;line-height:1.2;cursor:pointer}\
.stride-pf-corner .stride-pf-btn{box-shadow:0 4px 14px rgba(0,0,0,.14)}\
.stride-pf-btn:hover{background:#f5f5f4;border-color:#a8a29e}\
.stride-pf-btn:focus-visible{outline:3px solid #57534e;outline-offset:2px}",
        );
    }
    css.push_str("@media print{");
    if settings.enabled {
        css.push_str(
            "*,*::before,*::after{background:transparent!important;color:#000!important;\
box-shadow:none!important;text-shadow:none!important}\
body{font-size:11pt;line-height:1.5}\
header,[style*=fixed],[style*=sticky]{position:static!important}\
a{text-decoration:underline}\
img,svg,canvas{max-width:100%!important;height:auto}\
h1,h2,h3,h4,h5,h6{break-after:avoid;break-inside:avoid}\
p,li,blockquote{orphans:3;widows:3}\
img,figure,table,pre,blockquote,tr{break-inside:avoid}\
thead{display:table-header-group}\
pre{white-space:pre-wrap!important;border:1px solid #999;padding:.5em}\
abbr[title]::after{content:\" (\" attr(title) \")\"}",
        );
        if settings.show_urls {
            css.push_str(
                "a[href^=\"http\"]::after{content:\" (\" attr(href) \")\";font-size:.85em;\
font-weight:normal;word-break:break-all}\
a[data-stride-pf-href]::after{content:\" (\" attr(data-stride-pf-href) \")\";font-size:.85em;\
font-weight:normal;word-break:break-all}\
a[data-stride-pf-same]::after{content:none}a:has(img)::after{content:none}",
            );
        }
        css.push_str(HIDE);
        css.push_str("{display:none!important}");
        // One rule per selector: an invalid one then drops only itself.
        for selector in &settings.hide {
            css.push_str(selector);
            css.push_str("{display:none!important}");
        }
    } else {
        css.push_str(".stride-pf{display:none!important}");
    }
    css.push_str("}@page{margin:1.8cm 1.6cm}</style>");
    if !settings.enabled {
        // Only the button is wanted: no page rule either.
        css = css.replace("@page{margin:1.8cm 1.6cm}", "");
    }
    css
}

/// Wires the button, and around printing: opens closed `<details>` (and
/// closes them again after), and gives relative links their full address.
fn script(settings: &Settings) -> String {
    let mut js = String::from(
        "(function(){var d=document,w=window,b=d.querySelector('.stride-pf'),o=[];\
if(b&&w.print){b.hidden=false;b.querySelector('button').addEventListener('click',function(){w.print()})}",
    );
    if settings.enabled {
        js.push_str(
            "w.addEventListener('beforeprint',function(){\
d.querySelectorAll('details:not([open])').forEach(function(e){e.open=true;o.push(e)});",
        );
        if settings.show_urls {
            js.push_str(
                "d.querySelectorAll('a[href]').forEach(function(a){\
var h=a.getAttribute('href'),t=a.textContent.trim();\
if(/^(#|javascript:|mailto:|tel:)/i.test(h)||a.querySelector('img'))return;\
if(t===a.href||t===h||t+'/'===a.href)a.setAttribute('data-stride-pf-same','');\
else if(!/^https?:/i.test(h))a.setAttribute('data-stride-pf-href',a.href)});",
            );
        }
        js.push_str(
            "});w.addEventListener('afterprint',function(){o.forEach(function(e){e.open=false});o=[]});",
        );
    }
    js.push_str("})();");
    js
}

/// The index just past the `>` of the opening `<body…>` tag.
fn body_open_end(bytes: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<body") {
        let at = from + rel;
        match bytes.get(at + 5) {
            Some(b'>') => return Some(at + 6),
            Some(c) if c.is_ascii_whitespace() || *c == b'/' => {
                let mut quote = 0u8;
                for (i, &c) in bytes[at + 5..].iter().enumerate() {
                    match c {
                        b'"' | b'\'' if quote == 0 => quote = c,
                        c if c == quote => quote = 0,
                        b'>' if quote == 0 => return Some(at + 5 + i + 1),
                        _ => {}
                    }
                }
                return None;
            }
            _ => from = at + 5,
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

    const PAGE: &str = "<!doctype html><html><head><title>T</title></head>\
<body class=\"x\" data-a='>'><nav>N</nav><main><h1>Hi</h1></main><footer>F</footer></body></html>";

    #[test]
    fn default_adds_print_css_and_script_but_no_button() {
        let html = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(html.contains("@media print{"));
        assert!(html.contains("</style></head>"));
        assert!(html.contains("</script></body></html>"));
        assert!(html.contains("attr(href)"));
        assert!(!html.contains("<div class=\"stride-pf"));
    }

    #[test]
    fn button_goes_after_the_content_or_in_the_corner() {
        let s = Settings { button: true, label: "Print <me>".into(), ..Settings::default() };
        let html = build(PAGE, "guide", &s).unwrap();
        assert!(html.contains("<span>Print &lt;me&gt;</span></button></div></main>"));
        let s = Settings { button: true, corner: true, ..Settings::default() };
        let html = build(PAGE, "guide", &s).unwrap();
        assert!(html.contains("data-a='>'><div class=\"stride-pf stride-pf-corner\" hidden>"));
        assert!(html.contains("<span>Print this page</span>"));
    }

    #[test]
    fn is_idempotent_and_respects_settings() {
        let once = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(build(&once, "guide", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "guide", &off).is_none());
        let skip = Settings { exclude: vec!["guide".into()], ..Settings::default() };
        assert!(build(PAGE, "guide", &skip).is_none());
        let no_urls = Settings { show_urls: false, ..Settings::default() };
        assert!(!build(PAGE, "guide", &no_urls).unwrap().contains("attr(href)"));
    }

    #[test]
    fn refuses_fragments() {
        assert!(build("<p>no body</p>", "x", &Settings::default()).is_none());
        assert!(build("<bodyguard></bodyguard>", "x", &Settings::default()).is_none());
    }

    #[test]
    fn selectors_are_checked() {
        assert_eq!(
            parse_selectors(" .news ,#c, a:not([href^='#']) ").unwrap(),
            vec![".news", "#c", "a:not([href^='#'])"]
        );
        assert!(parse_selectors("x}</style><script>").is_err());
        assert!(parse_selectors("a{b").is_err());
        assert!(parse_selectors("a:not(b").is_err());
        assert!(parse_selectors("").unwrap().is_empty());
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            enabled: true,
            show_urls: false,
            hide: vec![".newsletter".into(), "#comments".into()],
            button: true,
            label: "Print".into(),
            corner: true,
            exclude: vec!["home".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
    }
}
