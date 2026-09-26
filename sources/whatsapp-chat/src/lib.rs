//! WhatsApp Chat: a floating "chat with us on WhatsApp" button on every page.
//!
//! One permission (`storage`, for the panel's settings), one hook and one
//! panel. The button is a plain link to `https://wa.me/<number>?text=...`, so
//! nothing is loaded from WhatsApp or anyone else until a visitor clicks it.
//! The animation is CSS only and respects `prefers-reduced-motion`; the only
//! script is the optional opening-hours check (about 500 bytes).
//!
//! Without a stored number there is nothing to link to, so the page is left
//! untouched. That is also what happens when `storage` was refused.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};

/// All settings live under one key, so a render costs one host call.
const CONFIG: &str = "config";
/// Marks the injected block so a page never gets two buttons.
const MARKER: &str = "id=\"swc\"";
/// WhatsApp green. The button is always white-on-green in this colour, as in
/// WhatsApp's own brand; the link's accessible name carries the meaning.
const DEFAULT_COLOUR: &str = "#25D366";
const DEFAULT_TZ: &str = "Europe/Amsterdam";

struct Config {
    phone: String,
    message: String,
    label: String,
    left: bool,
    colour: String,
    pulse: bool,
    hours: Option<Hours>,
}

struct Hours {
    days: &'static str,
    open: u32,
    close: u32,
    timezone: String,
}

fn defaults() -> Map<String, JsonValue> {
    let mut values = Map::new();
    for (key, value) in [
        ("phone", ""),
        ("message", ""),
        ("label", ""),
        ("position", "right"),
        ("colour", DEFAULT_COLOUR),
        ("days", "weekdays"),
        ("open", "09:00"),
        ("close", "17:00"),
        ("timezone", DEFAULT_TZ),
    ] {
        values.insert(key.into(), value.into());
    }
    values.insert("pulse".into(), true.into());
    values.insert("hours".into(), false.into());
    values
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let text = |name: &str| {
            values.get(name).and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned()
        };
        let flag = |name: &str, fallback: bool| {
            values.get(name).and_then(JsonValue::as_bool).unwrap_or(fallback)
        };
        let colour = text("colour");
        let hours = match (parse_time(&text("open")), parse_time(&text("close"))) {
            (Some(open), Some(close)) if flag("hours", false) && open < close => Some(Hours {
                days: match text("days").as_str() {
                    "all" => "0123456",
                    "mon-sat" => "123456",
                    _ => "12345",
                },
                open,
                close,
                timezone: {
                    let tz = text("timezone");
                    if is_timezone(&tz) { tz } else { DEFAULT_TZ.to_owned() }
                },
            }),
            _ => None,
        };
        Config {
            phone: normalise_phone(&text("phone")).unwrap_or_default(),
            message: text("message"),
            label: text("label"),
            left: text("position") == "left",
            colour: if is_colour(&colour) { colour } else { DEFAULT_COLOUR.to_owned() },
            pulse: flag("pulse", true),
            hours,
        }
    }
}

/// The stored settings merged over the defaults. Never fails.
fn load_values() -> Map<String, JsonValue> {
    let mut values = defaults();
    match kv::get::<Map<String, JsonValue>>(CONFIG) {
        Ok(Some(stored)) => values.extend(stored),
        Ok(None) => {}
        Err(error) if error.is_permission_denied() => {}
        Err(error) => stride_pdk::log("warn", &format!("using default settings: {error}")),
    }
    values
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    Ok(Json(PageRendered { html: render(page.html, &Config::from_values(&load_values())) }))
}

fn render(html: String, config: &Config) -> String {
    if config.phone.is_empty() || html.contains(MARKER) {
        return html;
    }
    let block = button(config, is_dutch(&html));
    match rfind_ci(&html, "</body") {
        Some(at) => {
            let mut html = html;
            html.insert_str(at, &block);
            html
        }
        None => html + &block,
    }
}

/// Shows the button only inside the opening hours, in the site's time zone.
/// A zone the browser does not know shows the button rather than hiding it.
const HOURS_SCRIPT: &str = r#"(function(){var e=document.getElementById('swc'),d=e.dataset;function t(){try{var p={};new Intl.DateTimeFormat('en-GB',{timeZone:d.tz,weekday:'short',hour:'2-digit',minute:'2-digit',hourCycle:'h23'}).formatToParts(new Date()).forEach(function(x){p[x.type]=x.value});var w='SunMonTueWedThuFriSat'.indexOf(p.weekday)/3,m=p.hour*60+ +p.minute;e.hidden=!(d.days.indexOf(w)>=0&&m>=+d.open&&m<+d.close)}catch(x){e.hidden=false}}t();setInterval(t,60000)})();"#;

