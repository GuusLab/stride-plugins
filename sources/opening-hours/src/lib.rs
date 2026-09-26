//! Opening Hours: a table of opening hours and an "Open now · closes at
//! 17:00" badge, placed where an editor types `[opening-hours]` or
//! `[open-now]` in a page, and optionally in the footer of every page.
//!
//! The table is plain HTML and works without JavaScript. Whether the business
//! is open *right now* depends on when the page is visited, not when it was
//! published, so a few hundred bytes of script work that out in the
//! business's own time zone and highlight today.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
const FIELDS: [&str; 13] = [
    "enabled", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "note", "timezone",
    "language", "footer", "colour",
];
const TABLE_CODE: &str = "[opening-hours]";
const BADGE_CODE: &str = "[open-now]";
const MARKER: &str = "data-soh";
const ZONES: [(&str, &str); 8] = [
    ("europe-amsterdam", "Europe/Amsterdam"),
    ("europe-london", "Europe/London"),
    ("europe-helsinki", "Europe/Helsinki"),
    ("utc", "UTC"),
    ("america-new-york", "America/New_York"),
    ("america-chicago", "America/Chicago"),
    ("america-los-angeles", "America/Los_Angeles"),
    ("australia-sydney", "Australia/Sydney"),
];

/// Minutes since midnight: `[(540, 1020)]` is 9:00 to 17:00.
type Ranges = Vec<(u32, u32)>;

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    days: [String; 7],
    note: String,
    timezone: String,
    language: String,
    /// `none`, `badge` or `table`.
    footer: String,
    colour: String,
}

