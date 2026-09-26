//! SEO Inspector: a Stride plugin.
//!
//! Every time a page's tree is saved, it walks the document and checks what a
//! search engine and a screen reader notice first: the SEO title's length, a
//! missing meta description, images without alt text, the number of `h1`s,
//! skipped heading levels, thin content and a long URL. The result is a score
//! from 0 to 100 and a list of warnings, stored per page and shown in a panel
//! in the site settings.
//!
//! It never changes a document (it always answers `{"document": null}`), so it
//! does not need `write-pages`. It asks for `storage` only, to keep the
//! reports and its three settings. Without it there is nowhere to put a
//! report: the hook does nothing and the panel says why.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::serde_json;
use stride_pdk::{
    DocumentSave, DocumentSaved, JsonValue, Map, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
const REPORT_PREFIX: &str = "page:";
/// Keys are at most 128 bytes; leave room for the prefix.
const MAX_SLUG_KEY: usize = 120;
/// 256 keys per site, one is the settings.
const MAX_REPORTS: usize = 250;
const MAX_REPORT_CHARS: usize = 8192;

const DEFAULT_MIN_WORDS: u32 = 300;
const DEFAULT_MAX_TITLE: u32 = 60;
const DEFAULT_MAX_URL: u32 = 75;
const MIN_TITLE: usize = 15;
const MIN_DESCRIPTION: usize = 50;
const MAX_DESCRIPTION: usize = 160;

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Settings {
    min_words: u32,
    max_title: u32,
    max_url: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            min_words: DEFAULT_MIN_WORDS,
            max_title: DEFAULT_MAX_TITLE,
            max_url: DEFAULT_MAX_URL,
        }
    }
}

fn int(value: Option<&JsonValue>, min: u32, max: u32, fallback: u32) -> u32 {
    value
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .map(|n| n.clamp(min as u64, max as u64) as u32)
        .unwrap_or(fallback)
}

impl Settings {
    fn from_map(map: &Map<String, JsonValue>) -> Self {
        let d = Self::default();
        Self {
            min_words: int(map.get("min-words"), 0, 5000, d.min_words),
            max_title: int(map.get("max-title"), 30, 120, d.max_title),
            max_url: int(map.get("max-url"), 20, 200, d.max_url),
        }
    }

    fn to_map(self) -> Map<String, JsonValue> {
        let mut map = Map::new();
        map.insert("min-words".into(), self.min_words.into());
        map.insert("max-title".into(), self.max_title.into());
        map.insert("max-url".into(), self.max_url.into());
        map
    }

    fn load() -> Result<Self, stride_pdk::HostError> {
        Ok(match kv::get::<JsonValue>(SETTINGS_KEY)? {
            Some(JsonValue::Object(map)) => Self::from_map(&map),
            _ => Self::default(),
        })
    }
}

// ---------------------------------------------------------------- analysis

#[derive(Debug, Default)]
struct Facts {
    headings: Vec<u8>,
    images: usize,
    images_without_alt: usize,
    words: usize,
    first_h1: Option<String>,
}

fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

