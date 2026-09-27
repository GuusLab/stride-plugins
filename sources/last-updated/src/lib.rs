//! Last Updated — a Stride plugin.
//!
//! It shows "Last updated on 27 September 2026" on every page, in Dutch or
//! English to match the page's `lang`.
//!
//! A WebAssembly module has no clock, so the date has to come from Stride.
//! `on_publish` reads the site's document listing (`read-pages`, listings
//! only, never a page's content) and records, per page, the day and the
//! version it was published with. Publishing rewrites a document's
//! `updatedAt` but not its `version`, so a date only moves when the version
//! did: "Publish site" does not stamp today on every page. `on_page_render`,
//! which runs for every visitor, then does one storage read and a string
//! insert, and never calls the host for anything else.

use std::collections::BTreeMap;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::serde_json;
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Published,
    kv,
};

// Storage keys.
const ENABLED: &str = "enabled";
const POSITION: &str = "position";
const FORMAT: &str = "format";
const LABEL: &str = "label";
const SKIP: &str = "skip";
/// Every recorded date, in one value: `{slug: "YYYY-MM-DD|version"}`.
/// One key instead of one per page keeps clear of the 256-key limit.
const DATES: &str = "dates";
/// Set when `on_publish` could not read the listing, so the panel can say why.
const PROBLEM: &str = "problem";

// Panel field names that differ from the storage keys.
const F_SKIP: &str = "skip-pages";

const MAX_LABEL: usize = 40;
const MAX_SKIP: usize = 100;
/// Stay well under the 64 KiB value limit.
const MAX_DATES_BYTES: usize = 60_000;

/// The class on the line, and the marker that makes the hook idempotent.
const MARKER: &str = "stride-last-updated";

// ---------------------------------------------------------------- the hooks

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = Settings::load();
    let html = match load_dates() {
        Some(dates) => {
            let date = dates.get(page.slug.trim_matches('/')).and_then(|entry| day_of(entry));
            match date {
                Some(date) => render(&page.html, &page.slug, date, &settings).unwrap_or(page.html),
                None => page.html,
            }
        }
        None => page.html,
    };
    Ok(Json(PageRendered { html }))
}

#[plugin_fn]
pub fn on_publish(Json(event): Json<Published>) -> FnResult<Json<JsonValue>> {
    record(&event);
    Ok(Json(JsonValue::Object(Map::new())))
}

/// Read the listing and update the stored dates. Every failure leaves things
/// as they were: a publish is never the place to complain.
fn record(event: &Published) {
    let listing: Result<JsonValue, _> = stride_pdk::action(
        "documents.list",
        &serde_json::json!({ "siteId": event.site_id, "kind": "page" }),
    );
    let listing = match listing {
        Ok(listing) => {
            if kv::get::<String>(PROBLEM).ok().flatten().is_some() {
                let _ = kv::delete(PROBLEM);
            }
            listing
        }
        Err(error) => {
            let problem = if error.is_permission_denied() { "read-pages" } else { "listing" };
            let _ = kv::set(PROBLEM, &problem);
            return;
        }
    };
    let documents: Vec<Doc> = listing
        .get("documents")
        .and_then(JsonValue::as_array)
        .map(|documents| documents.iter().filter_map(Doc::from_json).collect())
        .unwrap_or_default();
    let Ok(stored) = kv::get::<BTreeMap<String, String>>(DATES) else {
        // Storage refused (or unreadable): there is nowhere to keep a date.
        return;
    };
    let stored = stored.unwrap_or_default();
    let updated = merge(&stored, &documents, &event.document_id);
    if updated != stored && serde_json::to_string(&updated).map_or(0, |s| s.len()) <= MAX_DATES_BYTES
    {
        let _ = kv::set(DATES, &updated);
    }
}

/// One page from `documents.list`.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub id: String,
    pub slug: String,
    pub version: i64,
    /// `YYYY-MM-DD`.
    pub day: String,
}

