//! Dark Mode Toggle: a Stride plugin.
//!
//! It gives every page a dark version and a switch for visitors. The dark
//! colours are derived at publish time from the page's own design colours
//! (the `--c-*` custom properties Stride renders into `<head>`): each one's
//! lightness is flipped in OKLab, so text that was dark on light becomes
//! light on dark with about the same contrast, and hues stay recognisable.
//!
//! Before a visitor chooses, the page follows `prefers-color-scheme` with
//! plain CSS. A choice is kept in `localStorage` and put back by a one-line
//! script in `<head>`, before anything paints, so there is no flash. The
//! switch itself needs JavaScript, so it is `hidden` until the script runs.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-dm";
const DEFAULT_LABEL: &str = "Dark mode";

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
    let html = build(&page.html, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_dark_mode(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                     It still adds dark mode with the defaults shown here."
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
                "Saved. Dark mode is off. Publish the site again to remove it from pages \
                 that are already live."
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
pub enum Start {
    System,
    Light,
    Dark,
}

impl Start {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Start::System),
            "light" => Some(Start::Light),
            "dark" => Some(Start::Dark),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Start::System => "system",
            Start::Light => "light",
            Start::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    BottomRight,
    BottomLeft,
    Header,
}

impl Placement {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "bottom-right" => Some(Placement::BottomRight),
            "bottom-left" => Some(Placement::BottomLeft),
            "header" => Some(Placement::Header),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Placement::BottomRight => "bottom-right",
            Placement::BottomLeft => "bottom-left",
            Placement::Header => "header",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub start: Start,
    pub placement: Placement,
    /// `#rrggbb`, or empty to derive.
    pub background: String,
    /// `#rrggbb`, or empty to derive.
    pub text: String,
    /// Token name (dashed, without `--c-`) and its dark `#rrggbb`.
    pub overrides: Vec<(String, String)>,
    /// Empty for the default.
    pub label: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            start: Start::System,
            placement: Placement::BottomRight,
            background: String::new(),
            text: String::new(),
            overrides: Vec::new(),
            label: String::new(),
        }
    }
}

