//! Reading Progress: a Stride plugin.
//!
//! It puts a thin bar at the top of every long page that fills up as the
//! reader scrolls, and optionally a round back-to-top button in a corner.
//! The bar is a CSS scroll-driven animation (`animation-timeline: scroll()`);
//! browsers without it get the same result from a few hundred bytes of inline
//! JavaScript, which also hides the bar on short pages and shows the button
//! once the reader is half a screen down. No third-party requests.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-rp";
const DEFAULT_COLOR: &str = "#f43f5e";

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
pub fn panel_reading_progress(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                     It still shows the progress bar with the defaults shown here."
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
                "Saved. The progress bar is off. Publish the site again to remove it from \
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
pub enum Thickness {
    Thin,
    Medium,
    Bold,
}

impl Thickness {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "thin" => Some(Thickness::Thin),
            "medium" => Some(Thickness::Medium),
            "bold" => Some(Thickness::Bold),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Thickness::Thin => "thin",
            Thickness::Medium => "medium",
            Thickness::Bold => "bold",
        }
    }
    fn px(self) -> u32 {
        match self {
            Thickness::Thin => 3,
            Thickness::Medium => 5,
            Thickness::Bold => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// `#rrggbb`, or empty for the default rose.
    pub color: String,
    pub thickness: Thickness,
    pub back_to_top: bool,
    pub left: bool,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            color: String::new(),
            thickness: Thickness::Thin,
            back_to_top: true,
            left: false,
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
            color: s("color").and_then(valid_color).unwrap_or_default(),
            thickness: s("thickness").and_then(Thickness::parse).unwrap_or(d.thickness),
            back_to_top: b("backToTop").unwrap_or(d.back_to_top),
            left: s("buttonSide") == Some("left"),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("color".into(), self.color.clone().into());
        map.insert("thickness".into(), self.thickness.name().into());
        map.insert("backToTop".into(), self.back_to_top.into());
        map.insert("buttonSide".into(), self.side().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        if !self.color.is_empty() {
            values.insert("color".into(), self.color.clone().into());
        }
        values.insert("thickness".into(), self.thickness.name().into());
        values.insert("back-to-top".into(), self.back_to_top.into());
        values.insert("button-side".into(), self.side().into());
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
                "The colour has to look like #f43f5e. Nothing was saved.".to_owned()
            })?,
        };
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            color,
            thickness: s("thickness").and_then(Thickness::parse).unwrap_or(d.thickness),
            back_to_top: b("back-to-top").unwrap_or(d.back_to_top),
            left: s("button-side") == Some("left"),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn side(&self) -> &'static str {
        if self.left { "left" } else { "right" }
    }

    fn effective_color(&self) -> &str {
        if self.color.is_empty() { DEFAULT_COLOR } else { &self.color }
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

/// Dark or white, whichever reads better on `color` (a valid `#rrggbb`).
fn ink_on(color: &str) -> &'static str {
    let channel = |i: usize| {
        let v = f64::from(u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(0)) / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
    // The arrow is a graphic, so 3:1 is enough (WCAG 1.4.11). White looks
    // best on most brand colours; switch to near-black only when white fails.
    let on_white = 1.05 / (l + 0.05);
    if on_white >= 3.0 { "#fff" } else { "#111827" }
}

// ------------------------------------------------------------ the plain work

/// The page with the bar (and button) added, or `None` to leave it alone:
/// disabled, excluded, already done, or not a whole document.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || settings.exclude.iter().any(|s| s == slug) {
        return None;
    }
    if html.contains(MARKER) {
        return None;
    }
    let bytes = html.as_bytes();
    let body = body_open_end(bytes)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);

    let mut markup = String::from(
        "<div class=\"stride-rp\" aria-hidden=\"true\"><div class=\"stride-rp-bar\"></div></div>",
    );
    if settings.back_to_top {
        markup.push_str(
            "<a class=\"stride-rp-top\" href=\"#top\" aria-label=\"Back to top\">\
<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" focusable=\"false\">\
<path d=\"M12 19V5M5 12l7-7 7 7\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" \
stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg></a>",
        );
    }
    let style = style(settings);
    let script = format!("<script>{SCRIPT}</script>");

    let mut out = String::with_capacity(html.len() + style.len() + markup.len() + script.len());
    match head_close {
        Some(at) => {
            out.push_str(&html[..at]);
            out.push_str(&style);
            out.push_str(&html[at..body]);
            out.push_str(&markup);
        }
        None => {
            out.push_str(&html[..body]);
            out.push_str(&style);
            out.push_str(&markup);
        }
    }
    out.push_str(&html[body..body_close]);
    out.push_str(&script);
    out.push_str(&html[body_close..]);
    Some(out)
}