/// White glyph: a speech bubble with a telephone handset, 32x32.
/// The official WhatsApp glyph (simple-icons "whatsapp", CC0), used only to
/// link to WhatsApp as Meta's brand guidelines allow.
const GLYPH: &str = "<svg viewBox=\"0 0 24 24\" width=\"30\" height=\"30\" aria-hidden=\"true\" focusable=\"false\">\
<path fill=\"currentColor\" d=\"M17.472 14.382c-.297-.149-1.758-.867-2.03-.967-.273-.099-.471-.148-.67.15-.197.297-.767.966-.94 1.164-.173.199-.347.223-.644.075-.297-.15-1.255-.463-2.39-1.475-.883-.788-1.48-1.761-1.653-2.059-.173-.297-.018-.458.13-.606.134-.133.298-.347.446-.52.149-.174.198-.298.298-.497.099-.198.05-.371-.025-.52-.075-.149-.669-1.612-.916-2.207-.242-.579-.487-.5-.669-.51-.173-.008-.371-.01-.57-.01-.198 0-.52.074-.792.372-.272.297-1.04 1.016-1.04 2.479 0 1.462 1.065 2.875 1.213 3.074.149.198 2.096 3.2 5.077 4.487.709.306 1.262.489 1.694.625.712.227 1.36.195 1.871.118.571-.085 1.758-.719 2.006-1.413.248-.694.248-1.289.173-1.413-.074-.124-.272-.198-.57-.347m-5.421 7.403h-.004a9.87 9.87 0 01-5.031-1.378l-.361-.214-3.741.982.998-3.648-.235-.374a9.86 9.86 0 01-1.51-5.26c.001-5.45 4.436-9.884 9.888-9.884 2.64 0 5.122 1.03 6.988 2.898a9.825 9.825 0 012.893 6.994c-.003 5.45-4.437 9.884-9.885 9.884m8.413-18.297A11.815 11.815 0 0012.05 0C5.495 0 .16 5.335.157 11.892c0 2.096.547 4.142 1.588 5.945L.057 24l6.305-1.654a11.882 11.882 0 005.683 1.448h.005c6.554 0 11.89-5.335 11.893-11.893a11.821 11.821 0 00-3.48-8.413Z\"/></svg>";