impl Settings {
    /// Whatever is stored, never fail: a field that is missing or the wrong
    /// shape falls back to its default.
    pub fn from_json(value: &JsonValue) -> Self {
        let d = Settings::default();
        let s = |key: &str| value.get(key).and_then(JsonValue::as_str);
        Settings {
            enabled: value.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            start: s("start").and_then(Start::parse).unwrap_or(d.start),
            placement: s("placement").and_then(Placement::parse).unwrap_or(d.placement),
            background: s("background").and_then(valid_color).unwrap_or_default(),
            text: s("text").and_then(valid_color).unwrap_or_default(),
            overrides: s("overrides").and_then(|t| parse_overrides(t).ok()).unwrap_or_default(),
            label: s("label").map(clean_label).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("start".into(), self.start.name().into());
        map.insert("placement".into(), self.placement.name().into());
        map.insert("background".into(), self.background.clone().into());
        map.insert("text".into(), self.text.clone().into());
        map.insert("overrides".into(), self.overrides_text().into());
        map.insert("label".into(), self.label.clone().into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("start".into(), self.start.name().into());
        values.insert("placement".into(), self.placement.name().into());
        if !self.background.is_empty() {
            values.insert("background".into(), self.background.clone().into());
        }
        if !self.text.is_empty() {
            values.insert("text".into(), self.text.clone().into());
        }
        values.insert("overrides".into(), self.overrides_text().into());
        values.insert("label".into(), self.label.clone().into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let color = |key: &str, what: &str| match s(key).map(str::trim) {
            None | Some("") => Ok(String::new()),
            Some(color) => valid_color(color).ok_or_else(|| {
                format!("The {what} colour has to look like #111827. Nothing was saved.")
            }),
        };
        Ok(Settings {
            enabled: values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            start: s("start").and_then(Start::parse).unwrap_or(d.start),
            placement: s("placement").and_then(Placement::parse).unwrap_or(d.placement),
            background: color("background", "dark background")?,
            text: color("text", "dark text")?,
            overrides: parse_overrides(s("overrides").unwrap_or(""))?,
            label: s("label").map(clean_label).unwrap_or_default(),
        })
    }

    fn overrides_text(&self) -> String {
        self.overrides
            .iter()
            .map(|(name, color)| format!("{} = {color}", name.replace('-', ".")))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn label(&self) -> &str {
        if self.label.is_empty() { DEFAULT_LABEL } else { &self.label }
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

/// A design colour's name as the theme editor shows it (`surface.deep`) or
/// as CSS has it (`--c-surface-deep`), turned into the dashed CSS form.
fn token_name(name: &str) -> Option<String> {
    let name = name.trim();
    let name = name.strip_prefix("--c-").unwrap_or(name).replace('.', "-").to_ascii_lowercase();
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then_some(name)
}

/// "surface.deep = #0f2a22" per line. `:` works as well as `=`.
pub fn parse_overrides(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let bad = || {
            format!(
                "Line {} of the colour overrides should look like: surface.deep = #0f2a22. \
                 Nothing was saved.",
                i + 1
            )
        };
        let (name, color) = line.rsplit_once(['=', ':']).ok_or_else(bad)?;
        let name = token_name(name).ok_or_else(bad)?;
        let color = valid_color(color).ok_or_else(bad)?;
        out.retain(|(n, _)| *n != name);
        out.push((name, color));
        if out.len() > 64 {
            return Err("Keep the colour overrides to 64 lines. Nothing was saved.".to_owned());
        }
    }
    Ok(out)
}

fn clean_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect()
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

// ---------------------------------------------------------------- colour math

#[derive(Debug, Clone, Copy)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

fn to_linear(v: f64) -> f64 {
    if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn from_linear(v: f64) -> f64 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn rgb(color: &str) -> [f64; 3] {
    let c = |i: usize| f64::from(u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(0)) / 255.0;
    [c(1), c(3), c(5)]
}

fn lab(color: &str) -> Lab {
    let [r, g, b] = rgb(color).map(to_linear);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    Lab {
        l: 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        a: 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        b: 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    }
}

fn hex(c: Lab) -> String {
    let l = (c.l + 0.396_337_777_4 * c.a + 0.215_803_757_3 * c.b).powi(3);
    let m = (c.l - 0.105_561_345_8 * c.a - 0.063_854_172_8 * c.b).powi(3);
    let s = (c.l - 0.089_484_177_5 * c.a - 1.291_485_548 * c.b).powi(3);
    let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s;
    let byte = |v: f64| (from_linear(v.clamp(0.0, 1.0)) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// WCAG relative luminance, for the tests and the contrast guard.
fn luminance(color: &str) -> f64 {
    let [r, g, b] = rgb(color).map(to_linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub fn contrast(x: &str, y: &str) -> f64 {
    let (a, b) = (luminance(x), luminance(y));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The dark version of every design colour on the page.
///
/// The page's lightest colour becomes the dark background and its darkest
/// becomes the dark text; everything in between is spread over that range in
/// reverse, by OKLab lightness. Colour is kept but softened a little, since
/// saturated colours glare on a dark ground. A custom background or text
/// colour moves the ends of the range and tints the neutrals towards itself.
pub fn derive(tokens: &[(String, String)], settings: &Settings) -> Vec<(String, String)> {
    let bg = lab(if settings.background.is_empty() { "#14171c" } else { &settings.background });
    let fg = lab(if settings.text.is_empty() { "#eceef1" } else { &settings.text });
    let tint = !settings.background.is_empty() || !settings.text.is_empty();
    let labs: Vec<Lab> = tokens.iter().map(|(_, c)| lab(c)).collect();
    let lo = labs.iter().map(|c| c.l).fold(f64::INFINITY, f64::min);
    let hi = labs.iter().map(|c| c.l).fold(f64::NEG_INFINITY, f64::max);
    tokens
        .iter()
        .zip(&labs)
        .map(|((name, _), c)| {
            if let Some((_, color)) = settings.overrides.iter().find(|(n, _)| n == name) {
                return (name.clone(), color.clone());
            }
            // 0 for the page's lightest colour, 1 for its darkest.
            let t = if hi - lo < 0.2 { 1.0 - c.l } else { (hi - c.l) / (hi - lo) };
            let t = t.clamp(0.0, 1.0);
            let l = bg.l + (fg.l - bg.l) * t;
            let chroma = (c.a * c.a + c.b * c.b).sqrt();
            let keep = if chroma < 0.03 && tint { 0.0 } else { 0.8 };
            let (na, nb) = (bg.a + (fg.a - bg.a) * t, bg.b + (fg.b - bg.b) * t);
            let mix = if tint && chroma < 0.03 { 1.0 } else { 0.0 };
            let out = Lab { l, a: c.a * keep + na * mix, b: c.b * keep + nb * mix };
            (name.clone(), hex(out))
        })
        .collect()
}

// ------------------------------------------------------------ the plain work

/// Every `--c-name:#hex` declared in the page's `<head>`, last one wins,
/// in first-seen order. Values that are not plain hex are left alone.
pub fn tokens(head: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut rest = head;
    while let Some(at) = rest.find("--c-") {
        rest = &rest[at + 4..];
        let name_len = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
            .count();
        let name = rest[..name_len].to_ascii_lowercase();
        let after = rest[name_len..].trim_start();
        let Some(value) = after.strip_prefix(':') else { continue };
        let value = value.trim_start();
        let hex_len = value.bytes().skip(1).take_while(u8::is_ascii_hexdigit).count();
        if !value.starts_with('#') || name.is_empty() || name.len() > 64 {
            continue;
        }
        let Some(color) = valid_color(&value[..1 + hex_len]) else { continue };
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = color,
            None => out.push((name, color)),
        }
        if out.len() >= 200 {
            break;
        }
    }
    out
}

/// The page with dark mode added, or `None` to leave it alone: disabled,
/// already done, or not a whole document.
pub fn build(html: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let bytes = html.as_bytes();
    let body = body_open_end(bytes)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);
    let head = &html[..head_close.unwrap_or(body)];

    let dark = derive(&tokens(head), settings);
    let head_markup = format!("{}<script>{}</script>", style(&dark, settings), head_script());
    let button = button(settings);

    // Where the switch goes: at the end of the header's menu, else before
    // `</header>`, else floating right after `<body>`.
    let header_at = if settings.placement == Placement::Header {
        header_slot(bytes, body, body_close)
    } else {
        None
    };

    let mut out = String::with_capacity(html.len() + head_markup.len() + button.len() + 1200);
    match head_close {
        Some(at) => {
            out.push_str(&html[..at]);
            out.push_str(&head_markup);
            out.push_str(&html[at..body]);
        }
        None => {
            out.push_str(&html[..body]);
            out.push_str(&head_markup);
        }
    }
    match header_at {
        Some(at) => {
            out.push_str(&html[body..at]);
            out.push_str(&button);
            out.push_str(&html[at..body_close]);
        }
        None => {
            out.push_str(&button);
            out.push_str(&html[body..body_close]);
        }
    }
    out.push_str("<script>");
    out.push_str(&body_script(settings.start));
    out.push_str("</script>");
    out.push_str(&html[body_close..]);
    Some(out)
}

/// Inside the first `<header>`: just before the `</nav>` of its first menu,
/// or else just before `</header>`.
fn header_slot(bytes: &[u8], from: usize, to: usize) -> Option<usize> {
    let open = from + tag_at(&bytes[from..to], b"<header")?;
    let close = open + find_ci(&bytes[open..to], b"</header>")?;
    if let Some(nav) = tag_at(&bytes[open..close], b"<nav") {
        if let Some(end) = find_ci(&bytes[open + nav..close], b"</nav>") {
            return Some(open + nav + end);
        }
    }
    Some(close)
}

/// Where `<tag` starts as a whole tag name (so `<header` never matches `<headers`).
fn tag_at(bytes: &[u8], tag: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], tag) {
        let at = from + rel;
        match bytes.get(at + tag.len()) {
            Some(c) if c.is_ascii_whitespace() || *c == b'>' || *c == b'/' => return Some(at),
            _ => from = at + tag.len(),
        }
    }
    None
}

fn button(settings: &Settings) -> String {
    let place = match settings.placement {
        Placement::Header => "stride-dm-in",
        Placement::BottomLeft => "stride-dm-fl stride-dm-l",
        Placement::BottomRight => "stride-dm-fl",
    };
    format!(
        "<button type=\"button\" class=\"stride-dm {place}\" aria-pressed=\"false\" \
aria-label=\"{}\" hidden>\
<svg class=\"stride-dm-moon\" viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" \
focusable=\"false\"><path d=\"M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z\" fill=\"currentColor\"/></svg>\
<svg class=\"stride-dm-sun\" viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" \
focusable=\"false\"><circle cx=\"12\" cy=\"12\" r=\"4.5\" fill=\"currentColor\"/><path d=\"M12 2v2.5M12 \
19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8\" \
stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"/></svg></button>",
        escape(settings.label())
    )
}

/// Put a remembered choice back before anything paints.
fn head_script() -> &'static str {
    "try{var m=localStorage.getItem('stride-dm');if(m=='dark'||m=='light')\
document.documentElement.setAttribute('data-theme',m)}catch(e){}"
}

/// Show the switch, keep `aria-pressed` true to what is on screen, and
/// remember a choice. Inert when the switch is not there.
fn body_script(start: Start) -> String {
    let fallback = match start {
        Start::System => "q.matches",
        Start::Light => "false",
        Start::Dark => "true",
    };
    format!(
        "(function(){{var d=document.documentElement,b=d.querySelector('.stride-dm'),\
q=matchMedia('(prefers-color-scheme: dark)');if(!b)return;\
function k(){{var t=d.getAttribute('data-theme');return t?t=='dark':{fallback}}}\
function u(){{b.setAttribute('aria-pressed',k()?'true':'false')}}\
b.hidden=false;u();b.addEventListener('click',function(){{var n=k()?'light':'dark';\
d.setAttribute('data-theme',n);try{{localStorage.setItem('stride-dm',n)}}catch(e){{}}u()}});\
if(q.addEventListener)q.addEventListener('change',u)}})();"
    )
}

fn style(dark: &[(String, String)], settings: &Settings) -> String {
    let mut vars = String::from("color-scheme:dark");
    for (name, color) in dark {
        vars.push_str(&format!(";--c-{name}:{color}"));
    }
    // A page without design colours still gets a dark canvas.
    let canvas = if dark.is_empty() { "background:Canvas;color:CanvasText" } else { "" };
    let rule = |selector: &str| {
        let mut rule = format!("{selector}{{{vars}}}");
        if !canvas.is_empty() {
            rule.push_str(&format!("{selector} body{{{canvas}}}"));
        }
        rule
    };
    let palette = match settings.start {
        Start::System => format!(
            "{}html[data-theme=light]{{color-scheme:light}}\
@media (prefers-color-scheme:dark){{{}}}",
            rule("html[data-theme=dark]"),
            rule("html:not([data-theme=light])")
        ),
        Start::Light => format!("{}html:not([data-theme=dark]){{color-scheme:light}}", rule("html[data-theme=dark]")),
        Start::Dark => rule("html:not([data-theme=light])"),
    };
    format!(
        "<style id=\"stride-dm\">{palette}\
.stride-dm{{display:inline-flex;align-items:center;justify-content:center;width:44px;height:44px;\
padding:0;border-radius:50%;cursor:pointer;font:inherit;flex:none}}\
.stride-dm[hidden]{{display:none}}\
.stride-dm[aria-pressed=true] .stride-dm-moon,.stride-dm[aria-pressed=false] .stride-dm-sun{{display:none}}\
.stride-dm:focus-visible{{outline:2px solid currentColor;outline-offset:3px}}\
.stride-dm-fl{{position:fixed;bottom:20px;right:20px;z-index:2147483000;background:#fff;color:#1f2937;\
border:1px solid rgba(0,0,0,.12);box-shadow:0 4px 14px rgba(0,0,0,.16)}}\
.stride-dm-fl[aria-pressed=true]{{background:#262a31;color:#f5f5f4;border-color:rgba(255,255,255,.16);\
box-shadow:0 4px 14px rgba(0,0,0,.5)}}\
.stride-dm-l{{right:auto;left:20px}}\
.stride-dm-in{{width:40px;height:40px;background:transparent;color:inherit;\
border:1px solid rgba(127,127,127,.45)}}\
.stride-dm:hover{{filter:brightness(1.1)}}\
@media print{{.stride-dm{{display:none}}}}</style>"
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

    const STUDIO: &str = "<!doctype html><html><head><title>T</title><style>\
:root{--c-ink-body:#1d2b25;--c-ink-inverse:#f4f2e9;--c-ink-muted:#55605c;\
--c-surface-deep:#213b32;--c-surface-night:#1d2b25;--c-surface-page:#f4f2e9;\
--c-surface-raised:#e3e7ce;--c-odd:var(--c-ink-body)}</style></head>\
<body class=\"x\" data-a='>'><header class=\"h\"><div><nav class=\"n\"><a href=\"/\">Home</a></nav>\
</div></header><main><h1>Hi</h1></main></body></html>";

    fn get<'a>(dark: &'a [(String, String)], name: &str) -> &'a str {
        &dark.iter().find(|(n, _)| n == name).unwrap().1
    }

    #[test]
    fn reads_tokens() {
        let t = tokens(STUDIO);
        assert_eq!(t.len(), 7);
        assert_eq!(t[0], ("ink-body".to_owned(), "#1d2b25".to_owned()));
        assert!(!t.iter().any(|(n, _)| n == "odd"));
    }

    #[test]
    fn derived_palette_keeps_contrast() {
        let dark = derive(&tokens(STUDIO), &Settings::default());
        let page = get(&dark, "surface-page");
        assert!(luminance(page) < 0.03, "page {page}");
        assert!(contrast(get(&dark, "ink-body"), page) >= 7.0);
        assert!(contrast(get(&dark, "ink-muted"), page) >= 4.5);
        assert!(contrast(get(&dark, "ink-inverse"), get(&dark, "surface-night")) >= 7.0);
        assert!(contrast(get(&dark, "ink-body"), get(&dark, "surface-raised")) >= 4.5);
    }

    #[test]
    fn custom_ends_and_overrides() {
        let s = Settings {
            background: "#0b1020".into(),
            text: "#f0f4ff".into(),
            overrides: vec![("surface-deep".into(), "#123456".into())],
            ..Settings::default()
        };
        let dark = derive(&tokens(STUDIO), &s);
        assert_eq!(get(&dark, "surface-deep"), "#123456");
        assert!(contrast(get(&dark, "surface-page"), "#0b1020") < 1.1);
        assert!(contrast(get(&dark, "ink-body"), "#f0f4ff") < 1.1);
    }

    #[test]
    fn colour_round_trip() {
        for c in ["#000000", "#ffffff", "#1d2b25", "#f43f5e", "#334155"] {
            assert_eq!(hex(lab(c)), c);
        }
    }

    #[test]
    fn floating_by_default() {
        let html = build(STUDIO, &Settings::default()).unwrap();
        assert!(html.contains("</script></head>"));
        assert!(html.contains("data-a='>'><button type=\"button\" class=\"stride-dm stride-dm-fl\""));
        assert!(html.contains("@media (prefers-color-scheme:dark){html:not([data-theme=light])"));
        assert!(html.contains("aria-label=\"Dark mode\" hidden>"));
        assert!(html.ends_with("</script></body></html>"));
        assert!(build(&html, &Settings::default()).is_none());
    }

    #[test]
    fn header_placement_and_label_escaping() {
        let s = Settings {
            placement: Placement::Header,
            label: "Dark <b>\"mode\"".into(),
            ..Settings::default()
        };
        let html = build(STUDIO, &s).unwrap();
        assert!(html.contains("Home</a><button type=\"button\" class=\"stride-dm stride-dm-in\""));
        assert!(html.contains("aria-label=\"Dark &lt;b&gt;&quot;mode&quot;\""));
        let bare = STUDIO.replace("<header class=\"h\">", "<section>").replace("</header>", "</section>");
        assert!(build(&bare, &s).unwrap().contains("stride-dm-in\""));
    }

    #[test]
    fn start_modes_and_off() {
        let dark = Settings { start: Start::Dark, ..Settings::default() };
        let html = build(STUDIO, &dark).unwrap();
        assert!(html.contains("html:not([data-theme=light]){color-scheme:dark"));
        assert!(!html.contains("@media (prefers-color-scheme"));
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(STUDIO, &off).is_none());
        assert!(build("<p>no body</p>", &Settings::default()).is_none());
        let plain = build("<html><head></head><body><p>x</p></body></html>", &Settings::default()).unwrap();
        assert!(plain.contains("background:Canvas"));
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            start: Start::Light,
            placement: Placement::BottomLeft,
            background: "#0b1020".into(),
            text: "#f0f4ff".into(),
            overrides: vec![("surface-deep".into(), "#0f2a22".into())],
            label: "Donkere modus".into(),
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        let mut bad = Map::new();
        bad.insert("background".into(), "url(x)".into());
        assert!(Settings::from_values(&bad).is_err());
        let mut bad = Map::new();
        bad.insert("overrides".into(), "surface deep = red".into());
        assert!(Settings::from_values(&bad).is_err());
        assert_eq!(
            parse_overrides("--c-surface-deep: #ABC\n\nink.body=#fff").unwrap(),
            vec![("surface-deep".into(), "#aabbcc".into()), ("ink-body".into(), "#ffffff".into())]
        );
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
    }
}