impl Doc {
    fn from_json(value: &JsonValue) -> Option<Doc> {
        if value.get("kind").and_then(JsonValue::as_str).is_some_and(|kind| kind != "page") {
            return None;
        }
        let updated = value.get("updatedAt")?.as_str()?;
        Some(Doc {
            id: value.get("id")?.as_str()?.to_owned(),
            slug: value.get("slug")?.as_str()?.trim_matches('/').to_owned(),
            version: value.get("version")?.as_i64()?,
            day: valid_day(updated.get(..10)?)?.to_owned(),
        })
    }
}

/// The dates after a publish of `published_id`.
///
/// - The published page gets its listing date if its version changed (or it
///   had none); the same version keeps the date it had.
/// - Other pages we have never seen get their listing date, which is how
///   installing on a site with existing pages fills in after one publish.
/// - Pages that are gone (deleted or renamed) are dropped.
pub fn merge(
    stored: &BTreeMap<String, String>,
    documents: &[Doc],
    published_id: &str,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for doc in documents {
        let entry = format!("{}|{}", doc.day, doc.version);
        let kept = stored.get(&doc.slug);
        let value = match kept {
            Some(old) if doc.id == published_id => {
                if version_of(old) == Some(doc.version) { old.clone() } else { entry }
            }
            Some(old) => old.clone(),
            None => entry,
        };
        out.insert(doc.slug.clone(), value);
    }
    out
}

fn day_of(entry: &str) -> Option<&str> {
    valid_day(entry.split('|').next()?)
}

fn version_of(entry: &str) -> Option<i64> {
    entry.split('|').nth(1)?.parse().ok()
}

fn load_dates() -> Option<BTreeMap<String, String>> {
    kv::get::<BTreeMap<String, String>>(DATES).ok().flatten()
}

/// `YYYY-MM-DD` with a real month and day, or `None`.
fn valid_day(day: &str) -> Option<&str> {
    let (_, m, d) = parse_day(day)?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some(day)
}

fn parse_day(day: &str) -> Option<(u32, u32, u32)> {
    let b = day.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |s: &str| -> Option<u32> {
        s.bytes().all(|c| c.is_ascii_digit()).then(|| s.parse().ok()).flatten()
    };
    Some((num(&day[..4])?, num(&day[5..7])?, num(&day[8..10])?))
}

// --------------------------------------------------------------- rendering

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lang {
    En,
    Nl,
}

/// The page with the line, or `None` to leave it exactly as it arrived.
pub fn render(html: &str, slug: &str, day: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let slug = slug.trim_matches('/');
    if settings.skip.iter().any(|skipped| skipped == slug) {
        return None;
    }
    let lang = page_lang(html);
    let line = line(day, lang, settings)?;
    let mut out = inject(html, &line, settings.position)?;
    let css = STYLE;
    match find_ci(out.as_bytes(), b"</head>") {
        Some(head) => out.insert_str(head, css),
        None => {
            let at = out.find(&line)?;
            out.insert_str(at, css);
        }
    }
    Some(out)
}

const STYLE: &str = "<style>.stride-last-updated{display:flex;align-items:center;gap:.45em;\
margin:.5rem 0 1.25rem;font-size:.875rem;line-height:1.4;opacity:.78}\
.stride-last-updated svg{width:1em;height:1em;flex:none}\
.stride-last-updated--end{margin:2rem 0 1rem;padding-top:1rem;border-top:1px solid currentColor;\
border-top-color:color-mix(in srgb,currentColor 18%,transparent)}</style>";

const ICON: &str = "<svg aria-hidden=\"true\" focusable=\"false\" viewBox=\"0 0 24 24\" \
fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" \
stroke-linejoin=\"round\"><path d=\"M21 12a9 9 0 1 1-2.64-6.36\"/><path d=\"M21 4v5h-5\"/>\
<path d=\"M12 7.5V12l3 2\"/></svg>";

/// The `<p>` itself.
pub fn line(day: &str, lang: Lang, settings: &Settings) -> Option<String> {
    let shown = format_day(day, lang, settings.format)?;
    let label = if settings.label.is_empty() {
        match lang {
            Lang::En => "Last updated on",
            Lang::Nl => "Laatst bijgewerkt op",
        }
        .to_owned()
    } else {
        settings.label.clone()
    };
    let modifier = match settings.position {
        Position::BelowTitle => "",
        Position::End => " stride-last-updated--end",
    };
    Some(format!(
        "<p class=\"{MARKER}{modifier}\">{ICON}<span>{} <time datetime=\"{day}\">{}</time></span></p>",
        escape(&label),
        escape(&shown),
    ))
}

