//! Reading Time — a Stride plugin.
//!
//! It adds one line — "4 min read" — under the first heading of every page,
//! and keeps a per-page word count so the panel can tell an editor which pages
//! have grown long. It asks for `storage` and nothing else: the word count
//! comes out of the HTML the host already hands the hook, so it never needs to
//! read a page through the action registry.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

// Storage keys. The first three are the ones the original plugin used, so a
// site that ran it keeps its settings.
const ENABLED: &str = "enabled";
const RATE: &str = "wpm";
const LABEL: &str = "label";
const STYLE: &str = "style";
const ACCENT: &str = "accent";
const SKIP: &str = "skip";
const COUNT_PREFIX: &str = "count:";

// Panel field names (the manifest's).
const F_RATE: &str = "words-per-minute";
const F_SKIP: &str = "skip-pages";

const DEFAULT_RATE: u32 = 200;
const MIN_RATE: u32 = 60;
const MAX_RATE: u32 = 600;
const DEFAULT_LABEL: &str = "min read";
const MAX_LABEL: usize = 40;
const MAX_SKIP: usize = 100;

/// The class on the line, and the marker that makes the hook idempotent.
const MARKER: &str = "stride-reading-time";

/// A key is at most 128 bytes; a very long slug simply goes unrecorded.
const MAX_KEY: usize = 128;

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = Settings::load();
    match render(&page.html, &page.slug, &settings) {
        Some((html, words)) => {
            remember(&page.slug, words);
            Ok(Json(PageRendered { html }))
        }
        None => Ok(Json(PageRendered { html: page.html })),
    }
}

/// The page with a reading time and the number of words counted, or `None` to
/// leave the page exactly as it arrived.
pub fn render(html: &str, slug: &str, settings: &Settings) -> Option<(String, u32)> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let slug = slug.trim_matches('/');
    if settings.skip.iter().any(|skipped| skipped == slug) {
        return None;
    }
    let words = count_words(content(html)?);
    if words == 0 {
        return None;
    }
    let minutes = minutes_for(words, settings.rate);
    let badge = badge(words, minutes, settings);
    let mut out = inject(html, &badge)?;
    if settings.badge {
        let css = style(settings);
        match find_ci(out.as_bytes(), b"</head>") {
            Some(head) => out.insert_str(head, &css),
            // No head: put the style right in front of the line.
            None => {
                let at = out.find(&badge)?;
                out.insert_str(at, &css);
            }
        }
    }
    Some((out, words))
}

/// Store this page's word count, if there is anywhere to store it. Every way
/// this can fail is a way the plugin carries on: storage may not have been
/// granted, and a very large site will fill the bag.
fn remember(slug: &str, words: u32) {
    let key = format!("{COUNT_PREFIX}{slug}");
    if key.len() > MAX_KEY {
        return;
    }
    if kv::get::<u32>(&key).ok().flatten() == Some(words) {
        return;
    }
    if let Err(error) = kv::set(&key, &words)
        && !error.is_permission_denied()
    {
        stride_pdk::log("info", &format!("reading time not recorded: {error}"));
    }
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_reading_time(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: Settings::load().to_values(),
            message: report(),
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let settings = match Settings::from_values(&request.values) {
                Ok(settings) => settings,
                Err(error) => {
                    return Ok(Json(PanelResponse {
                        values: Settings::load().to_values(),
                        message: String::new(),
                        error,
                    }));
                }
            };
            if let Err(error) = settings.save() {
                return Ok(Json(PanelResponse {
                    values: settings.to_values(),
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is \
                         nowhere to keep these settings. It still shows a reading time on \
                         every page, with the defaults."
                            .to_owned()
                    } else {
                        format!("Nothing was saved: {error}")
                    },
                }));
            }
            let message = if settings.enabled {
                format!(
                    "Saved. Pages show a reading time at {} words a minute from the next \
                     time they are published.",
                    settings.rate
                )
            } else {
                "Saved. The reading time is off on this site.".to_owned()
            };
            Ok(Json(PanelResponse {
                values: settings.to_values(),
                message,
                error: String::new(),
            }))
        }
    }
}

