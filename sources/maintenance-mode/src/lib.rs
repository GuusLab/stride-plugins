//! Maintenance Mode: a coming-soon or maintenance page in front of the site.
//!
//! One permission (`storage`, for the panel's settings), one hook and one
//! panel. Without `storage` the curtain still goes up with its defaults: a
//! site owner who installed this to keep an unfinished site out of view must
//! never have it silently fall away. Disabling the plugin is the off switch.
//!
//! The hook has no idea who is asking (and the page is cacheable for a
//! minute), so the decision is made in the browser and it fails closed:
//!
//! * a `<style>` in `<head>` hides every child of `<body>` except the
//!   maintenance page, so without JavaScript, or with a failed request,
//!   visitors see the maintenance page and nothing else;
//! * a tiny script asks `system.whoami` on the same origin. Only when the
//!   answer is a signed-in account that may edit pages does it take the
//!   curtain down and show a small "maintenance mode is on" badge.
//!
//! It is a curtain, not access control: the page's HTML is still in the
//! source. The README says so.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};

/// All settings live under one key, so a render costs one host call.
const CONFIG: &str = "config";
/// Marks the injected block so a page is never given two curtains.
const MARKER: &str = "id=\"smm\"";

const DEFAULT_ACCENT: &str = "#64748B";

#[derive(Debug)]
struct Config {
    enabled: bool,
    maintenance: bool,
    light: bool,
    site_name: String,
    headline: String,
    message: String,
    launch: Option<Launch>,
    contact: String,
    accent: String,
    badge: bool,
}

/// A launch moment as typed in the panel: `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`,
/// in the visitor's local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Launch {
    year: u32,
    month: u32,
    day: u32,
    time: Option<(u32, u32)>,
}

impl Launch {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (date, time) = match text.split_once([' ', 'T']) {
            Some((date, time)) => (date, Some(time.trim())),
            None => (text, None),
        };
        let mut parts = date.split('-');
        let year = number(parts.next()?, 4)?;
        let month = number(parts.next()?, 2)?;
        let day = number(parts.next()?, 2)?;
        if parts.next().is_some() || !(2000..=2999).contains(&year) || !(1..=12).contains(&month) {
            return None;
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            2 if leap => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        if !(1..=days).contains(&day) {
            return None;
        }
        let time = match time {
            None | Some("") => None,
            Some(time) => {
                let (h, m) = time.split_once(':')?;
                let (h, m) = (number(h, 2)?, number(m, 2)?);
                if h > 23 || m > 59 {
                    return None;
                }
                Some((h, m))
            }
        };
        Some(Launch { year, month, day, time })
    }

    /// What JavaScript's `Date` reads as local time. Always with a time: a
    /// bare date would be read as UTC midnight.
    fn iso(&self) -> String {
        let (h, m) = self.time.unwrap_or((0, 0));
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:00", self.year, self.month, self.day, h, m)
    }

    fn human(&self, dutch: bool) -> String {
        const EN: [&str; 12] = [
            "January", "February", "March", "April", "May", "June", "July", "August", "September",
            "October", "November", "December",
        ];
        const NL: [&str; 12] = [
            "januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus",
            "september", "oktober", "november", "december",
        ];
        let month = if dutch { NL } else { EN }[self.month as usize - 1];
        let date = format!("{} {month} {}", self.day, self.year);
        match self.time {
            Some((h, m)) => format!("{date} {} {h:02}:{m:02}", if dutch { "om" } else { "at" }),
            None => date,
        }
    }
}

fn number(text: &str, width: usize) -> Option<u32> {
    (text.len() == width && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok())?
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let text = |name: &str| {
            values.get(name).and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned()
        };
        let flag = |name: &str| values.get(name).and_then(JsonValue::as_bool).unwrap_or(true);
        let accent = text("accent");
        Config {
            enabled: flag("enabled"),
            maintenance: text("mode") == "maintenance",
            light: text("theme") == "light",
            site_name: text("site-name"),
            headline: text("headline"),
            message: text("message"),
            launch: Launch::parse(&text("launch")),
            contact: text("contact"),
            accent: if is_colour(&accent) { accent } else { DEFAULT_ACCENT.to_owned() },
            badge: flag("badge"),
        }
    }

    fn defaults() -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), true.into());
        values.insert("mode".into(), "coming-soon".into());
        values.insert("site-name".into(), "".into());
        values.insert("headline".into(), "".into());
        values.insert("message".into(), "".into());
        values.insert("launch".into(), "".into());
        values.insert("contact".into(), "".into());
        values.insert("theme".into(), "dark".into());
        values.insert("accent".into(), DEFAULT_ACCENT.into());
        values.insert("badge".into(), true.into());
        values
    }
}