const MONTHS_EN: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
const MONTHS_NL: [&str; 12] = [
    "januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus", "september",
    "oktober", "november", "december",
];
const SHORT_EN: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const SHORT_NL: [&str; 12] =
    ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"];

pub fn format_day(day: &str, lang: Lang, format: Format) -> Option<String> {
    let (y, m, d) = parse_day(valid_day(day)?)?;
    let i = (m - 1) as usize;
    Some(match (format, lang) {
        (Format::Long, Lang::En) => format!("{d} {} {y}", MONTHS_EN[i]),
        (Format::Long, Lang::Nl) => format!("{d} {} {y}", MONTHS_NL[i]),
        (Format::Short, Lang::En) => format!("{d} {} {y}", SHORT_EN[i]),
        (Format::Short, Lang::Nl) => format!("{d} {} {y}", SHORT_NL[i]),
        (Format::Numeric, Lang::Nl) => format!("{d:02}-{m:02}-{y}"),
        (Format::Numeric, Lang::En) => day.to_owned(),
    })
}

/// Dutch when `<html lang>` starts with `nl`, English otherwise.
pub fn page_lang(html: &str) -> Lang {
    let bytes = html.as_bytes();
    let Some(at) = find_ci(bytes, b"<html") else {
        return Lang::En;
    };
    let tag_end = html[at..].find('>').map_or(html.len(), |end| at + end);
    let tag = &html[at..tag_end];
    let Some(lang_at) = find_ci(tag.as_bytes(), b"lang=") else {
        return Lang::En;
    };
    let value = tag[lang_at + 5..].trim_start_matches(['"', '\'']);
    if value.len() >= 2 && value[..2].eq_ignore_ascii_case("nl") {
        Lang::Nl
    } else {
        Lang::En
    }
}

/// Put `line` where the settings say. `None` when the page has no body.
pub fn inject(html: &str, line: &str, position: Position) -> Option<String> {
    let bytes = html.as_bytes();
    let body = find_ci(bytes, b"<body")?;
    let body_open = body + html[body..].find('>')? + 1;
    let at = match position {
        Position::BelowTitle => find_ci(&bytes[body_open..], b"</h1>")
            .map(|end| body_open + end + 5)
            .or_else(|| after_open(html, body_open, b"<main"))
            .unwrap_or(body_open),
        Position::End => find_ci(&bytes[body_open..], b"</main>")
            .or_else(|| find_ci(&bytes[body_open..], b"<footer"))
            .or_else(|| find_ci(&bytes[body_open..], b"</body>"))
            .map_or(html.len(), |end| body_open + end),
    };
    let mut out = String::with_capacity(html.len() + line.len() + STYLE.len());
    out.push_str(&html[..at]);
    out.push_str(line);
    out.push_str(&html[at..]);
    Some(out)
}

fn after_open(html: &str, from: usize, tag: &[u8]) -> Option<usize> {
    let at = from + find_ci(&html.as_bytes()[from..], tag)?;
    Some(at + html[at..].find('>')? + 1)
}

pub fn escape(text: &str) -> String {
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

fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_last_updated(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: Settings::load().to_values(),
            message: report(),
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let settings = Settings::from_values(&request.values);
            if let Err(error) = settings.save() {
                return Ok(Json(PanelResponse {
                    values: settings.to_values(),
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so it can keep \
                         neither these settings nor any dates, and shows nothing. Grant storage \
                         under Plugins, Permissions."
                            .to_owned()
                    } else {
                        format!("Nothing was saved: {error}")
                    },
                }));
            }
            let message = if settings.enabled {
                "Saved. Visitors see the change on their next page load.".to_owned()
            } else {
                "Saved. The date is hidden on every page.".to_owned()
            };
            Ok(Json(PanelResponse {
                values: settings.to_values(),
                message,
                error: String::new(),
            }))
        }
    }
}