/// One line about what has been counted so far. Empty when there is nothing to
/// say, which includes the case where storage was refused.
fn report() -> String {
    let Ok(keys) = kv::list(COUNT_PREFIX) else {
        return String::new();
    };
    let mut pages = 0u32;
    let mut total = 0u64;
    let mut longest_slug = String::new();
    let mut longest = 0u32;
    for key in &keys {
        let words = kv::get::<u32>(key).ok().flatten().unwrap_or(0);
        pages += 1;
        total += u64::from(words);
        if words > longest {
            longest = words;
            longest_slug = key.strip_prefix(COUNT_PREFIX).unwrap_or(key).to_owned();
        }
    }
    if pages == 0 {
        return String::new();
    }
    let rate = Settings::load().rate;
    let pages_text = if pages == 1 { "1 page".to_owned() } else { format!("{pages} pages") };
    format!(
        "Counted {pages_text} so far, {total} words in all. The longest is \"{longest_slug}\": \
         {longest} words, about {} min.",
        minutes_for(longest, rate)
    )
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub rate: u32,
    pub label: String,
    /// The pill with a clock; `false` is plain text with no CSS at all.
    pub badge: bool,
    /// `#rrggbb`, or empty for the text colour of the page.
    pub accent: String,
    pub skip: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            rate: DEFAULT_RATE,
            label: DEFAULT_LABEL.to_owned(),
            badge: true,
            accent: String::new(),
            skip: Vec::new(),
        }
    }
}

impl Settings {
    /// Every read is allowed to fail: a plugin installed without `storage`
    /// still shows a reading time, it just shows the default one.
    pub fn load() -> Self {
        let d = Settings::default();
        Settings {
            enabled: kv::get::<bool>(ENABLED).ok().flatten().unwrap_or(d.enabled),
            rate: kv::get::<u32>(RATE)
                .ok()
                .flatten()
                .filter(|rate| (MIN_RATE..=MAX_RATE).contains(rate))
                .unwrap_or(d.rate),
            label: kv::get::<String>(LABEL)
                .ok()
                .flatten()
                .map(|label| clean_label(&label))
                .filter(|label| !label.is_empty())
                .unwrap_or(d.label),
            badge: kv::get::<String>(STYLE).ok().flatten().as_deref() != Some("plain"),
            accent: kv::get::<String>(ACCENT)
                .ok()
                .flatten()
                .and_then(|color| valid_color(&color))
                .unwrap_or_default(),
            skip: kv::get::<String>(SKIP)
                .ok()
                .flatten()
                .map(|text| parse_slugs(&text))
                .unwrap_or_default(),
        }
    }

    fn save(&self) -> Result<(), stride_pdk::HostError> {
        kv::set(ENABLED, &self.enabled)?;
        kv::set(RATE, &self.rate)?;
        kv::set(LABEL, &self.label)?;
        kv::set(STYLE, &self.style_name())?;
        kv::set(ACCENT, &self.accent)?;
        kv::set(SKIP, &self.skip.join(", "))
    }

    fn style_name(&self) -> &'static str {
        if self.badge { "badge" } else { "plain" }
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert(ENABLED.into(), self.enabled.into());
        values.insert(F_RATE.into(), self.rate.into());
        values.insert(LABEL.into(), self.label.clone().into());
        values.insert(STYLE.into(), self.style_name().into());
        if !self.accent.is_empty() {
            values.insert(ACCENT.into(), self.accent.clone().into());
        }
        values.insert(F_SKIP.into(), self.skip.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let rate = values
            .get(F_RATE)
            .and_then(JsonValue::as_i64)
            .unwrap_or(i64::from(d.rate));
        if rate < i64::from(MIN_RATE) || rate > i64::from(MAX_RATE) {
            return Err(format!(
                "A reading rate has to be between {MIN_RATE} and {MAX_RATE} words a minute. \
                 Nothing was saved."
            ));
        }
        let accent = match values.get(ACCENT).and_then(JsonValue::as_str).map(str::trim) {
            None | Some("") => String::new(),
            Some(color) => valid_color(color).ok_or_else(|| {
                "The accent colour has to look like #0f766e. Nothing was saved.".to_owned()
            })?,
        };
        let label = values
            .get(LABEL)
            .and_then(JsonValue::as_str)
            .map(clean_label)
            .filter(|label| !label.is_empty())
            .unwrap_or(d.label);
        Ok(Settings {
            enabled: values.get(ENABLED).and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            rate: rate as u32,
            label,
            badge: values.get(STYLE).and_then(JsonValue::as_str) != Some("plain"),
            accent,
            skip: values
                .get(F_SKIP)
                .and_then(JsonValue::as_str)
                .map(parse_slugs)
                .unwrap_or_default(),
        })
    }
}