/// Scroll fallback, short-page check and button toggle. Everything is looked
/// up by class, so it is inert when the markup is not there.
const SCRIPT: &str = "(function(){var d=document,r=d.documentElement,b=d.querySelector('.stride-rp'),\
t=d.querySelector('.stride-rp-top'),c=window.CSS&&CSS.supports,\
s=!(c&&CSS.supports('animation-timeline','scroll()')),q=0;\
function u(){q=0;var h=innerHeight,m=r.scrollHeight-h,y=scrollY||r.scrollTop;\
if(b){b.hidden=m<h*.5;if(s)b.style.setProperty('--stride-rp',m>0?Math.min(1,Math.max(0,y/m)):0)}\
if(t)t.classList.toggle('stride-rp-show',y>h*.5)}\
function f(){if(!q){q=1;requestAnimationFrame(u)}}\
addEventListener('scroll',f,{passive:true});addEventListener('resize',f);\
if(t)t.addEventListener('click',function(e){e.preventDefault();\
scrollTo({top:0,behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'auto':'smooth'})});\
u()})();";

fn style(settings: &Settings) -> String {
    let color = settings.effective_color();
    let ink = ink_on(color);
    let px = settings.thickness.px();
    let side = settings.side();
    format!(
        "<style>.stride-rp{{position:fixed;top:0;left:0;right:0;height:{px}px;z-index:2147483000;\
pointer-events:none}}.stride-rp-bar{{height:100%;background:{color};transform-origin:0 50%;\
transform:scaleX(var(--stride-rp,0))}}@supports (animation-timeline:scroll()){{.stride-rp-bar{{\
animation:stride-rp linear both;animation-timeline:scroll(root)}}}}\
@keyframes stride-rp{{from{{transform:scaleX(0)}}to{{transform:scaleX(1)}}}}\
.stride-rp-top{{position:fixed;bottom:24px;{side}:24px;z-index:2147483000;width:44px;height:44px;\
border-radius:50%;display:flex;align-items:center;justify-content:center;background:{color};\
color:{ink};box-shadow:0 4px 14px rgba(0,0,0,.18),0 0 0 1px rgba(255,255,255,.35);\
opacity:0;visibility:hidden;transform:translateY(8px);\
transition:opacity .2s,transform .2s,visibility .2s}}\
.stride-rp-top.stride-rp-show{{opacity:1;visibility:visible;transform:none}}\
.stride-rp-top:hover{{filter:brightness(1.08)}}\
.stride-rp-top:focus-visible{{outline:3px solid {color};outline-offset:3px}}\
@media (prefers-reduced-motion:reduce){{.stride-rp-top{{transition:none}}}}\
@media print{{.stride-rp,.stride-rp-top{{display:none}}}}</style>"
    )
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
<body class=\"x\" data-a='>'><main><h1>Hi</h1></main></body></html>";

    #[test]
    fn adds_style_bar_button_and_script() {
        let html = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(html.contains("</style></head>"));
        assert!(html.contains("data-a='>'><div class=\"stride-rp\" aria-hidden=\"true\">"));
        assert!(html.contains("class=\"stride-rp-top\""));
        assert!(html.contains("</script></body></html>"));
        assert!(html.contains("background:#f43f5e"));
        assert!(html.contains("height:3px"));
    }

    #[test]
    fn is_idempotent_and_respects_settings() {
        let once = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(build(&once, "guide", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "guide", &off).is_none());
        let skip = Settings { exclude: vec!["guide".into()], ..Settings::default() };
        assert!(build(PAGE, "guide", &skip).is_none());
        let bare = Settings { back_to_top: false, left: true, ..Settings::default() };
        assert!(!build(PAGE, "guide", &bare).unwrap().contains("stride-rp-top\""));
    }

    #[test]
    fn refuses_fragments() {
        assert!(build("<p>no body</p>", "x", &Settings::default()).is_none());
        assert!(build("<bodyguard></bodyguard>", "x", &Settings::default()).is_none());
    }

    #[test]
    fn colours_and_ink() {
        assert_eq!(valid_color(" #ABC "), Some("#aabbcc".into()));
        assert_eq!(valid_color("red"), None);
        assert_eq!(valid_color("#12345g"), None);
        assert_eq!(ink_on("#f43f5e"), "#fff");
        assert_eq!(ink_on("#fde047"), "#111827");
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            color: "#0ea5e9".into(),
            thickness: Thickness::Bold,
            back_to_top: false,
            left: true,
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