/// One line about what has been recorded, or why nothing can be.
fn report() -> String {
    match kv::get::<String>(PROBLEM).ok().flatten().as_deref() {
        Some("read-pages") => {
            return "No dates can be recorded: this plugin was not granted read-pages, which it \
                    needs to see when a page changed. Grant it under Plugins, Permissions, then \
                    publish the site once."
                .to_owned();
        }
        Some(_) => {
            return "The last publish could not read the list of pages, so no dates were \
                    recorded. Publish again to retry."
                .to_owned();
        }
        None => {}
    }
    match load_dates() {
        Some(dates) if !dates.is_empty() => {
            let count = dates.len();
            let pages = if count == 1 { "1 page".to_owned() } else { format!("{count} pages") };
            let newest = dates
                .iter()
                .filter_map(|(slug, entry)| Some((day_of(entry)?, slug)))
                .max();
            match newest {
                Some((day, slug)) => format!(
                    "Dates recorded for {pages}. Most recent change: \"{slug}\" on {}.",
                    format_day(day, Lang::En, Format::Long).unwrap_or_default()
                ),
                None => format!("Dates recorded for {pages}."),
            }
        }
        _ => "No dates recorded yet. Publish the site once and every page gets its date."
            .to_owned(),
    }
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    BelowTitle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Long,
    Short,
    Numeric,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub position: Position,
    pub format: Format,
    /// Empty for the localised default.
    pub label: String,
    pub skip: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            position: Position::BelowTitle,
            format: Format::Long,
            label: String::new(),
            skip: vec!["home".to_owned()],
        }
    }
}

impl Position {
    fn parse(value: Option<&str>) -> Self {
        if value == Some("end") { Position::End } else { Position::BelowTitle }
    }
    fn name(self) -> &'static str {
        match self {
            Position::BelowTitle => "below-title",
            Position::End => "end",
        }
    }
}

impl Format {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("short") => Format::Short,
            Some("numeric") => Format::Numeric,
            _ => Format::Long,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Format::Long => "long",
            Format::Short => "short",
            Format::Numeric => "numeric",
        }
    }
}

impl Settings {
    /// Every read may fail; the defaults stand in.
    pub fn load() -> Self {
        let d = Settings::default();
        let text = |key: &str| kv::get::<String>(key).ok().flatten();
        Settings {
            enabled: kv::get::<bool>(ENABLED).ok().flatten().unwrap_or(d.enabled),
            position: Position::parse(text(POSITION).as_deref()),
            format: Format::parse(text(FORMAT).as_deref()),
            label: text(LABEL).map(|label| clean_label(&label)).unwrap_or_default(),
            skip: text(SKIP).map(|skip| parse_slugs(&skip)).unwrap_or(d.skip),
        }
    }

    fn save(&self) -> Result<(), stride_pdk::HostError> {
        kv::set(ENABLED, &self.enabled)?;
        kv::set(POSITION, &self.position.name())?;
        kv::set(FORMAT, &self.format.name())?;
        kv::set(LABEL, &self.label)?;
        kv::set(SKIP, &self.skip.join(", "))
    }

    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert(ENABLED.into(), self.enabled.into());
        values.insert(POSITION.into(), self.position.name().into());
        values.insert(FORMAT.into(), self.format.name().into());
        values.insert(LABEL.into(), self.label.clone().into());
        values.insert(F_SKIP.into(), self.skip.join(", ").into());
        values
    }

    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        let text = |key: &str| values.get(key).and_then(JsonValue::as_str);
        Settings {
            enabled: values.get(ENABLED).and_then(JsonValue::as_bool).unwrap_or(true),
            position: Position::parse(text(POSITION)),
            format: Format::parse(text(FORMAT)),
            label: text(LABEL).map(clean_label).unwrap_or_default(),
            skip: text(F_SKIP).map(parse_slugs).unwrap_or_default(),
        }
    }
}

/// Whitespace collapsed, at most 40 characters.
fn clean_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_LABEL).collect()
}