/// Whitespace collapsed, at most 40 characters.
fn clean_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_LABEL)
        .collect()
}

/// `#rgb` or `#rrggbb`, lowercased. Anything else is refused, which is what
/// keeps the value safe to put into CSS.
pub fn valid_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if (hex.len() == 3 || hex.len() == 6) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_ascii_lowercase()))
    } else {
        None
    }
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(MAX_SKIP)
        .collect()
}

// ------------------------------------------------------------ the markup

const CLOCK: &str = "<svg aria-hidden=\"true\" focusable=\"false\" viewBox=\"0 0 24 24\" \
width=\"16\" height=\"16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.25\" \
stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"9\"/>\
<path d=\"M12 7v5l3 2\"/></svg>";

fn badge(words: u32, minutes: u32, settings: &Settings) -> String {
    // `label` was typed by an administrator and is going into HTML.
    let label = escape(&settings.label);
    if settings.badge {
        format!(
            "<p class=\"{MARKER} {MARKER}--badge\" data-words=\"{words}\">{CLOCK}\
<span>{minutes} {label}</span></p>"
        )
    } else {
        format!("<p class=\"{MARKER}\" data-words=\"{words}\">{minutes} {label}</p>")
    }
}

fn style(settings: &Settings) -> String {
    let accent = if settings.accent.is_empty() {
        "currentColor"
    } else {
        settings.accent.as_str()
    };
    format!(
        "<style>.{MARKER}--badge{{--rt:{accent};display:flex;align-items:center;gap:.45em;\
width:fit-content;margin:.75rem 0 1.5rem;padding:.3em .85em .3em .65em;font-size:.875rem;\
font-weight:600;line-height:1.3;letter-spacing:.01em;font-variant-numeric:tabular-nums;\
border-radius:999px;background:color-mix(in srgb,var(--rt) 9%,transparent);\
box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--rt) 22%,transparent)}}\
.{MARKER}--badge svg{{flex:none;width:1.1em;height:1.1em;color:var(--rt)}}</style>"
    )
}

// ------------------------------------------------------------ the plain work

/// The part of the page whose words are counted: the inside of `<main>`, or
/// the inside of `<body>`. `None` for a fragment with neither.
fn content(html: &str) -> Option<&str> {
    let bytes = html.as_bytes();
    for name in [&b"main"[..], b"body"] {
        if let Some(open) = find_tag(bytes, 0, name) {
            let start = open + html[open..].find('>')? + 1;
            let end = find_ci(&bytes[start..], &[b"</", name].concat()).map_or(html.len(), |o| start + o);
            return Some(&html[start..end]);
        }
    }
    None
}

/// Elements whose contents are not prose a visitor reads.
const SKIPPED: [&[u8]; 8] = [
    b"script", b"style", b"noscript", b"template", b"svg", b"nav", b"footer", b"aside",
];

/// Words in the visible text of a piece of HTML. Scripts, styles, navigation,
/// footers and asides are not counted, and neither are tag names or attributes.
pub fn count_words(html: &str) -> u32 {
    let bytes = html.as_bytes();
    let mut at = 0usize;
    let mut words = 0u32;
    let mut in_word = false;
    while at < bytes.len() {
        if bytes[at] == b'<' {
            in_word = false;
            let name = opening_name(&bytes[at..]);
            let close = match html[at..].find('>') {
                Some(offset) => at + offset,
                None => return words,
            };
            let self_closing = close > at && bytes[close - 1] == b'/';
            at = close + 1;
            if let Some(name) = name
                && !self_closing
                && SKIPPED.iter().any(|s| s.eq_ignore_ascii_case(name))
            {
                at = skip_element(bytes, at, name);
            }
            continue;
        }
        if bytes[at].is_ascii_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words = words.saturating_add(1);
        }
        at += 1;
    }
    words
}

