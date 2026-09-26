//! Countdown Banner: a bar that counts down to a deadline — a sale, a launch,
//! the last day to order before the holidays — and disappears (or says it has
//! ended) once the moment has passed.
//!
//! Pages are rendered when they are published, not when they are visited, so
//! the counting happens in the visitor's browser. Without JavaScript the bar
//! still shows the message and the date it ends.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 12] = [
    "enabled", "message", "ends", "timezone", "link-text", "link-url", "after", "ended-message",
    "language", "position", "colour", "skip-pages",
];
const MARKER: &str = "id=\"stride-countdown\"";

/// Enum values must be slugs, so the time zone names are mapped here.
const ZONES: [(&str, &str, &str); 12] = [
    ("europe-amsterdam", "Europe/Amsterdam", "Amsterdam, Brussels, Berlin, Paris"),
    ("europe-london", "Europe/London", "London, Dublin, Lisbon"),
    ("europe-helsinki", "Europe/Helsinki", "Helsinki, Athens, Kyiv"),
    ("europe-istanbul", "Europe/Istanbul", "Istanbul"),
    ("utc", "UTC", "UTC"),
    ("america-new-york", "America/New_York", "New York, Toronto"),
    ("america-chicago", "America/Chicago", "Chicago, Mexico City"),
    ("america-denver", "America/Denver", "Denver"),
    ("america-los-angeles", "America/Los_Angeles", "Los Angeles, Vancouver"),
    ("asia-dubai", "Asia/Dubai", "Dubai"),
    ("asia-singapore", "Asia/Singapore", "Singapore, Hong Kong"),
    ("australia-sydney", "Australia/Sydney", "Sydney, Melbourne"),
];

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    message: String,
    /// `YYYY-MM-DD HH:MM`, wall time in `timezone`.
    ends: String,
    timezone: String,
    link_text: String,
    link_url: String,
    /// `hide` or `message`.
    after: String,
    ended_message: String,
    /// `en`, `nl`, `de` or `fr`.
    language: String,
    /// `top` or `bottom`.
    position: String,
    colour: String,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            message: String::new(),
            ends: String::new(),
            timezone: "europe-amsterdam".into(),
            link_text: String::new(),
            link_url: String::new(),
            after: "hide".into(),
            ended_message: String::new(),
            language: "en".into(),
            position: "top".into(),
            colour: "#111827".into(),
            skip_pages: Vec::new(),
        }
    }
}