impl Default for Config {
    fn default() -> Self {
        let open = || "09:00-17:00".to_owned();
        Config {
            enabled: true,
            days: [open(), open(), open(), open(), open(), "10:00-16:00".into(), "closed".into()],
            note: String::new(),
            timezone: "europe-amsterdam".into(),
            language: "en".into(),
            footer: "none".into(),
            colour: "#15803d".into(),
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let pick = |name: &str, allowed: &[&str], fallback: &str| {
            let v = text(values, name);
            if allowed.contains(&v.as_str()) { v } else { fallback.to_owned() }
        };
        let zones: Vec<&str> = ZONES.iter().map(|z| z.0).collect();
        let colour = text(values, "colour").to_ascii_lowercase();
        let mut days = d.days.clone();
        for (i, day) in DAYS.iter().enumerate() {
            if values.contains_key(*day) {
                days[i] = text(values, day);
            }
        }
        Config {
            enabled: flag(values, "enabled", d.enabled),
            days,
            note: text(values, "note"),
            timezone: pick("timezone", &zones, &d.timezone),
            language: pick("language", &["en", "nl", "de", "fr"], &d.language),
            footer: pick("footer", &["none", "badge", "table"], &d.footer),
            colour: if is_colour(&colour) { colour } else { d.colour },
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        for (i, day) in DAYS.iter().enumerate() {
            values.insert((*day).into(), self.days[i].clone().into());
        }
        values.insert("note".into(), self.note.clone().into());
        values.insert("timezone".into(), self.timezone.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("footer".into(), self.footer.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values
    }

    fn zone(&self) -> &'static str {
        ZONES.iter().find(|z| z.0 == self.timezone).map(|z| z.1).unwrap_or("Europe/Amsterdam")
    }
}

/// `9:00-17:00`, `09.00 – 12.30, 13:30-18:00`, `closed`, `24h`. Err names
/// the part that could not be read.
fn parse_day(text: &str) -> Result<Ranges, String> {
    let lower = text.trim().to_lowercase();
    if lower.is_empty() || ["closed", "gesloten", "geschlossen", "fermé", "ferme", "-"].contains(&lower.as_str()) {
        return Ok(Vec::new());
    }
    if ["24h", "24 h", "24/7", "open 24 hours", "hele dag"].contains(&lower.as_str()) {
        return Ok(vec![(0, 1440)]);
    }
    let time = |t: &str| -> Option<u32> {
        let t = t.trim();
        let (h, m) = match t.split_once([':', '.']) {
            Some((h, m)) => (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?),
            None => (t.parse::<u32>().ok()?, 0),
        };
        ((h < 24 && m < 60) || (h == 24 && m == 0)).then_some(h * 60 + m)
    };
    let mut ranges = Vec::new();
    for part in lower.split([',', ';', '&']).map(str::trim).filter(|p| !p.is_empty()) {
        let normalised = part.replace(['–', '—'], "-").replace(" to ", "-").replace(" tot ", "-");
        let (from, to) = normalised.split_once('-').ok_or_else(|| part.to_owned())?;
        let (from, mut to) = (time(from).ok_or_else(|| part.to_owned())?, time(to).ok_or_else(|| part.to_owned())?);
        if to <= from {
            // 22:00-02:00: open past midnight; counted to the end of this day.
            to = 1440;
        }
        ranges.push((from, to));
    }
    Ok(ranges)
}

struct Words {
    days: [&'static str; 7],
    closed: &'static str,
    caption: &'static str,
    open_now: &'static str,
    closed_now: &'static str,
    closes: &'static str,
    opens: &'static str,
    opens_day: &'static str,
    today: &'static str,
}

fn words(language: &str) -> Words {
    match language {
        "nl" => Words {
            days: ["Maandag", "Dinsdag", "Woensdag", "Donderdag", "Vrijdag", "Zaterdag", "Zondag"],
            closed: "Gesloten",
            caption: "Openingstijden",
            open_now: "Nu open",
            closed_now: "Nu gesloten",
            closes: "sluit om {t}",
            opens: "opent om {t}",
            opens_day: "opent {d} om {t}",
            today: "Vandaag",
        },
        "de" => Words {
            days: ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"],
            closed: "Geschlossen",
            caption: "Öffnungszeiten",
            open_now: "Jetzt geöffnet",
            closed_now: "Jetzt geschlossen",
            closes: "schließt um {t}",
            opens: "öffnet um {t}",
            opens_day: "öffnet {d} um {t}",
            today: "Heute",
        },
        "fr" => Words {
            days: ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"],
            closed: "Fermé",
            caption: "Horaires d'ouverture",
            open_now: "Ouvert",
            closed_now: "Fermé",
            closes: "ferme à {t}",
            opens: "ouvre à {t}",
            opens_day: "ouvre {d} à {t}",
            today: "Aujourd'hui",
        },
        _ => Words {
            days: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
            closed: "Closed",
            caption: "Opening hours",
            open_now: "Open now",
            closed_now: "Closed now",
            closes: "closes at {t}",
            opens: "opens at {t}",
            opens_day: "opens {d} at {t}",
            today: "Today",
        },
    }
}

fn hhmm(minutes: u32) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// `Mon..Sun` as parsed ranges, with days that do not parse treated as closed.
fn week(config: &Config) -> [Ranges; 7] {
    std::array::from_fn(|i| parse_day(&config.days[i]).unwrap_or_default())
}

fn table(config: &Config, w: &Words, week: &[Ranges; 7], with_badge: bool) -> String {
    let mut rows = String::new();
    for (i, ranges) in week.iter().enumerate() {
        let hours = if ranges.is_empty() {
            w.closed.to_owned()
        } else {
            ranges.iter().map(|(a, b)| format!("{}–{}", hhmm(*a), if *b == 1440 { "24:00".into() } else { hhmm(*b) })).collect::<Vec<_>>().join(", ")
        };
        rows.push_str(&format!("<tr data-day=\"{i}\"><th scope=\"row\">{}</th><td>{hours}</td></tr>", w.days[i]));
    }
    let note = if config.note.is_empty() { String::new() } else { format!("<p class=\"soh-note\">{}</p>", escape(&config.note)) };
    format!(
        "<div class=\"soh\" {MARKER}>{badge}<table class=\"soh-table\"><caption>{}</caption><tbody>{rows}</tbody></table>{note}</div>",
        w.caption,
        badge = if with_badge { badge() } else { String::new() },
    )
}

fn badge() -> String {
    format!("<span class=\"soh-badge\" {MARKER}-badge hidden><i aria-hidden=\"true\"></i><b></b><span></span></span>")
}

fn style(config: &Config) -> String {
    format!(
        "<style id=\"stride-opening-hours\">.soh{{max-width:26rem;font:15px/1.5 system-ui,-apple-system,\"Segoe UI\",sans-serif;margin:1.25em 0}}\
         .soh-table{{width:100%;border-collapse:collapse;margin-top:10px}}\
         .soh-table caption{{text-align:left;font-weight:700;font-size:1.05em;padding-bottom:6px}}\
         .soh-table th,.soh-table td{{padding:7px 10px;border-bottom:1px solid rgba(127,127,127,.22);text-align:left;font-weight:400}}\
         .soh-table td{{text-align:right;font-variant-numeric:tabular-nums}}\
         .soh-table tr.soh-today th,.soh-table tr.soh-today td{{font-weight:700}}\
         .soh-table tr.soh-today{{background:rgba(127,127,127,.09)}}\
         .soh-note{{font-size:.9em;opacity:.8;margin:.6em 0 0}}\
         .soh-badge{{display:inline-flex;align-items:center;gap:7px;padding:5px 12px 5px 10px;border-radius:999px;\
         background:rgba(127,127,127,.12);font:500 14px/1.3 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         .soh-badge[hidden]{{display:none}}.soh-badge i{{width:9px;height:9px;border-radius:50%;background:#9ca3af}}\
         .soh-badge.soh-open i{{background:{c};box-shadow:0 0 0 3px {c}33}}.soh-badge.soh-open b{{color:{c}}}\
         .soh-badge b{{font-weight:700}}footer .soh{{margin:0 0 1em}}</style>",
        c = config.colour,
    )
}

fn script(config: &Config, w: &Words, week: &[Ranges; 7]) -> String {
    let data: Vec<String> = week
        .iter()
        .map(|r| format!("[{}]", r.iter().map(|(a, b)| format!("[{a},{b}]")).collect::<Vec<_>>().join(",")))
        .collect();
    format!(
        "<script>(function(){{var H=[{data}],Z='{zone}',DAYS=['{d}'],W={{open:'{open}',closed:'{closed}',closes:'{closes}',\
         opens:'{opens}',opensDay:'{opens_day}',today:'{today}'}};\
         function now(){{try{{var o={{}};new Intl.DateTimeFormat('en-GB',{{timeZone:Z,weekday:'short',hour:'numeric',minute:'numeric',hourCycle:'h23'}})\
         .formatToParts(new Date()).forEach(function(p){{o[p.type]=p.value}});\
         return [['Mon','Tue','Wed','Thu','Fri','Sat','Sun'].indexOf(o.weekday),+o.hour*60+ +o.minute]}}\
         catch(e){{var d=new Date();return [(d.getDay()+6)%7,d.getHours()*60+d.getMinutes()]}}}}\
         function t(m){{m=m%1440;return (m<600?'0':'')+Math.floor(m/60)+':'+(m%60<10?'0':'')+m%60}}\
         function status(){{var n=now(),day=n[0],min=n[1];\
         var open=H[day].filter(function(r){{return r[0]<=min&&min<r[1]}})[0];\
         if(open){{var end=open[1];if(end===1440){{var nx=H[(day+1)%7][0];if(nx&&nx[0]===0)return [1,W.open,''];}}\
         return [1,W.open,W.closes.replace('{{t}}',t(end))]}}\
         var later=H[day].filter(function(r){{return r[0]>min}})[0];if(later)return [0,W.closed,W.opens.replace('{{t}}',t(later[0]))];\
         for(var i=1;i<=7;i++){{var r=H[(day+i)%7][0];if(r)return [0,W.closed,W.opensDay.replace('{{d}}',i===1?DAYS[7]:DAYS[(day+i)%7]).replace('{{t}}',t(r[0]))]}}\
         return [0,W.closed,'']}}\
         var s=status(),day=now()[0];\
         [].forEach.call(document.querySelectorAll('[{MARKER}-badge]'),function(b){{b.classList.toggle('soh-open',!!s[0]);\
         b.querySelector('b').textContent=s[1]+' ';b.querySelector('span').textContent=s[2]?'· '+s[2]:'';b.hidden=false}});\
         [].forEach.call(document.querySelectorAll('.soh-table tr[data-day=\"'+day+'\"]'),function(r){{r.className='soh-today';\
         r.setAttribute('aria-current','date')}})}})()</script>",
        data = data.join(","),
        zone = config.zone(),
        d = w.days.iter().map(|d| if matches!(config.language.as_str(), "nl" | "fr") { d.to_lowercase() } else { (*d).to_owned() }).chain(std::iter::once(match config.language.as_str() {
            "nl" => "morgen".to_owned(),
            "de" => "morgen".to_owned(),
            "fr" => "demain".to_owned(),
            _ => "tomorrow".to_owned(),
        })).map(|d| d.replace('\'', "\\'")).collect::<Vec<_>>().join("','"),
        open = w.open_now,
        closed = w.closed_now,
        closes = w.closes,
        opens = w.opens,
        opens_day = w.opens_day.replace('\'', "\\'"),
        today = w.today.replace('\'', "\\'"),
    )
}

/// Replace a shortcode; a paragraph holding only the code is replaced whole,
/// because a table inside `<p>` is not valid HTML.
fn replace_code(html: &str, code: &str, block: &str, inline: &str) -> (String, bool) {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut found = false;
    while let Some(at) = rest.find(code) {
        found = true;
        let before = &rest[..at];
        let after = &rest[at + code.len()..];
        let p_open = before.rfind("<p");
        let whole = p_open
            .filter(|&p| before[p..].find('>').map(|g| before[p + g + 1..].trim().is_empty()).unwrap_or(false))
            .filter(|_| after.trim_start().starts_with("</p>"));
        match whole {
            Some(p) => {
                out.push_str(&before[..p]);
                out.push_str(block);
                let close = after.find("</p>").map(|c| c + 4).unwrap_or(0);
                rest = &after[close..];
            }
            None => {
                out.push_str(before);
                out.push_str(inline);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, found)
}

fn render(html: String, config: &Config) -> String {
    let has_codes = html.contains(TABLE_CODE) || html.contains(BADGE_CODE);
    if !config.enabled || html.contains("id=\"stride-opening-hours\"") || (!has_codes && config.footer == "none") {
        return html;
    }
    let w = words(&config.language);
    let week = week(config);
    let table = table(config, &w, &week, !html.contains(BADGE_CODE));
    let (html, _) = replace_code(&html, TABLE_CODE, &table, &table);
    let (mut html, _) = replace_code(&html, BADGE_CODE, &format!("<p>{}</p>", badge()), &badge());
    match config.footer.as_str() {
        "badge" | "table" => {
            let block = if config.footer == "table" { table.clone() } else { format!("<p>{}</p>", badge()) };
            let lower = html.to_ascii_lowercase();
            match lower.rfind("<footer").and_then(|f| html[f..].find('>').map(|g| f + g + 1)) {
                Some(at) => html.insert_str(at, &block),
                None => insert_before_body_end(&mut html, &format!("<div class=\"soh-footer\">{block}</div>")),
            }
        }
        _ => {}
    }
    insert_in_head(&mut html, &style(config));
    insert_before_body_end(&mut html, &script(config, &w, &week));
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
        Err(error) => {
            // Without storage there are no hours to show, only defaults nobody entered.
            stride_pdk::log("warn", &format!("no opening hours shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &config) }))
}

#[plugin_fn]
pub fn panel_opening_hours(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: String| Ok(Json(PanelResponse { error, ..PanelResponse::default() }));
            let names = words("en").days;
            for (i, day) in config.days.iter().enumerate() {
                if let Err(part) = parse_day(day) {
                    return refuse(format!(
                        "{}: \"{part}\" is not a time range. Write it like 09:00-17:00, separate two ranges with a comma, or write closed.",
                        names[i]
                    ));
                }
            }
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere to keep your opening hours.".to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: "Saved. Type [opening-hours] or [open-now] on a page, then publish it again.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_formats() {
        assert_eq!(parse_day("9:00-17:00"), Ok(vec![(540, 1020)]));
        assert_eq!(parse_day("09.00 – 12.30, 13:30-18:00"), Ok(vec![(540, 750), (810, 1080)]));
        assert_eq!(parse_day("Gesloten"), Ok(vec![]));
        assert_eq!(parse_day(""), Ok(vec![]));
        assert_eq!(parse_day("24h"), Ok(vec![(0, 1440)]));
        assert_eq!(parse_day("22:00-02:00"), Ok(vec![(1320, 1440)]));
        assert_eq!(parse_day("9:00 to 17:30"), Ok(vec![(540, 1050)]));
        assert!(parse_day("morning").is_err());
        assert!(parse_day("25:00-26:00").is_err());
    }

    #[test]
    fn table_code_replaces_its_paragraph() {
        let html = "<html><head></head><body><h2>Visit</h2><p class=\"x\">[opening-hours]</p><p>Come by.</p></body></html>";
        let out = render(html.into(), &Config::default());
        assert!(!out.contains("[opening-hours]"));
        assert!(!out.contains("<p class=\"x\">"));
        assert!(out.contains("<caption>Opening hours</caption>"));
        assert!(out.contains("<th scope=\"row\">Saturday</th><td>10:00–16:00</td>"));
        assert!(out.contains("<th scope=\"row\">Sunday</th><td>Closed</td>"));
        assert!(out.contains("var H=[[[540,1020]],[[540,1020]],[[540,1020]],[[540,1020]],[[540,1020]],[[600,960]],[]]"));
        assert_eq!(render(out.clone(), &Config::default()), out);
    }

    #[test]
    fn inline_badge_and_dutch() {
        let html = "<html><head></head><body><p>We are [open-now] today.</p></body></html>";
        let c = Config { language: "nl".into(), ..Config::default() };
        let out = render(html.into(), &c);
        assert!(out.contains("<p>We are <span class=\"soh-badge\""));
        assert!(out.contains("open:'Nu open'"));
    }

    #[test]
    fn footer_badge_and_nothing_without_codes() {
        let html = "<html><head></head><body><main>x</main><footer class=\"f\"><p>Contact</p></footer></body></html>";
        assert_eq!(render(html.into(), &Config::default()), html);
        let c = Config { footer: "badge".into(), ..Config::default() };
        let out = render(html.into(), &c);
        assert!(out.contains("<footer class=\"f\"><p><span class=\"soh-badge\""));
    }

    #[test]
    fn note_is_escaped() {
        let c = Config { note: "<script>x</script>".into(), ..Config::default() };
        let both = render("<p>[open-now]</p><p>[opening-hours]</p>".into(), &Config::default());
        assert_eq!(both.matches("class=\"soh-badge\"").count(), 1);
        let out = render("<p>[opening-hours]</p>".into(), &c);
        assert!(out.contains("&lt;script&gt;x&lt;/script&gt;"));
    }
}