/// The element name when `tag` (starting at `<`) opens an element.
fn opening_name(tag: &[u8]) -> Option<&[u8]> {
    let rest = tag.get(1..)?;
    let len = rest.iter().take_while(|b| b.is_ascii_alphanumeric()).count();
    if len == 0 { None } else { Some(&rest[..len]) }
}

/// The offset of the matching `</name`, from `from`, or the end of the input.
fn skip_element(bytes: &[u8], from: usize, name: &[u8]) -> usize {
    let mut at = from;
    while at + 2 + name.len() <= bytes.len() {
        if bytes[at] == b'<'
            && bytes[at + 1] == b'/'
            && bytes[at + 2..at + 2 + name.len()].eq_ignore_ascii_case(name)
            && !bytes.get(at + 2 + name.len()).is_some_and(u8::is_ascii_alphanumeric)
        {
            return at;
        }
        at += 1;
    }
    bytes.len()
}

/// Where `<name` opens an element (not `<names…`), from `from`.
fn find_tag(bytes: &[u8], from: usize, name: &[u8]) -> Option<usize> {
    let mut at = from;
    while let Some(offset) = find_ci(&bytes[at..], &[b"<", name].concat()) {
        let start = at + offset;
        let next = bytes.get(start + 1 + name.len()).copied();
        if matches!(next, Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r')) {
            return Some(start);
        }
        at = start + 1;
    }
    None
}

/// Whole minutes to read `words` at `wpm`, never less than one.
pub fn minutes_for(words: u32, wpm: u32) -> u32 {
    if wpm == 0 {
        return 1;
    }
    words.div_ceil(wpm).max(1)
}

/// Escape text destined for HTML character data or an attribute.
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

/// Put `badge` into `html` after the first `</h1>` inside the body, or right
/// after `<body …>`. Returns `None` when there is nowhere sensible to put it.
pub fn inject(html: &str, badge: &str) -> Option<String> {
    let body = find_tag(html.as_bytes(), 0, b"body")?;
    let opened = html[body..].find('>').map(|offset| body + offset + 1)?;
    let at = match find_ci(&html.as_bytes()[opened..], b"</h1>") {
        Some(offset) => opened + offset + "</h1>".len(),
        None => opened,
    };
    let mut out = String::with_capacity(html.len() + badge.len() + 600);
    out.push_str(&html[..at]);
    out.push_str(badge);
    out.push_str(&html[at..]);
    Some(out)
}