/// "home, /contact about" into ["home", "contact", "about"].
fn parse_slugs(text: &str) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    for slug in text.split([',', ' ', '\n']).map(|s| s.trim().trim_matches('/')) {
        let slug = if slug.is_empty() { continue } else { slug.to_ascii_lowercase() };
        if !slugs.contains(&slug) && slugs.len() < MAX_SKIP {
            slugs.push(slug);
        }
    }
    slugs
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html lang=\"en\"><head><title>x</title></head>\
<body><nav>Home</nav><main><h1>Pricing</h1><p>Text.</p></main><footer>f</footer></body></html>";

    fn doc(id: &str, slug: &str, version: i64, day: &str) -> Doc {
        Doc { id: id.into(), slug: slug.into(), version, day: day.into() }
    }

    #[test]
    fn a_republish_without_edits_keeps_the_old_date() {
        let mut stored = BTreeMap::new();
        stored.insert("about".to_owned(), "2026-03-02|7".to_owned());
        let docs = [doc("d1", "about", 7, "2026-09-27")];
        assert_eq!(merge(&stored, &docs, "d1")["about"], "2026-03-02|7");
    }

    #[test]
    fn a_publish_with_new_edits_moves_the_date() {
        let mut stored = BTreeMap::new();
        stored.insert("about".to_owned(), "2026-03-02|7".to_owned());
        let docs = [doc("d1", "about", 9, "2026-09-27")];
        assert_eq!(merge(&stored, &docs, "d1")["about"], "2026-09-27|9");
    }

    #[test]
    fn another_page_keeps_its_date_and_new_ones_are_seeded() {
        let mut stored = BTreeMap::new();
        stored.insert("about".to_owned(), "2026-03-02|7".to_owned());
        stored.insert("gone".to_owned(), "2026-01-01|1".to_owned());
        let docs = [doc("d1", "about", 9, "2026-09-27"), doc("d2", "blog/x", 3, "2026-05-05")];
        let merged = merge(&stored, &docs, "d2");
        assert_eq!(merged["about"], "2026-03-02|7");
        assert_eq!(merged["blog/x"], "2026-05-05|3");
        assert!(!merged.contains_key("gone"));
    }

    #[test]
    fn renders_under_the_title_in_english() {
        let out = render(PAGE, "pricing", "2026-09-27", &Settings::default()).unwrap();
        assert!(out.contains("</h1><p class=\"stride-last-updated\">"));
        assert!(out.contains("Last updated on <time datetime=\"2026-09-27\">27 September 2026</time>"));
        assert!(out.contains("<style>.stride-last-updated"));
        assert!(render(&out, "pricing", "2026-09-27", &Settings::default()).is_none());
    }

    #[test]
    fn dutch_pages_get_dutch() {
        let page = PAGE.replace("lang=\"en\"", "lang=\"nl-NL\"");
        let out = render(&page, "pricing", "2026-03-05", &Settings::default()).unwrap();
        assert!(out.contains("Laatst bijgewerkt op <time datetime=\"2026-03-05\">5 maart 2026</time>"));
        let s = Settings { format: Format::Numeric, ..Settings::default() };
        assert!(render(&page, "p", "2026-03-05", &s).unwrap().contains(">05-03-2026<"));
        let s = Settings { format: Format::Short, ..Settings::default() };
        assert!(render(&page, "p", "2026-03-05", &s).unwrap().contains(">5 mrt 2026<"));
    }

    #[test]
    fn end_position_goes_before_main_closes() {
        let s = Settings { position: Position::End, ..Settings::default() };
        let out = render(PAGE, "pricing", "2026-09-27", &s).unwrap();
        assert!(out.contains("</p></main>"));
        assert!(out.contains("stride-last-updated--end"));
    }

    #[test]
    fn skips_home_and_escapes_the_label() {
        assert!(render(PAGE, "home", "2026-09-27", &Settings::default()).is_none());
        let s = Settings { label: "<b>\"Hi\"</b>".into(), ..Settings::default() };
        let out = render(PAGE, "x", "2026-09-27", &s).unwrap();
        assert!(out.contains("&lt;b&gt;&quot;Hi&quot;&lt;/b&gt;"));
    }

    #[test]
    fn rejects_bad_days() {
        assert!(format_day("2026-13-01", Lang::En, Format::Long).is_none());
        assert!(format_day("nonsense!!", Lang::En, Format::Long).is_none());
    }
}