fn button(config: &Config, dutch: bool) -> String {
    let default_message = if dutch {
        "Hallo! Ik heb een vraag."
    } else {
        "Hi! I have a question."
    };
    let message = if config.message.is_empty() { default_message } else { &config.message };
    let href = format!("https://wa.me/{}?text={}", config.phone, url_encode(message));
    let name = if dutch { "Chat met ons via WhatsApp" } else { "Chat with us on WhatsApp" };
    let colour = &config.colour;
    let ink = if colour.eq_ignore_ascii_case(DEFAULT_COLOUR) { "#FFFFFF" } else { readable_on(colour) };
    let side = if config.left { "left" } else { "right" };
    let direction = if config.left { "row" } else { "row-reverse" };
    // The accessible name is the visible label when there is one (WCAG 2.5.3,
    // label in name) plus where it leads; otherwise the whole phrase, hidden.
    let label = if config.label.is_empty() {
        format!("<span class=\"swc-s\">{name}</span>")
    } else {
        format!(
            "<span class=\"swc-l\">{}</span><span class=\"swc-s\"> (WhatsApp)</span>",
            escape(&config.label)
        )
    };
    let pulse = if config.pulse {
        format!(
            "#swc .swc-b::after{{content:\"\";position:absolute;inset:0;border-radius:50%;box-shadow:0 0 0 0 {colour};\
animation:swc 2.4s ease-out 1s 3}}\
@keyframes swc{{0%{{box-shadow:0 0 0 0 {colour}99}}100%{{box-shadow:0 0 0 18px {colour}00}}}}\
@media (prefers-reduced-motion:reduce){{#swc .swc-b::after{{animation:none}}#swc,#swc .swc-b{{transition:none}}}}"
        )
    } else {
        String::new()
    };
    let (hidden, data, script) = match &config.hours {
        Some(hours) => (
            " hidden",
            format!(
                " data-days=\"{}\" data-open=\"{}\" data-close=\"{}\" data-tz=\"{}\"",
                hours.days,
                hours.open,
                hours.close,
                escape(&hours.timezone)
            ),
            format!("<script>{HOURS_SCRIPT}</script>"),
        ),
        None => ("", String::new(), String::new()),
    };
    format!(
        "<style>#swc{{position:fixed;z-index:2147482000;{side}:20px;bottom:20px;display:flex;flex-direction:{direction};\
align-items:center;gap:10px;text-decoration:none;font:600 15px/1.2 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif;\
color:#111827;-webkit-tap-highlight-color:transparent}}#swc[hidden]{{display:none}}\
#swc .swc-b{{position:relative;display:grid;place-items:center;width:58px;height:58px;border-radius:50%;flex:none;\
background:{colour};color:{ink};box-shadow:0 6px 20px rgba(0,0,0,.22);transition:transform .2s ease,box-shadow .2s ease}}\
#swc:hover .swc-b{{transform:translateY(-2px) scale(1.05);box-shadow:0 10px 26px rgba(0,0,0,.26)}}\
#swc .swc-l{{background:#fff;padding:10px 14px;border-radius:12px;box-shadow:0 4px 18px rgba(0,0,0,.14)}}\
#swc .swc-s{{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}}\
#swc:focus-visible{{outline:none}}#swc:focus-visible .swc-b{{outline:3px solid {colour};outline-offset:3px}}\
#swc:focus-visible .swc-l{{text-decoration:underline}}\
@media (max-width:480px){{#swc{{{side}:14px;bottom:14px}}#swc .swc-l{{display:none}}}}\
@media print{{#swc{{display:none}}}}{pulse}</style>\
<a id=\"swc\" href=\"{href}\" target=\"_blank\" rel=\"noopener noreferrer\"{data}{hidden}>\
<span class=\"swc-b\">{GLYPH}</span>{label}</a>{script}",
        href = escape(&href),
    )
}