/// `needle` must be ASCII, which is what keeps the answer on a char boundary.
fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><style>body{color:red}</style></head>\
<body><nav>Home Shop Journal</nav><h1>About Atelier</h1><p>One two three four five.</p>\
<script>var x = 'six seven eight';</script><footer>Legal stuff here</footer></body></html>";

    #[test]
    fn counts_only_visible_prose() {
        assert_eq!(count_words(content(PAGE).unwrap()), 7);
    }

    #[test]
    fn prefers_main_over_body() {
        let page = "<html><body><header>Big Brand</header><main><h1>T</h1><p>a b</p></main>\
<div>ignored words</div></body></html>";
        assert_eq!(count_words(content(page).unwrap()), 3);
    }

    #[test]
    fn a_navigator_element_is_not_a_nav() {
        assert_eq!(count_words("<navx>one two</navx>"), 2);
        assert_eq!(count_words("<svg/> three"), 1);
    }

    #[test]
    fn counts_nothing_in_an_empty_page() {
        assert_eq!(render("<html><body></body></html>", "x", &Settings::default()), None);
    }

    #[test]
    fn treats_entities_and_punctuation_as_one_word_each() {
        assert_eq!(count_words("<p>Well &mdash; done, really.</p>"), 4);
    }

    #[test]
    fn rounding() {
        assert_eq!(minutes_for(1, 200), 1);
        assert_eq!(minutes_for(0, 200), 1);
        assert_eq!(minutes_for(200, 200), 1);
        assert_eq!(minutes_for(201, 200), 2);
        assert_eq!(minutes_for(500, 0), 1);
    }

    #[test]
    fn escapes_the_five() {
        assert_eq!(escape("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[test]
    fn renders_a_badge_after_the_first_heading_with_style_in_head() {
        let (out, words) = render(PAGE, "about", &Settings::default()).unwrap();
        assert_eq!(words, 7);
        assert!(out.contains("</h1><p class=\"stride-reading-time stride-reading-time--badge\" \
data-words=\"7\"><svg aria-hidden=\"true\""), "got: {out}");
        assert!(out.contains("<span>1 min read</span></p><p>One"));
        assert!(out.contains("--rt:currentColor") && out.contains("</style></head>"));
        assert_eq!(out.matches("data-words").count(), 1);
        // Idempotent.
        assert_eq!(render(&out, "about", &Settings::default()), None);
    }

    #[test]
    fn plain_style_has_no_css_and_the_original_markup() {
        let settings = Settings { badge: false, ..Settings::default() };
        let (out, _) = render(PAGE, "about", &settings).unwrap();
        assert!(out.contains("</h1><p class=\"stride-reading-time\" data-words=\"7\">1 min read</p>"));
        assert!(!out.contains("--rt"));
    }

    #[test]
    fn escapes_the_label_and_uses_the_accent() {
        let settings = Settings {
            label: "<b>min</b>".into(),
            accent: "#0f766e".into(),
            ..Settings::default()
        };
        let (out, _) = render(PAGE, "about", &settings).unwrap();
        assert!(out.contains("1 &lt;b&gt;min&lt;/b&gt;"));
        assert!(out.contains("--rt:#0f766e"));
    }

    #[test]
    fn skips_pages_and_honours_off() {
        let skip = Settings { skip: parse_slugs("home, /about/"), ..Settings::default() };
        assert_eq!(render(PAGE, "/about", &skip), None);
        let off = Settings { enabled: false, ..Settings::default() };
        assert_eq!(render(PAGE, "about", &off), None);
    }

    #[test]
    fn falls_back_to_the_body_and_refuses_fragments() {
        let out = inject("<html><body class=\"x\"><p>Hi</p></body></html>", "B").unwrap();
        assert!(out.starts_with("<html><body class=\"x\">B<p>Hi</p>"), "got: {out}");
        assert_eq!(inject("<p>a fragment</p>", "B"), None);
        assert_eq!(render("<p>a fragment</p>", "x", &Settings::default()), None);
    }

    #[test]
    fn a_page_without_head_gets_its_style_inline() {
        let (out, _) = render("<body><h1>T</h1><p>x y</p></body>", "x", &Settings::default()).unwrap();
        assert!(out.starts_with("<body><h1>T</h1><style>"), "got: {out}");
    }

    #[test]
    fn settings_from_the_panel() {
        let mut values = Map::new();
        values.insert(F_RATE.into(), 238.into());
        values.insert(LABEL.into(), "  minuten   leestijd ".into());
        values.insert(STYLE.into(), "plain".into());
        values.insert(ACCENT.into(), "#ABC".into());
        values.insert(F_SKIP.into(), "home,contact".into());
        let s = Settings::from_values(&values).unwrap();
        assert_eq!(s.rate, 238);
        assert_eq!(s.label, "minuten leestijd");
        assert!(!s.badge);
        assert_eq!(s.accent, "#abc");
        assert_eq!(s.skip, vec!["home", "contact"]);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);

        values.insert(F_RATE.into(), 20.into());
        assert!(Settings::from_values(&values).is_err());
        values.insert(F_RATE.into(), 200.into());
        values.insert(ACCENT.into(), "red;}".into());
        assert!(Settings::from_values(&values).is_err());
        assert_eq!(Settings::from_values(&Map::new()).unwrap(), Settings::default());
    }
}