fn one_of(value: String, allowed: &[&str], fallback: String) -> String {
    if allowed.contains(&value.as_str()) { value } else { fallback }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let zones: Vec<&str> = ZONES.iter().map(|z| z.0).collect();
        let colour = text(values, "colour").to_ascii_lowercase();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            message: text(values, "message"),
            ends: text(values, "ends"),
            timezone: one_of(text(values, "timezone"), &zones, d.timezone),
            link_text: text(values, "link-text"),
            link_url: text(values, "link-url"),
            after: one_of(text(values, "after"), &["hide", "message"], d.after),
            ended_message: text(values, "ended-message"),
            language: one_of(text(values, "language"), &["en", "nl", "de", "fr"], d.language),
            position: one_of(text(values, "position"), &["top", "bottom"], d.position),
            colour: if is_colour(&colour) { colour } else { d.colour },
            skip_pages: slug_list(&text(values, "skip-pages")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("message".into(), self.message.clone().into());
        values.insert("ends".into(), self.ends.clone().into());
        values.insert("timezone".into(), self.timezone.clone().into());
        values.insert("link-text".into(), self.link_text.clone().into());
        values.insert("link-url".into(), self.link_url.clone().into());
        values.insert("after".into(), self.after.clone().into());
        values.insert("ended-message".into(), self.ended_message.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("position".into(), self.position.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
    }

    fn zone(&self) -> &'static str {
        ZONES.iter().find(|z| z.0 == self.timezone).map(|z| z.1).unwrap_or("Europe/Amsterdam")
    }
}

/// `2026-12-24 18:00` (or `2026-12-24`, meaning midnight at its start is not
/// what anybody means: a date alone ends at 23:59).
fn parse_deadline(text: &str) -> Option<[u32; 5]> {
    let text = text.trim().replace('T', " ");
    let (date, time) = match text.split_once(' ') {
        Some((d, t)) => (d.trim(), t.trim()),
        None => (text.as_str(), "23:59"),
    };
    let d: Vec<u32> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let t: Vec<u32> = time.split([':', '.']).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if d.len() != 3 || !(t.len() == 2 || t.len() == 3) {
        return None;
    }
    let (year, month, day, hour, minute) = (d[0], d[1], d[2], t[0], t[1]);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let ok = (2000..=2100).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days[(month - 1) as usize]
        && hour < 24
        && minute < 60;
    ok.then_some([year, month, day, hour, minute])
}

const MONTHS: [[&str; 12]; 4] = [
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    ["januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus", "september", "oktober", "november", "december"],
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
];

struct Words {
    units: [&'static str; 4],
    ends: &'static str,
}

fn words(language: &str) -> (usize, Words) {
    match language {
        "nl" => (1, Words { units: ["dagen", "uur", "min", "sec"], ends: "Eindigt" }),
        "de" => (2, Words { units: ["Tage", "Std", "Min", "Sek"], ends: "Endet am" }),
        "fr" => (3, Words { units: ["jours", "h", "min", "s"], ends: "Se termine le" }),
        _ => (0, Words { units: ["days", "hrs", "min", "sec"], ends: "Ends" }),
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
            stride_pdk::log("warn", &format!("no countdown shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &Config::from_values(&values)) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    let Some([y, mo, d, h, mi]) = parse_deadline(&config.ends) else { return html };
    if !config.enabled || config.message.is_empty() || slug_matches(&config.skip_pages, slug) || html.contains(MARKER) {
        return html;
    }
    let (index, words) = words(&config.language);
    let fg = readable_on(&config.colour);
    let when = format!("{d} {} {y}, {h:02}:{mi:02}", MONTHS[index][(mo - 1) as usize]);
    let link = match safe_url(&config.link_url) {
        Some(url) if !config.link_text.is_empty() => {
            format!("<a class=\"scd-cta\" href=\"{}\">{}</a>", escape(&url), escape(&config.link_text))
        }
        _ => String::new(),
    };
    let unit = |i: usize, key: &str| {
        format!("<span class=\"scd-unit\"><b data-scd=\"{key}\">–</b><small>{}</small></span>", words.units[i])
    };
    let bar = format!(
        "<div {MARKER} class=\"scd-{pos}\" role=\"region\" aria-label=\"Countdown\" data-end=\"{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}\" \
         data-zone=\"{zone}\" data-after=\"{after}\"><div class=\"scd-in\">\
         <p class=\"scd-msg\">{msg}</p><p class=\"scd-ended\" hidden>{ended}</p>\
         <p class=\"scd-clock\" aria-hidden=\"true\" hidden>{u0}{u1}{u2}{u3}</p>\
         <p class=\"scd-when\">{ends} {when}</p>{link}</div></div>",
        pos = config.position,
        zone = config.zone(),
        after = config.after,
        msg = escape(&config.message),
        ended = escape(&config.ended_message),
        u0 = unit(0, "d"),
        u1 = unit(1, "h"),
        u2 = unit(2, "m"),
        u3 = unit(3, "s"),
        ends = words.ends,
    );
    let cta_bg = if fg == "#fff" { "#fff" } else { "#111827" };
    let cta_fg = config.colour.as_str();
    let place = if config.position == "bottom" {
        "position:fixed;left:16px;right:16px;bottom:16px;z-index:2147483000;border-radius:14px;box-shadow:0 12px 40px rgba(0,0,0,.25)"
    } else {
        "position:relative"
    };
    let style = format!(
        "<style>#stride-countdown{{{place};background:{bg};color:{fg};\
         font:500 15px/1.35 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         #stride-countdown[hidden]{{display:none}}\
         #stride-countdown .scd-in{{max-width:72rem;margin:0 auto;padding:10px 16px;display:flex;flex-wrap:wrap;\
         align-items:center;justify-content:center;gap:8px 18px}}\
         #stride-countdown p{{margin:0}}.scd-msg,.scd-ended{{font-weight:650}}\
         #stride-countdown .scd-clock{{display:flex;gap:6px}}#stride-countdown .scd-clock[hidden],#stride-countdown [hidden]{{display:none}}\
         .scd-unit{{display:inline-flex;align-items:baseline;gap:3px;padding:3px 8px;border-radius:7px;\
         background:rgba(127,127,127,.22)}}\
         .scd-unit b{{font:700 17px/1.2 ui-monospace,\"SF Mono\",Menlo,monospace;font-variant-numeric:tabular-nums;\
         min-width:2ch;text-align:right}}.scd-unit small{{font-size:12px;opacity:.85}}\
         .scd-when{{font-size:13px;opacity:.85}}.scd-live .scd-when{{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}}\
         .scd-cta{{display:inline-block;padding:6px 14px;border-radius:999px;background:{cta_bg};color:{cta_fg};\
         font-weight:650;text-decoration:none}}.scd-cta:hover{{text-decoration:underline}}\
         .scd-cta:focus-visible{{outline:2px solid currentColor;outline-offset:3px}}\
         @media (max-width:600px){{#stride-countdown .scd-in{{justify-content:flex-start}}}}</style>",
        bg = config.colour,
    );
    let script = "<script>(function(){var b=document.getElementById('stride-countdown');if(!b)return;\
        var p=b.dataset.end.split(/[-T:]/).map(Number),z=b.dataset.zone;\
        function off(t){try{var f=new Intl.DateTimeFormat('en-US',{timeZone:z,hourCycle:'h23',year:'numeric',month:'numeric',\
        day:'numeric',hour:'numeric',minute:'numeric',second:'numeric'}).formatToParts(new Date(t)),o={};\
        f.forEach(function(x){o[x.type]=+x.value});return Date.UTC(o.year,o.month-1,o.day,o.hour%24,o.minute,o.second)-t}\
        catch(e){return -new Date(t).getTimezoneOffset()*60000}}\
        var wall=Date.UTC(p[0],p[1]-1,p[2],p[3],p[4]),end=wall-off(wall);end=wall-off(end);\
        var clock=b.querySelector('.scd-clock'),els={};['d','h','m','s'].forEach(function(k){els[k]=b.querySelector('[data-scd='+k+']')});\
        function pad(n){return n<10?'0'+n:''+n}\
        function tick(){var left=end-Date.now();if(left<=0){clearInterval(timer);\
        if(b.dataset.after==='message'&&b.querySelector('.scd-ended').textContent){b.querySelector('.scd-msg').hidden=true;\
        clock.hidden=true;b.querySelector('.scd-when').hidden=true;var c=b.querySelector('.scd-cta');if(c)c.hidden=true;\
        b.querySelector('.scd-ended').hidden=false}else b.hidden=true;return}\
        var s=Math.floor(left/1000);els.d.textContent=Math.floor(s/86400);els.h.textContent=pad(Math.floor(s%86400/3600));\
        els.m.textContent=pad(Math.floor(s%3600/60));els.s.textContent=pad(s%60);clock.hidden=false;b.classList.add('scd-live')}\
        var timer=setInterval(tick,1000);tick()})()</script>";
    let mut html = html;
    insert_in_head(&mut html, &style);
    if config.position == "bottom" {
        insert_before_body_end(&mut html, &format!("{bar}{script}"));
    } else {
        insert_after_body_start(&mut html, &bar);
        insert_before_body_end(&mut html, script);
    }
    html
}

#[plugin_fn]
pub fn panel_countdown_banner(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: &str| Ok(Json(PanelResponse { error: error.to_owned(), ..PanelResponse::default() }));
            if !config.ends.is_empty() && parse_deadline(&config.ends).is_none() {
                return refuse("Write the end as year-month-day and time, for example 2026-12-24 18:00.");
            }
            if !config.message.is_empty() && config.ends.is_empty() {
                return refuse("Add the date and time the countdown ends.");
            }
            if !config.link_url.is_empty() && safe_url(&config.link_url).is_none() {
                return refuse("The button link must start with https://, http://, mailto:, tel:, / or #.");
            }
            if !config.link_url.is_empty() && config.link_text.is_empty() {
                return refuse("Add a button text for the link, or clear the link.");
            }
            if config.after == "message" && config.ended_message.is_empty() {
                return refuse("Write the message to show after the end, or choose to hide the bar.");
            }
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(&if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere to keep the countdown.".to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: if config.enabled && !config.message.is_empty() {
                    "Saved. Publish your pages again to show the countdown.".to_owned()
                } else {
                    "Saved. No countdown is shown.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head></head><body class=\"x\"><main>Hi</main></body></html>";

    fn config() -> Config {
        Config { message: "Summer sale ends in".into(), ends: "2026-08-31 18:00".into(), ..Config::default() }
    }

    #[test]
    fn deadlines() {
        assert_eq!(parse_deadline("2026-12-24 18:00"), Some([2026, 12, 24, 18, 0]));
        assert_eq!(parse_deadline("2026-12-24T09:30"), Some([2026, 12, 24, 9, 30]));
        assert_eq!(parse_deadline("2026-12-24"), Some([2026, 12, 24, 23, 59]));
        assert_eq!(parse_deadline("2027-02-29 10:00"), None);
        assert_eq!(parse_deadline("2028-02-29 10:00"), Some([2028, 2, 29, 10, 0]));
        assert_eq!(parse_deadline("24-12-2026 18:00"), None);
        assert_eq!(parse_deadline("next friday"), None);
    }

    #[test]
    fn top_bar_after_body_with_fallback_date() {
        let out = render(PAGE.into(), "home", &config());
        let bar = out.find("<div id=\"stride-countdown\" class=\"scd-top\"").unwrap();
        assert!(bar > out.find("<body class=\"x\">").unwrap() && bar < out.find("<main>").unwrap());
        assert!(out.contains("data-end=\"2026-08-31T18:00\" data-zone=\"Europe/Amsterdam\""));
        assert!(out.contains("Ends 31 August 2026, 18:00"));
        assert_eq!(render(out.clone(), "home", &config()), out);
    }

    #[test]
    fn dutch_bottom_with_escaped_text_and_safe_link() {
        let mut c = config();
        c.language = "nl".into();
        c.position = "bottom".into();
        c.message = "<b>Uitverkoop</b>".into();
        c.link_text = "Shop nu".into();
        c.link_url = "javascript:alert(1)".into();
        let out = render(PAGE.into(), "home", &c);
        assert!(out.contains("&lt;b&gt;Uitverkoop&lt;/b&gt;"));
        assert!(out.contains("Eindigt 31 augustus 2026, 18:00"));
        assert!(!out.contains("javascript:"));
        assert!(out.find("stride-countdown\" class=\"scd-bottom\"").unwrap() > out.find("<main>").unwrap());
    }

    #[test]
    fn nothing_without_message_or_valid_date() {
        assert_eq!(render(PAGE.into(), "home", &Config::default()), PAGE);
        let bad = Config { ends: "soon".into(), ..config() };
        assert_eq!(render(PAGE.into(), "home", &bad), PAGE);
    }

    #[test]
    fn unknown_settings_fall_back() {
        let mut values = config().to_values();
        values.insert("timezone".into(), "mars".into());
        values.insert("colour".into(), "red".into());
        let c = Config::from_values(&values);
        assert_eq!(c.zone(), "Europe/Amsterdam");
        assert_eq!(c.colour, "#111827");
    }
}