fn walk(node: &JsonValue, facts: &mut Facts) {
    let kind = node.get("kind");
    let kind_type = kind.and_then(|k| k.get("type")).and_then(JsonValue::as_str);
    match kind_type {
        Some("text") => {
            let content = kind
                .and_then(|k| k.get("content"))
                .and_then(JsonValue::as_str)
                .unwrap_or("");
            facts.words += count_words(content);
            let tag = kind
                .and_then(|k| k.get("tag"))
                .and_then(JsonValue::as_str)
                .unwrap_or("p");
            if let Some(level) = heading_level(tag) {
                facts.headings.push(level);
                if level == 1 && facts.first_h1.is_none() {
                    facts.first_h1 = Some(content.trim().to_owned());
                }
            }
        }
        Some("image") => {
            facts.images += 1;
            let alt = kind
                .and_then(|k| k.get("alt"))
                .and_then(JsonValue::as_str)
                .unwrap_or("");
            // An empty alt marks an image as decorative, which is legitimate,
            // but on a content page it is far more often forgotten.
            if alt.trim().is_empty() {
                facts.images_without_alt += 1;
            }
        }
        Some("button") => {
            if let Some(label) = kind.and_then(|k| k.get("label")).and_then(JsonValue::as_str) {
                facts.words += count_words(label);
            }
        }
        _ => {}
    }
    if let Some(children) = node.get("children").and_then(JsonValue::as_array) {
        for child in children {
            walk(child, facts);
        }
    }
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Report {
    score: u32,
    warnings: Vec<String>,
    words: usize,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn analyse(slug: &str, document: &JsonValue, settings: Settings) -> Report {
    let mut facts = Facts::default();
    if let Some(root) = document.get("root") {
        walk(root, &mut facts);
    }
    let seo = document.get("seo");
    let seo_str = |field: &str| {
        seo.and_then(|s| s.get(field))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };

    let mut penalty: u32 = 0;
    let mut warnings = Vec::new();
    let mut warn = |points: u32, text: String| {
        penalty += points;
        warnings.push(text);
    };

    // Title: the SEO title if set, otherwise the first h1 is the best guess
    // at what the page is called (the page name is not in the tree).
    let max_title = settings.max_title as usize;
    match seo_str("title") {
        Some(title) => {
            let n = chars(title);
            if n > max_title {
                warn(10, format!("Title is {n} characters; search results cut it off after about {max_title}."));
            } else if n < MIN_TITLE {
                warn(10, format!("Title is only {n} characters; aim for {MIN_TITLE} to {max_title}."));
            }
        }
        None => warn(5, "No SEO title set; the page name is used as the title.".into()),
    }

    match seo_str("description") {
        None => warn(20, "Missing meta description.".into()),
        Some(description) => {
            let n = chars(description);
            if n < MIN_DESCRIPTION {
                warn(5, format!("Meta description is only {n} characters; aim for {MIN_DESCRIPTION} to {MAX_DESCRIPTION}."));
            } else if n > MAX_DESCRIPTION {
                warn(5, format!("Meta description is {n} characters; search results show about {MAX_DESCRIPTION}."));
            }
        }
    }

    if facts.images_without_alt > 0 {
        let n = facts.images_without_alt;
        warn(
            (5 * n as u32).min(20),
            format!("{} of {} without alt text.", plural(n, "image", "images"), facts.images),
        );
    }

    let h1s = facts.headings.iter().filter(|&&l| l == 1).count();
    if h1s == 0 {
        warn(15, "No h1 heading.".into());
    } else if h1s > 1 {
        warn(10, format!("{h1s} h1 headings; a page should have one."));
    }

    let mut skipped = 0u32;
    let mut first_skip = None;
    let mut previous = 0u8;
    for &level in &facts.headings {
        if previous > 0 && level > previous + 1 {
            skipped += 1;
            first_skip.get_or_insert((previous, level));
        }
        previous = level;
    }
    if let Some((from, to)) = first_skip {
        let more = if skipped > 1 {
            format!(" ({} in total)", plural(skipped as usize, "skip", "skips"))
        } else {
            String::new()
        };
        warn(
            (5 * skipped).min(15),
            format!("Heading level skipped: h{from} followed by h{to}{more}."),
        );
    }

    let min_words = settings.min_words as usize;
    if facts.words < min_words {
        warn(15, format!("Thin content: {} (at least {min_words} recommended).", plural(facts.words, "word", "words")));
    }

    let path = if slug == "home" { "/".to_owned() } else { format!("/{slug}") };
    let n = chars(&path);
    if n > settings.max_url as usize {
        warn(5, format!("URL is {n} characters; keep it under {}.", settings.max_url));
    }

    if seo.and_then(|s| s.get("noindex")).and_then(JsonValue::as_bool) == Some(true) {
        warnings.push("Marked noindex: search engines are asked to leave this page out.".into());
    }

    Report {
        score: 100u32.saturating_sub(penalty),
        warnings,
        words: facts.words,
    }
}

fn report_key(slug: &str) -> String {
    let mut end = slug.len().min(MAX_SLUG_KEY);
    while !slug.is_char_boundary(end) {
        end -= 1;
    }
    format!("{REPORT_PREFIX}{}", &slug[..end])
}

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_document_save(Json(save): Json<DocumentSave>) -> FnResult<Json<DocumentSaved>> {
    // Templates and partials are not pages anybody searches for.
    if save.kind != "page" {
        return Ok(Json(DocumentSaved::default()));
    }
    let settings = match Settings::load() {
        Ok(settings) => settings,
        // Without storage there is nowhere to keep a report: look, say
        // nothing, change nothing.
        Err(error) if error.is_permission_denied() => return Ok(Json(DocumentSaved::default())),
        Err(error) => {
            stride_pdk::log("warn", &format!("seo-inspector: settings: {error}"));
            Settings::default()
        }
    };
    let report = analyse(&save.slug, &save.document, settings);
    let value = serde_json::json!({
        "slug": save.slug,
        "score": report.score,
        "words": report.words,
        "warnings": report.warnings,
    });
    let key = report_key(&save.slug);
    if let Err(error) = kv::set(&key, &value) {
        if !error.is_permission_denied() {
            stride_pdk::log("warn", &format!("seo-inspector: storing {key}: {error}"));
        }
    }
    Ok(Json(DocumentSaved::default()))
}

// ---------------------------------------------------------------- the panel

fn format_reports(mut reports: Vec<(String, u32, Vec<String>)>) -> String {
    if reports.is_empty() {
        return "No reports yet. Save a page and its score appears here.".into();
    }
    reports.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let average = reports.iter().map(|r| r.1 as usize).sum::<usize>() / reports.len();
    let mut out = format!(
        "{} checked, average score {average}/100.\n",
        plural(reports.len(), "page", "pages")
    );
    for (slug, score, warnings) in reports {
        let path = if slug == "home" { "/".to_owned() } else { format!("/{slug}") };
        let grade = match score {
            90..=100 => "good",
            70..=89 => "fair",
            _ => "poor",
        };
        out.push_str(&format!("\n{path}  {score}/100 ({grade})\n"));
        if warnings.is_empty() {
            out.push_str("  - No problems found.\n");
        }
        for warning in warnings {
            out.push_str(&format!("  - {warning}\n"));
        }
    }
    out.truncate(out.trim_end().len());
    // The panel field holds at most 8192 characters; keep the worst pages.
    if out.chars().count() > MAX_REPORT_CHARS {
        let cut = out.char_indices().nth(MAX_REPORT_CHARS - 40).map_or(out.len(), |(i, _)| i);
        let cut = out[..cut].rfind('\n').unwrap_or(cut);
        out.truncate(cut);
        out.push_str("\n\n(More pages not shown.)");
    }
    out
}

fn load_reports() -> Result<String, stride_pdk::HostError> {
    let keys = kv::list(REPORT_PREFIX)?;
    let mut reports = Vec::new();
    for key in keys.into_iter().take(MAX_REPORTS) {
        let Some(value) = kv::get::<JsonValue>(&key)? else { continue };
        let slug = value
            .get("slug")
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| key[REPORT_PREFIX.len()..].to_owned());
        let score = value.get("score").and_then(JsonValue::as_u64).unwrap_or(0).min(100) as u32;
        let warnings = value
            .get("warnings")
            .and_then(JsonValue::as_array)
            .map(|list| list.iter().filter_map(|w| w.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        reports.push((slug, score, warnings));
    }
    Ok(format_reports(reports))
}

fn refused(settings: Settings) -> PanelResponse {
    let mut values = settings.to_map();
    values.insert(
        "report".into(),
        "SEO Inspector needs the storage permission to keep page reports.".into(),
    );
    PanelResponse {
        values,
        message: String::new(),
        error: "Storage was not granted. Allow it under Admin, Plugins, Permissions.".into(),
    }
}

#[plugin_fn]
pub fn panel_seo_inspector(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let mut message = String::new();
    let settings = match request.event {
        PanelEvent::Load => match Settings::load() {
            Ok(settings) => settings,
            Err(error) if error.is_permission_denied() => {
                return Ok(Json(refused(Settings::default())));
            }
            Err(error) => {
                return Ok(Json(PanelResponse {
                    values: Settings::default().to_map(),
                    message: String::new(),
                    error: format!("Could not read the settings: {error}"),
                }));
            }
        },
        PanelEvent::Submit => {
            let settings = Settings::from_map(&request.values);
            match kv::set(SETTINGS_KEY, &JsonValue::Object(settings.to_map())) {
                Ok(()) => {}
                Err(error) if error.is_permission_denied() => return Ok(Json(refused(settings))),
                Err(error) => {
                    return Ok(Json(PanelResponse {
                        values: settings.to_map(),
                        message: String::new(),
                        error: format!("Could not save: {error}"),
                    }));
                }
            }
            message = "Saved. Pages are checked with these settings the next time they are saved.".into();
            settings
        }
    };
    let mut values = settings.to_map();
    let report = match load_reports() {
        Ok(report) => report,
        Err(error) if error.is_permission_denied() => return Ok(Json(refused(settings))),
        Err(error) => format!("Could not read the reports: {error}"),
    };
    values.insert("report".into(), report.into());
    Ok(Json(PanelResponse { values, message, error: String::new() }))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(tag: &str, content: &str) -> JsonValue {
        json!({"id": "t", "kind": {"type": "text", "tag": tag, "content": content}})
    }

    fn doc(children: Vec<JsonValue>, seo: JsonValue) -> JsonValue {
        json!({"root": {"id": "r", "kind": {"type": "frame"}, "children": children}, "seo": seo})
    }

    #[test]
    fn a_good_page_scores_100() {
        let body = "word ".repeat(400);
        let d = doc(
            vec![
                text("h1", "Handmade ceramics"),
                text("p", &body),
                text("h2", "Care"),
                json!({"id": "i", "kind": {"type": "image", "src": "/a.jpg", "alt": "A bowl"}}),
            ],
            json!({"title": "Handmade ceramics from Utrecht", "description": "Small-batch stoneware bowls, mugs and plates, thrown and glazed by hand in our Utrecht studio."}),
        );
        let r = analyse("shop", &d, Settings::default());
        assert_eq!(r.score, 100, "{:?}", r.warnings);
    }

    #[test]
    fn a_bad_page_collects_warnings() {
        let d = doc(
            vec![
                text("h1", "A"),
                text("h1", "B"),
                text("h4", "C"),
                json!({"id": "i", "kind": {"type": "image", "src": "/a.jpg"}}),
            ],
            json!(null),
        );
        let r = analyse(&"x".repeat(100), &d, Settings::default());
        assert_eq!(r.warnings.len(), 7, "{:?}", r.warnings);
        assert_eq!(r.score, 100 - 5 - 20 - 5 - 10 - 5 - 15 - 5);
    }

    #[test]
    fn settings_clamp_and_parse() {
        let mut m = Map::new();
        m.insert("min-words".into(), json!("9999"));
        m.insert("max-title".into(), json!(10));
        let s = Settings::from_map(&m);
        assert_eq!((s.min_words, s.max_title, s.max_url), (5000, 30, DEFAULT_MAX_URL));
    }

    #[test]
    fn reports_sort_worst_first() {
        let out = format_reports(vec![
            ("home".into(), 95, vec![]),
            ("about".into(), 40, vec!["Missing meta description.".into()]),
        ]);
        assert!(out.find("/about").unwrap() < out.find("/  95").unwrap(), "{out}");
    }
}