#[plugin_fn]
pub fn panel_whatsapp_chat(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let error = |text: &str| Ok(Json(PanelResponse { error: text.to_owned(), ..PanelResponse::default() }));
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse { values: load_values(), ..PanelResponse::default() })),
        PanelEvent::Submit => {
            let mut values = defaults();
            for (key, value) in request.values {
                if values.contains_key(&key) {
                    values.insert(key, value);
                }
            }
            let text = |name: &str| {
                values.get(name).and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned()
            };
            let phone = text("phone");
            if !phone.is_empty() && normalise_phone(&phone).is_none() {
                return error(
                    "Enter the number in international format with its country code, such as +31 6 12345678.",
                );
            }
            let colour = text("colour");
            if !colour.is_empty() && !is_colour(&colour) {
                return error("The button colour must be a hex value such as #25D366.");
            }
            let hours_on = values.get("hours").and_then(JsonValue::as_bool).unwrap_or(false);
            if hours_on {
                match (parse_time(&text("open")), parse_time(&text("close"))) {
                    (Some(open), Some(close)) if open < close => {}
                    (Some(_), Some(_)) => return error("The closing time must be later than the opening time."),
                    _ => return error("Opening hours must be 24-hour times such as 09:00 and 17:30."),
                }
                if !is_timezone(&text("timezone")) {
                    return error("The time zone must look like Europe/Amsterdam.");
                }
            }
            if let Err(error) = kv::set(CONFIG, &values) {
                return Ok(Json(PanelResponse {
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so it cannot keep \
                         settings, and without a number the button stays hidden."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                    ..PanelResponse::default()
                }));
            }
            Ok(Json(PanelResponse {
                values,
                message: if phone.is_empty() {
                    "Saved. Add a WhatsApp number to show the button.".to_owned()
                } else {
                    "Saved. Republish or reload a page to see the button.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

/// Digits only, country code first, as wa.me wants it. Accepts `+31 6 ...`,
/// `0031 6 ...` and `316...`; refuses national numbers such as `06 ...`.
fn normalise_phone(input: &str) -> Option<String> {
    if input.chars().any(|c| !(c.is_ascii_digit() || " +-().".contains(c))) {
        return None;
    }
    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    let digits = if input.trim_start().starts_with('+') {
        digits
    } else if let Some(rest) = digits.strip_prefix("00") {
        rest.to_owned()
    } else if digits.starts_with('0') {
        return None;
    } else {
        digits
    };
    (8..=15).contains(&digits.len()).then_some(digits).filter(|d| !d.starts_with('0'))
}

/// `HH:MM`, 24-hour, as minutes after midnight. `24:00` means end of day.
fn parse_time(value: &str) -> Option<u32> {
    let (h, m) = value.split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    let minutes = h * 60 + m;
    (m < 60 && minutes <= 24 * 60).then_some(minutes)
}

fn is_timezone(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
}

fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `lang` on the `<html>` element starts with `nl`.
fn is_dutch(html: &str) -> bool {
    let Some(start) = find_ci(html, "<html") else { return false };
    let tag = &html[start..];
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    let lower = tag.to_ascii_lowercase();
    let Some(at) = lower.find("lang=") else { return false };
    lower[at + 5..].trim_start_matches(['"', '\'']).starts_with("nl")
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.as_bytes().windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn rfind_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.as_bytes().windows(needle.len()).rposition(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// `#RGB` or `#RRGGBB` only, so `{colour}99` in the CSS stays a valid colour.
fn is_colour(value: &str) -> bool {
    matches!(value.len(), 4 | 7) && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Dark or light ink, whichever reads better on `colour` (WCAG luminance).
fn readable_on(colour: &str) -> &'static str {
    let hex = &colour[1..];
    let channel = |i: usize| -> f64 {
        let v = if hex.len() == 3 {
            u8::from_str_radix(&hex[i..i + 1].repeat(2), 16)
        } else {
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
        }
        .unwrap_or(0) as f64
            / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
    // White when it meets the 3:1 asked of graphics (the brand look), else
    // whichever of white and near-black contrasts more.
    let white = 1.05 / (l + 0.05);
    if white >= 3.0 || white >= (l + 0.05) / 0.0586 { "#FFFFFF" } else { "#0F172A" }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
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

    fn with(pairs: &[(&str, JsonValue)]) -> Config {
        let mut values = defaults();
        for (k, v) in pairs {
            values.insert((*k).into(), v.clone());
        }
        Config::from_values(&values)
    }

    #[test]
    fn nothing_without_a_number() {
        let html = "<html><body></body></html>".to_owned();
        assert_eq!(render(html.clone(), &with(&[])), html);
    }

    #[test]
    fn injects_once_with_encoded_message() {
        let c = with(&[("phone", "+31 6 1234 5678".into()), ("message", "Hoi & \"hallo\"".into())]);
        let html = render("<html lang=\"nl\"><body><p>x</p></body></html>".into(), &c);
        assert!(html.contains("https://wa.me/31612345678?text=Hoi%20%26%20%22hallo%22"));
        assert!(html.contains("Chat met ons via WhatsApp"));
        assert!(html.ends_with("</a></body></html>"));
        assert_eq!(render(html.clone(), &c), html);
    }

    #[test]
    fn escapes_and_refuses_bad_values() {
        let c = with(&[
            ("phone", "+44 20 7946 0958".into()),
            ("label", "<b>hi</b>".into()),
            ("colour", "red;}".into()),
            ("hours", true.into()),
            ("timezone", "\"><script>".into()),
        ]);
        let html = render("<body></body>".into(), &c);
        assert!(html.contains("&lt;b&gt;hi"));
        assert!(!html.contains("red;}"));
        assert!(html.contains("data-tz=\"Europe/Amsterdam\""));
        assert!(html.contains(" hidden>"));
    }

    #[test]
    fn phones_and_times() {
        assert_eq!(normalise_phone("0031 6-12345678").as_deref(), Some("31612345678"));
        assert_eq!(normalise_phone("06 12345678"), None);
        assert_eq!(normalise_phone("+1 (555) 010-0199").as_deref(), Some("15550100199"));
        assert_eq!(normalise_phone("call me"), None);
        assert_eq!(parse_time("9:30"), Some(570));
        assert_eq!(parse_time("24:00"), Some(1440));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(readable_on("#1DA851"), "#FFFFFF");
        assert_eq!(readable_on("#25D366"), "#0F172A");
        assert_eq!(readable_on("#FFEB3B"), "#0F172A");
    }
}