/// The stored settings, or the defaults when there are none or `storage` was
/// refused. Never fails.
fn load_values() -> Map<String, JsonValue> {
    let mut values = Config::defaults();
    match kv::get::<Map<String, JsonValue>>(CONFIG) {
        Ok(Some(stored)) => {
            for (key, value) in stored {
                if values.contains_key(&key) {
                    values.insert(key, value);
                }
            }
        }
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
    if !config.enabled || html.contains(MARKER) {
        return html;
    }
    let dutch = is_dutch(&html);
    let (style, body) = curtain(config, dutch);
    let head = format!("{HEAD}{style}");
    let mut html = html;
    // The hiding rule belongs in <head>, so nothing flashes before it applies.
    let mut in_head = false;
    if let Some(at) = find_ci(&html, "</head") {
        html.insert_str(at, &head);
        in_head = true;
    }
    let block = if in_head { body } else { format!("{head}{body}") };
    match find_ci(&html, "<body").and_then(|at| html[at..].find('>').map(|close| at + close + 1)) {
        Some(at) => {
            html.insert_str(at, &block);
            html
        }
        None => format!("{block}{html}"),
    }
}

/// Hides everything but the curtain until the script says otherwise. `!important`
/// because a theme's own `display` must not win.
const HEAD: &str = "<meta name=\"robots\" content=\"noindex\" id=\"smm-r\">\
<style id=\"smm-s\">html body>*:not(#smm):not(#smm-e){display:none!important}html,body{overflow:hidden!important}</style>";

struct Words {
    soon_headline: &'static str,
    soon_message: &'static str,
    soon_pill: &'static str,
    soon_launch: &'static str,
    back_headline: &'static str,
    back_message: &'static str,
    back_pill: &'static str,
    back_launch: &'static str,
    units: [&'static str; 4],
    countdown: &'static str,
    contact: &'static str,
    badge: &'static str,
}

const EN: Words = Words {
    soon_headline: "Something new is on its way",
    soon_message: "We are putting the finishing touches on our new website. Check back soon.",
    soon_pill: "Coming soon",
    soon_launch: "Launching",
    back_headline: "We\u{2019}ll be right back",
    back_message: "We are doing some scheduled maintenance. Thank you for your patience.",
    back_pill: "Under maintenance",
    back_launch: "Back",
    units: ["days", "hours", "minutes", "seconds"],
    countdown: "Time remaining",
    contact: "Questions?",
    badge: "Maintenance mode is on. Visitors see the maintenance page.",
};

const NL: Words = Words {
    soon_headline: "Er komt iets moois aan",
    soon_message: "We leggen de laatste hand aan onze nieuwe website. Kom snel terug.",
    soon_pill: "Binnenkort online",
    soon_launch: "Online op",
    back_headline: "We zijn zo terug",
    back_message: "We voeren gepland onderhoud uit. Bedankt voor je geduld.",
    back_pill: "Onderhoud",
    back_launch: "Terug op",
    units: ["dagen", "uur", "minuten", "seconden"],
    countdown: "Resterende tijd",
    contact: "Vragen?",
    badge: "Onderhoudsmodus staat aan. Bezoekers zien de onderhoudspagina.",
};

/// Countdown plus the editor check. No interpolation into it at all: the
/// launch moment arrives through an escaped data attribute.
/// The editors' badge. Plain CSS, no settings in it: the curtain's own
/// `<style>` is removed when the badge appears, so this one travels with it.
const BADGE_CSS: &str = "#smm-e{position:fixed;z-index:2147483000;left:16px;bottom:16px;display:inline-flex;align-items:center;\
gap:10px;max-width:calc(100% - 32px);padding:10px 16px;border-radius:999px;background:#0F172A;color:#F8FAFC;text-decoration:none;\
font:600 13px/1.3 system-ui,-apple-system,\"Segoe UI\",sans-serif;box-shadow:0 8px 24px rgba(2,6,23,.3)}#smm-e[hidden]{display:none}\
#smm-e i{flex:none;width:8px;height:8px;border-radius:50%;background:#F59E0B;box-shadow:0 0 0 4px rgba(245,158,11,.3)}\
#smm-e:focus-visible{outline:3px solid #F59E0B;outline-offset:3px}";

const SCRIPT: &str = r#"(function(){var d=document,m=d.getElementById('smm');if(!m)return;var t=m.getAttribute('data-launch'),c=d.getElementById('smm-c');if(t&&c){var e=new Date(t).getTime(),b=c.getElementsByTagName('b');var f=function(){var s=Math.max(0,Math.floor((e-Date.now())/1e3));if(!s){c.hidden=true;return}var v=[s/86400|0,s/3600%24|0,s/60%60|0,s%60];for(var i=0;i<4;i++)b[i].textContent=(i&&v[i]<10?'0':'')+v[i];setTimeout(f,1e3-Date.now()%1e3)};if(e>Date.now()){c.hidden=false;f()}}if(!window.fetch)return;fetch('/api/actions/system.whoami',{method:'POST',credentials:'same-origin',headers:{'content-type':'application/json'},body:'{}'}).then(function(r){return r.ok?r.json():null}).then(function(w){if(!w||w.id=='anonymous'||!(w.scopes||[]).some(function(s){return s=='*'||s.indexOf('documents:')==0}))return;['smm-s','smm-r','smm'].forEach(function(i){var x=d.getElementById(i);if(x)x.parentNode.removeChild(x)});var g=d.getElementById('smm-e');if(g)g.hidden=false}).catch(function(){})})();"#;

/// The stylesheet, for `<head>`, and the markup, for `<body>`.
fn curtain(config: &Config, dutch: bool) -> (String, String) {
    let w = if dutch { &NL } else { &EN };
    let (headline, message, pill, launch_word) = if config.maintenance {
        (w.back_headline, w.back_message, w.back_pill, w.back_launch)
    } else {
        (w.soon_headline, w.soon_message, w.soon_pill, w.soon_launch)
    };
    let headline = if config.headline.is_empty() { headline.to_owned() } else { escape(&config.headline) };
    let message = if config.message.is_empty() {
        format!("<p>{message}</p>")
    } else {
        config
            .message
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| format!("<p>{}</p>", escape(p).replace('\n', "<br>")))
            .collect()
    };
    let brand = if config.site_name.is_empty() {
        String::new()
    } else {
        format!("<p class=\"smm-n\">{}</p>", escape(&config.site_name))
    };
    let (launch_attr, launch_html) = match config.launch {
        Some(launch) => {
            let tiles: String = w
                .units
                .iter()
                .map(|unit| format!("<span><b>00</b><small>{unit}</small></span>"))
                .collect();
            (
                format!(" data-launch=\"{}\"", launch.iso()),
                format!(
                    "<p class=\"smm-l\">{launch_word} {}</p>\
<div id=\"smm-c\" class=\"smm-c\" role=\"timer\" aria-label=\"{}\" hidden>{tiles}</div>",
                    launch.human(dutch),
                    w.countdown
                ),
            )
        }
        None => (String::new(), String::new()),
    };
    let contact = if config.contact.is_empty() {
        String::new()
    } else if is_email(&config.contact) {
        let email = escape(&config.contact);
        format!("<p class=\"smm-k\">{} <a href=\"mailto:{email}\">{email}</a></p>", w.contact)
    } else {
        format!("<p class=\"smm-k\">{}</p>", escape(&config.contact))
    };
    let badge = if config.badge {
        format!(
            "<a id=\"smm-e\" href=\"/admin/\" hidden><i aria-hidden=\"true\"></i>{}</a>",
            w.badge
        )
    } else {
        String::new()
    };
    let accent = &config.accent;
    let (glow, bg, card, fg, muted, line, tile) = if config.light {
        ("4D", "#F8FAFC", "#FFFFFF", "#0F172A", "#475569", "rgba(15,23,42,.1)", "#F1F5F9")
    } else {
        ("73", "#0B1120", "rgba(255,255,255,.04)", "#F8FAFC", "#CBD5E1", "rgba(255,255,255,.1)", "rgba(255,255,255,.06)")
    };
    // The accent is a decoration (the glow, the dot, the link underline);
    // every piece of text keeps its AA-contrast colour whatever is picked.
    let style = format!(
        "<style>\
#smm{{position:fixed;inset:0;z-index:2147483000;overflow:auto;display:flex;align-items:center;justify-content:center;\
padding:32px 16px;box-sizing:border-box;background:{bg};color:{fg};\
background-image:radial-gradient(60rem 40rem at 15% -10%,{accent}{glow},transparent 60%),radial-gradient(50rem 36rem at 110% 110%,{accent}{glow},transparent 60%);\
font:17px/1.6 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif;text-align:center}}\
#smm *{{box-sizing:border-box}}#smm [hidden]{{display:none}}\
#smm .smm-w{{width:100%;max-width:640px;padding:48px 32px;border:1px solid {line};border-radius:28px;background:{card};\
box-shadow:0 30px 80px rgba(2,6,23,.25)}}\
#smm .smm-n{{margin:0 0 20px;font-weight:700;letter-spacing:.02em;font-size:15px;color:{muted}}}\
#smm .smm-p{{display:inline-flex;align-items:center;gap:8px;margin:0 0 20px;padding:6px 14px;border-radius:999px;\
border:1px solid {line};font-size:13px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:{muted}}}\
#smm .smm-p i{{width:8px;height:8px;border-radius:50%;background:{accent};box-shadow:0 0 0 4px {accent}59}}\
#smm h1{{margin:0 0 16px;font-size:clamp(32px,6vw,52px);line-height:1.1;font-weight:800;letter-spacing:-.02em;color:inherit}}\
#smm p{{margin:0 0 12px}}#smm .smm-m{{color:{muted};max-width:32em;margin:0 auto}}\
#smm .smm-l{{margin:28px 0 0;font-weight:600}}\
#smm .smm-c{{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin:16px auto 0;max-width:440px}}\
#smm .smm-c span{{display:flex;flex-direction:column;padding:14px 4px;border-radius:16px;background:{tile};border:1px solid {line}}}\
#smm .smm-c b{{font-size:clamp(24px,5vw,34px);line-height:1.1;font-weight:750;font-variant-numeric:tabular-nums}}\
#smm .smm-c small{{font-size:12px;color:{muted};text-transform:uppercase;letter-spacing:.06em}}\
#smm .smm-k{{margin:28px 0 0;padding-top:24px;border-top:1px solid {line};color:{muted};font-size:15px}}\
#smm a{{color:{fg};font-weight:600;text-decoration:underline;text-decoration-color:{accent};text-decoration-thickness:2px;text-underline-offset:3px}}\
#smm a:focus-visible{{outline:3px solid {accent};outline-offset:3px;border-radius:4px}}\
@media (max-width:520px){{#smm .smm-w{{padding:32px 16px;border-radius:22px}}#smm .smm-c{{gap:6px}}#smm .smm-c span{{padding:12px 2px;border-radius:12px}}#smm .smm-c small{{font-size:10px;letter-spacing:.02em}}}}\
@media (prefers-reduced-motion:no-preference){{#smm .smm-w{{animation:smm-in .6s ease-out both}}\
@keyframes smm-in{{from{{opacity:0;transform:translateY(12px)}}}}}}\
{BADGE_CSS}</style>"
    );
    let body = format!(
        "<main id=\"smm\" aria-labelledby=\"smm-h\"{launch_attr}><div class=\"smm-w\">{brand}<p class=\"smm-p\"><i aria-hidden=\"true\"></i>{pill}</p>\
<h1 id=\"smm-h\">{headline}</h1><div class=\"smm-m\">{message}</div>{launch_html}{contact}</div></main>\
{badge}<script>{SCRIPT}</script>"
    );
    (style, body)
}

#[plugin_fn]
pub fn panel_maintenance_mode(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse { values: load_values(), ..PanelResponse::default() })),
        PanelEvent::Submit => {
            let mut values = Config::defaults();
            for (key, value) in request.values {
                if values.contains_key(&key) {
                    values.insert(key, value);
                }
            }
            let error = |text: &str| Ok(Json(PanelResponse { error: text.to_owned(), ..PanelResponse::default() }));
            let text = |name: &str| values.get(name).and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned();
            let accent = text("accent");
            if !accent.is_empty() && !is_colour(&accent) {
                return error("The accent colour must be a hex value such as #64748B.");
            }
            let launch = text("launch");
            if !launch.is_empty() && Launch::parse(&launch).is_none() {
                return error(
                    "Write the launch date as 2026-11-01, or with a time as 2026-11-01 09:00. \
                     Leave it empty for no countdown.",
                );
            }
            if let Err(error) = kv::set(CONFIG, &values) {
                return Ok(Json(PanelResponse {
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so it cannot keep \
                         settings. Visitors still see the maintenance page, with its default \
                         text. Disable the plugin to take it down."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                    ..PanelResponse::default()
                }));
            }
            let on = values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(true);
            Ok(Json(PanelResponse {
                values,
                message: if on {
                    "Saved. Visitors now see the maintenance page; signed-in editors still see the \
                     real site."
                        .to_owned()
                } else {
                    "Saved. Maintenance mode is off and the site is open to everyone.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
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

fn is_colour(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Strict enough to put behind `mailto:`: one `@`, a dotted domain, and no
/// character that could end an attribute or start a query.
fn is_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else { return false };
    let ok = |c: char| c.is_ascii_alphanumeric() || "._+-".contains(c);
    !local.is_empty()
        && local.chars().all(ok)
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
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

    fn config() -> Config {
        Config::from_values(&Config::defaults())
    }

    const PAGE: &str = "<!doctype html><html lang=\"en\"><head><title>x</title></head><body><p>x</p></body></html>";

    #[test]
    fn injects_once_and_fails_closed() {
        let html = render(PAGE.into(), &config());
        assert!(html.contains("Something new is on its way"));
        let style = html.find("id=\"smm-s\"").unwrap();
        assert!(style < html.find("</head>").unwrap());
        assert!(html.find("<main id=\"smm\"").unwrap() > html.find("<body>").unwrap());
        assert!(html.contains("noindex"));
        assert_eq!(render(html.clone(), &config()), html);
    }

    #[test]
    fn off_leaves_page_alone() {
        let mut values = Config::defaults();
        values.insert("enabled".into(), false.into());
        assert_eq!(render(PAGE.into(), &Config::from_values(&values)), PAGE);
    }

    #[test]
    fn dutch_and_maintenance() {
        let mut values = Config::defaults();
        values.insert("mode".into(), "maintenance".into());
        let html = render("<html lang='nl-NL'><body></body></html>".into(), &Config::from_values(&values));
        assert!(html.contains("We zijn zo terug"));
    }

    #[test]
    fn escapes_and_refuses_bad_values() {
        let mut values = Config::defaults();
        values.insert("headline".into(), "<b>hi</b>".into());
        values.insert("contact".into(), "a\"@x.com".into());
        values.insert("accent".into(), "red;}".into());
        values.insert("launch".into(), "2026-02-30".into());
        let html = render("<body></body>".into(), &Config::from_values(&values));
        assert!(html.contains("&lt;b&gt;hi"));
        assert!(!html.contains("mailto:"));
        assert!(html.contains("a&quot;@x.com"));
        assert!(!html.contains("red;}"));
        assert!(!html.contains("data-launch=\""));
    }

    #[test]
    fn launch_and_contact() {
        let mut values = Config::defaults();
        values.insert("launch".into(), "2026-11-01 09:30".into());
        values.insert("contact".into(), "hello@atelier.example".into());
        let html = render("<body></body>".into(), &Config::from_values(&values));
        assert!(html.contains("data-launch=\"2026-11-01T09:30:00\""));
        assert!(html.contains("Launching 1 November 2026 at 09:30"));
        assert!(html.contains("href=\"mailto:hello@atelier.example\""));
        assert_eq!(Launch::parse("2024-02-29").map(|l| l.day), Some(29));
        assert!(Launch::parse("2025-02-29").is_none());
        assert!(Launch::parse("2026-1-01").is_none());
    }
}
