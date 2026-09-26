//! Schema Markup — a Stride plugin.
//!
//! Adds one `<script type="application/ld+json">` to the head of every
//! published page, holding a schema.org `@graph`:
//!
//! - the Organization, LocalBusiness (or Person) behind the site, with the
//!   address, opening hours, phone and profiles typed into the panel;
//! - the WebSite;
//! - a BlogPosting for pages under the configured post paths, drawn from the
//!   page's own `<h1>`, description, social image and first `<time>`.
//!
//! It asks for `storage` only, to keep the panel's settings. Without it the
//! plugin still works: it names the site after the home page title and marks
//! up posts under `blog/`, `news/` and `posts/`.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::serde_json::{self, json};
use stride_pdk::{JsonValue, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Values, kv};

const SETTINGS_KEY: &str = "settings";
const MARKER: &str = "data-stride-schema";
const DEFAULT_POST_PATHS: &str = "blog-, news-, blog/, news/";

/// Panel value to schema.org type.
const TYPES: [(&str, &str); 6] = [
    ("organization", "Organization"),
    ("local-business", "LocalBusiness"),
    ("store", "Store"),
    ("restaurant", "Restaurant"),
    ("professional-service", "ProfessionalService"),
    ("person", "Person"),
];

/// Every text field of the panel, in manifest order.
const TEXT_FIELDS: [&str; 14] = [
    "name",
    "site-url",
    "logo",
    "phone",
    "email",
    "street",
    "postal-code",
    "city",
    "country",
    "hours",
    "price-range",
    "same-as",
    "post-paths",
    "author",
];

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = Settings::load();
    let html = render(&page.html, &page.slug, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

/// The page with its structured data, or `None` to leave it untouched.
pub fn render(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) {
        return None;
    }
    let head_end = find_ci(html.as_bytes(), b"</head>")?;
    let graph = graph(html, slug, settings)?;
    let script = format!(
        "<script type=\"application/ld+json\" {MARKER}>{}</script>",
        script_safe(&serde_json::to_string(&graph).ok()?)
    );
    let mut out = String::with_capacity(html.len() + script.len());
    out.push_str(&html[..head_end]);
    out.push_str(&script);
    out.push_str(&html[head_end..]);
    Some(out)
}

/// The `@graph` for one page. `None` when there is nothing worth saying.
pub fn graph(html: &str, slug: &str, settings: &Settings) -> Option<JsonValue> {
    let page = PageFacts::read(html);
    let origin = settings
        .text("site-url")
        .and_then(origin_of)
        .or_else(|| page.canonical.as_deref().and_then(origin_of));
    let absolute = |url: &str| -> Option<String> {
        if url.starts_with("https://") || url.starts_with("http://") {
            Some(url.to_owned())
        } else if url.starts_with('/') && !url.starts_with("//") {
            origin.as_ref().map(|origin| format!("{origin}{url}"))
        } else {
            None
        }
    };
    let page_url = page
        .canonical
        .as_deref()
        .and_then(|url| absolute(url))
        .or_else(|| {
            let path = if slug == "home" { String::new() } else { slug.to_owned() };
            origin.as_ref().map(|origin| format!("{origin}/{path}"))
        });

    let name = settings
        .text("name")
        .map(str::to_owned)
        .or_else(|| site_name_from_title(page.title.as_deref()?, slug == "home"));

    let mut nodes: Vec<JsonValue> = Vec::new();
    let org_id = origin.as_ref().map(|origin| format!("{origin}/#organization"));

    if let Some(name) = &name {
        let kind = settings.kind();
        let mut org = Values::new();
        org.insert("@type".into(), kind.into());
        if let Some(id) = &org_id {
            org.insert("@id".into(), id.clone().into());
        }
        org.insert("name".into(), name.clone().into());
        if let Some(origin) = &origin {
            org.insert("url".into(), format!("{origin}/").into());
        }
        if let Some(logo) = settings.text("logo").and_then(|logo| absolute(logo)) {
            let key = if kind == "Person" { "image" } else { "logo" };
            org.insert(key.into(), logo.clone().into());
            if kind != "Person" && kind != "Organization" {
                org.insert("image".into(), logo.into());
            }
        }
        if let Some(phone) = settings.text("phone") {
            org.insert("telephone".into(), phone.into());
        }
        if let Some(email) = settings.text("email") {
            org.insert("email".into(), email.into());
        }
        let address = settings.address();
        if let Some(address) = address {
            org.insert("address".into(), address);
        }
        if is_local(kind) {
            let hours = parse_hours(settings.text("hours").unwrap_or("")).unwrap_or_default();
            if !hours.is_empty() {
                org.insert("openingHoursSpecification".into(), JsonValue::Array(hours));
            }
            if let Some(range) = settings.text("price-range") {
                org.insert("priceRange".into(), range.into());
            }
        }
        let same_as: Vec<JsonValue> = settings
            .text("same-as")
            .unwrap_or("")
            .split(['\n', ',', ' '])
            .map(str::trim)
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            .map(JsonValue::from)
            .collect();
        if !same_as.is_empty() {
            org.insert("sameAs".into(), JsonValue::Array(same_as));
        }
        nodes.push(JsonValue::Object(org));

        if let Some(origin) = &origin {
            let mut site = json!({
                "@type": "WebSite",
                "@id": format!("{origin}/#website"),
                "url": format!("{origin}/"),
                "name": name,
            });
            if let Some(lang) = &page.lang {
                site["inLanguage"] = lang.clone().into();
            }
            if let Some(id) = &org_id {
                site["publisher"] = json!({ "@id": id });
            }
            nodes.push(site);
        }
    }

    if settings.is_post(slug)
        && let Some(headline) = page.h1.clone().or_else(|| page.title.clone())
    {
        let mut post = Values::new();
        post.insert("@type".into(), "BlogPosting".into());
        if let Some(url) = &page_url {
            post.insert("@id".into(), format!("{url}#article").into());
            post.insert("mainEntityOfPage".into(), url.clone().into());
            post.insert("url".into(), url.clone().into());
        }
        post.insert("headline".into(), truncate(&headline, 110).into());
        if let Some(description) = &page.description {
            post.insert("description".into(), description.clone().into());
        }
        if let Some(image) = page.image.as_deref().and_then(|image| absolute(image)) {
            post.insert("image".into(), json!([image]));
        }
        if let Some(date) = &page.published {
            post.insert("datePublished".into(), date.clone().into());
        }
        if let Some(date) = &page.modified {
            post.insert("dateModified".into(), date.clone().into());
        }
        if let Some(lang) = &page.lang {
            post.insert("inLanguage".into(), lang.clone().into());
        }
        let publisher = match (&org_id, &name) {
            (Some(id), _) if name.is_some() => Some(json!({ "@id": id })),
            (_, Some(name)) => Some(json!({ "@type": settings.org_type(), "name": name })),
            _ => None,
        };
        let author = match settings.text("author") {
            Some(author) => Some(json!({ "@type": "Person", "name": author })),
            None => publisher.clone(),
        };
        if let Some(author) = author {
            post.insert("author".into(), author);
        }
        if let Some(publisher) = publisher {
            post.insert("publisher".into(), publisher);
        }
        if let Some(words) = page.words.filter(|words| *words > 0) {
            post.insert("wordCount".into(), words.into());
        }
        nodes.push(JsonValue::Object(post));
    }

    if nodes.is_empty() {
        return None;
    }
    Some(json!({ "@context": "https://schema.org", "@graph": nodes }))
}

fn is_local(kind: &str) -> bool {
    !matches!(kind, "Organization" | "Person")
}

/// `https://example.com` out of any absolute http(s) URL.
fn origin_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .map(|rest| ("https://", rest))
        .or_else(|| url.strip_prefix("http://").map(|rest| ("http://", rest)))?;
    let host = rest.1.split(['/', '?', '#']).next()?;
    if host.is_empty() || host.contains(['"', '<', '>', ' ', '\\']) {
        return None;
    }
    Some(format!("{}{host}", rest.0))
}

/// The site's name from a `<title>`: the part after the last " | ", " – ",
/// " — " or " - ", or the whole title on the home page.
pub fn site_name_from_title(title: &str, home: bool) -> Option<String> {
    for separator in [" | ", " – ", " — ", " - ", " · "] {
        if let Some((_, tail)) = title.rsplit_once(separator) {
            let tail = tail.trim();
            if !tail.is_empty() {
                return Some(tail.to_owned());
            }
        }
    }
    (home && !title.trim().is_empty()).then(|| title.trim().to_owned())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// JSON is about to sit inside a `<script>`: `</script>` or `<!--` in a
/// string must not end it. `<`, `>` and `&` only occur inside JSON strings,
/// where their `\u` escapes mean the same thing.
pub fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

// ------------------------------------------------------- reading the page

/// What the rendered page already says about itself.
#[derive(Debug, Default)]
pub struct PageFacts {
    title: Option<String>,
    h1: Option<String>,
    description: Option<String>,
    image: Option<String>,
    canonical: Option<String>,
    lang: Option<String>,
    published: Option<String>,
    modified: Option<String>,
    words: Option<u32>,
}

impl PageFacts {
    pub fn read(html: &str) -> Self {
        let bytes = html.as_bytes();
        let mut facts = PageFacts {
            title: element_text(html, b"title"),
            h1: element_text(html, b"h1"),
            ..PageFacts::default()
        };
        if let Some(at) = find_ci(bytes, b"<html") {
            facts.lang = attribute(tag_at(html, at), "lang").filter(|lang| {
                !lang.is_empty() && lang.len() <= 20 && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            });
        }
        let mut at = 0;
        while let Some(offset) = find_ci(&bytes[at..], b"<meta") {
            let tag = tag_at(html, at + offset);
            let key = attribute(tag, "name").or_else(|| attribute(tag, "property"));
            let content = attribute(tag, "content").filter(|content| !content.is_empty());
            match (key.as_deref(), content) {
                (Some("description"), Some(content)) => facts.description = Some(content),
                (Some("og:description"), Some(content)) if facts.description.is_none() => {
                    facts.description = Some(content);
                }
                (Some("og:image"), Some(content)) => facts.image = Some(content),
                (Some("article:published_time"), Some(content)) => facts.published = Some(content),
                (Some("article:modified_time"), Some(content)) => facts.modified = Some(content),
                (Some("og:url"), Some(content)) if facts.canonical.is_none() => {
                    facts.canonical = Some(content);
                }
                _ => {}
            }
            at += offset + 5;
        }
        let mut at = 0;
        while let Some(offset) = find_ci(&bytes[at..], b"<link") {
            let tag = tag_at(html, at + offset);
            if attribute(tag, "rel").is_some_and(|rel| rel.eq_ignore_ascii_case("canonical"))
                && let Some(href) = attribute(tag, "href")
            {
                facts.canonical = Some(href);
            }
            at += offset + 5;
        }
        if facts.published.is_none() {
            let mut at = 0;
            while let Some(offset) = find_ci(&bytes[at..], b"<time") {
                let tag = tag_at(html, at + offset);
                if let Some(date) = attribute(tag, "datetime").filter(|date| looks_like_date(date)) {
                    facts.published = Some(date);
                    break;
                }
                at += offset + 5;
            }
        }
        if let Some(body) = find_ci(bytes, b"<body") {
            facts.words = Some(count_words(&html[body..]));
        }
        facts
    }
}

/// `2026-03-14`, optionally followed by a time.
fn looks_like_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '+'))
}

/// The opening tag that starts at `at`, up to and including `>`.
fn tag_at(html: &str, at: usize) -> &str {
    let rest = &html[at..];
    // A `>` inside a quoted attribute value does not end the tag.
    let mut quote: Option<char> = None;
    for (index, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return &rest[..=index],
            _ => {}
        }
    }
    rest
}

/// The decoded value of `name="…"` in one tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = lower[from..].find(name) {
        let at = from + offset;
        from = at + name.len();
        let before = bytes.get(at.wrapping_sub(1)).copied().unwrap_or(b' ');
        if at == 0 || !before.is_ascii_whitespace() {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split(|c: char| c.is_ascii_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode(value.trim()));
    }
    None
}

/// The text of the first `<name …>…</name>`, tags stripped, whitespace
/// collapsed and entities decoded.
fn element_text(html: &str, name: &[u8]) -> Option<String> {
    let bytes = html.as_bytes();
    let mut open = vec![b'<'];
    open.extend_from_slice(name);
    let mut at = 0;
    let start = loop {
        let offset = find_ci(&bytes[at..], &open)?;
        let next = bytes.get(at + offset + open.len()).copied().unwrap_or(b'>');
        if next == b'>' || next.is_ascii_whitespace() {
            break at + offset;
        }
        at += offset + open.len();
    };
    let inner_start = start + tag_at(html, start).len();
    let mut close = b"</".to_vec();
    close.extend_from_slice(name);
    let inner_end = inner_start + find_ci(&bytes[inner_start..], &close)?;
    let text = strip_tags(&html[inner_start..inner_end]);
    let text = decode(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    (!text.is_empty()).then_some(text)
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// The entities a renderer writes, and numeric ones.
pub fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..].find(';').map(|end| end + 1).filter(|end| *end <= 10);
        let decoded = end.and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                _ => {
                    let number = entity.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, end + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Visible words, skipping `script` and `style`.
fn count_words(html: &str) -> u32 {
    let lower = html.to_ascii_lowercase();
    let mut words = 0u32;
    let mut in_word = false;
    let mut at = 0;
    let bytes = html.as_bytes();
    while at < bytes.len() {
        if bytes[at] == b'<' {
            in_word = false;
            let skip_to = if lower[at..].starts_with("<script") {
                lower[at..].find("</script").map(|end| at + end)
            } else if lower[at..].starts_with("<style") {
                lower[at..].find("</style").map(|end| at + end)
            } else {
                None
            };
            let from = skip_to.unwrap_or(at);
            at = match html[from..].find('>') {
                Some(offset) => from + offset + 1,
                None => bytes.len(),
            };
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

fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

// ------------------------------------------------------------ opening hours

const DAYS: [(&str, &str); 7] = [
    ("mo", "Monday"),
    ("tu", "Tuesday"),
    ("we", "Wednesday"),
    ("th", "Thursday"),
    ("fr", "Friday"),
    ("sa", "Saturday"),
    ("su", "Sunday"),
];

/// `Mo-Fr 09:00-17:30` per line into `OpeningHoursSpecification`s. Days may
/// be a range, a comma list, or both (`Mo-We,Fr`). An error names the line.
pub fn parse_hours(text: &str) -> Result<Vec<JsonValue>, String> {
    let mut specs = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let bad = || {
            format!(
                "\"{line}\" is not an opening-hours line. Write it like \"Mo-Fr 09:00-17:30\"."
            )
        };
        let (days, times) = line.rsplit_once(char::is_whitespace).ok_or_else(bad)?;
        let (opens, closes) = times.split_once(['-', '–']).ok_or_else(bad)?;
        let opens = time(opens).ok_or_else(bad)?;
        let closes = time(closes).ok_or_else(bad)?;
        let mut names = Vec::new();
        for part in days.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
            let (from, to) = part.split_once(['-', '–']).unwrap_or((part, part));
            let from = day(from).ok_or_else(bad)?;
            let to = day(to).ok_or_else(bad)?;
            let mut index = from;
            loop {
                let name = DAYS[index].1;
                if !names.contains(&name) {
                    names.push(name);
                }
                if index == to {
                    break;
                }
                index = (index + 1) % 7;
            }
        }
        if names.is_empty() {
            return Err(bad());
        }
        specs.push(json!({
            "@type": "OpeningHoursSpecification",
            "dayOfWeek": names,
            "opens": opens,
            "closes": closes,
        }));
    }
    Ok(specs)
}

fn day(text: &str) -> Option<usize> {
    let text = text.trim().to_ascii_lowercase();
    let short = text.get(..2)?;
    DAYS.iter().position(|(code, full)| {
        *code == short && (text.len() == 2 || full.to_ascii_lowercase().starts_with(&text))
    })
}

/// `9:00`, `09:00` or `9` into `09:00`.
fn time(text: &str) -> Option<String> {
    let text = text.trim();
    let (hours, minutes) = text.split_once([':', '.']).unwrap_or((text, "00"));
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    (hours <= 24 && minutes < 60 && minutes_len(minutes, text)).then(|| format!("{hours:02}:{minutes:02}"))
}

fn minutes_len(_minutes: u32, text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

// --------------------------------------------------------------- settings

#[derive(Debug, Clone)]
pub struct Settings {
    pub enabled: bool,
    pub values: Values,
}

impl Default for Settings {
    fn default() -> Self {
        let mut values = Values::new();
        values.insert("type".into(), "organization".into());
        values.insert("post-paths".into(), DEFAULT_POST_PATHS.into());
        Settings { enabled: true, values }
    }
}

impl Settings {
    /// Every failure, a refused permission included, means the defaults.
    pub fn load() -> Self {
        match kv::get::<Values>(SETTINGS_KEY) {
            Ok(Some(values)) => Settings::from_values(&values),
            _ => Settings::default(),
        }
    }

    pub fn from_values(values: &Values) -> Self {
        let mut settings = Settings::default();
        settings.enabled = values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(true);
        if let Some(kind) = values.get("type").and_then(JsonValue::as_str)
            && TYPES.iter().any(|(value, _)| *value == kind)
        {
            settings.values.insert("type".into(), kind.into());
        }
        for field in TEXT_FIELDS {
            if let Some(text) = values.get(field).and_then(JsonValue::as_str) {
                settings.values.insert(field.into(), text.trim().into());
            }
        }
        settings
    }

    fn text(&self, field: &str) -> Option<&str> {
        self.values
            .get(field)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }

    /// The schema.org type.
    fn kind(&self) -> &str {
        let value = self.text("type").unwrap_or("organization");
        TYPES.iter().find(|(v, _)| *v == value).map_or("Organization", |(_, kind)| kind)
    }

    fn org_type(&self) -> &str {
        if self.kind() == "Person" { "Person" } else { "Organization" }
    }

    fn is_post(&self, slug: &str) -> bool {
        self.text("post-paths")
            .unwrap_or("")
            .split([',', '\n'])
            .map(|path| path.trim().trim_start_matches('/'))
            .filter(|path| !path.is_empty())
            .any(|path| {
                // `blog-` or `blog/` as typed; a bare `blog` means either.
                // The post index itself (`blog`) is not a post.
                let prefixes: Vec<String> = if path.ends_with(['-', '/']) {
                    vec![path.to_owned()]
                } else {
                    vec![format!("{path}-"), format!("{path}/")]
                };
                prefixes
                    .iter()
                    .any(|prefix| slug.len() > prefix.len() && slug.starts_with(prefix.as_str()))
            })
    }

    fn address(&self) -> Option<JsonValue> {
        let mut address = Values::new();
        for (field, key) in [
            ("street", "streetAddress"),
            ("postal-code", "postalCode"),
            ("city", "addressLocality"),
            ("country", "addressCountry"),
        ] {
            if let Some(value) = self.text(field) {
                let value = if field == "country" { value.to_ascii_uppercase() } else { value.to_owned() };
                address.insert(key.into(), value.into());
            }
        }
        if address.is_empty() {
            return None;
        }
        address.insert("@type".into(), "PostalAddress".into());
        Some(JsonValue::Object(address))
    }

    fn to_values(&self) -> Values {
        let mut values = self.values.clone();
        values.insert("enabled".into(), self.enabled.into());
        for field in TEXT_FIELDS {
            values.entry(field).or_insert_with(|| "".into());
        }
        values
    }

    /// What is wrong with these settings, in words for the editor.
    fn problem(&self) -> Option<String> {
        for field in ["site-url", "logo"] {
            if let Some(url) = self.text(field) {
                let ok = if field == "logo" && url.starts_with('/') && !url.starts_with("//") {
                    true
                } else {
                    origin_of(url).is_some()
                };
                if !ok || url.contains(['"', '<', '>', ' ']) {
                    return Some(format!(
                        "\"{url}\" is not a web address. Start it with https://. Nothing was saved."
                    ));
                }
            }
        }
        if let Some(country) = self.text("country")
            && (country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return Some("The country is a two-letter code, such as NL or US. Nothing was saved.".into());
        }
        if let Some(phone) = self.text("phone")
            && !phone.chars().all(|c| c.is_ascii_digit() || " +-().".contains(c))
        {
            return Some("A phone number has digits, spaces, +, -, and brackets only. Nothing was saved.".into());
        }
        if let Some(email) = self.text("email")
            && (!email.contains('@') || email.contains(char::is_whitespace))
        {
            return Some(format!("\"{email}\" is not an email address. Nothing was saved."));
        }
        if let Err(error) = parse_hours(self.text("hours").unwrap_or("")) {
            return Some(format!("{error} Nothing was saved."));
        }
        None
    }

    fn summary(&self) -> String {
        if !self.enabled {
            return "Structured data is off on this site.".into();
        }
        let kind = match self.kind() {
            "LocalBusiness" => "a local business",
            "Store" => "a shop",
            "Restaurant" => "a restaurant",
            "ProfessionalService" => "a professional service",
            "Person" => "a person",
            _ => "an organization",
        };
        let name = self.text("name").map_or_else(
            || "named after the home page title".to_owned(),
            |name| format!("\"{name}\""),
        );
        let hours = parse_hours(self.text("hours").unwrap_or("")).map_or(0, |h| h.len());
        let mut parts = vec![format!("Every page describes {kind} {name}")];
        if is_local(self.kind()) && hours > 0 {
            parts.push(format!("with {hours} opening-hours line(s)"));
        }
        format!(
            "{}. Pages under {} are marked up as blog posts. Check a page with Google's Rich Results Test.",
            parts.join(" "),
            self.text("post-paths").unwrap_or("(none)")
        )
    }
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_schema_markup(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let settings = Settings::load();
            let message = match kv::get::<Values>(SETTINGS_KEY) {
                Err(error) if error.is_permission_denied() => {
                    "This plugin was not granted storage, so these settings cannot be kept. It still adds an Organization, WebSite and blog-post markup using the defaults.".to_owned()
                }
                _ => settings.summary(),
            };
            Ok(Json(PanelResponse { values: settings.to_values(), message, error: String::new() }))
        }
        PanelEvent::Submit => {
            let settings = Settings::from_values(&request.values);
            if let Some(error) = settings.problem() {
                return Ok(Json(PanelResponse {
                    values: request.values,
                    message: String::new(),
                    error,
                }));
            }
            let values = settings.to_values();
            if let Err(error) = kv::set(SETTINGS_KEY, &values) {
                return Ok(Json(PanelResponse {
                    values,
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere to keep these settings. It keeps adding markup with its defaults.".into()
                    } else {
                        error.to_string()
                    },
                }));
            }
            Ok(Json(PanelResponse {
                values,
                message: format!("Saved. {}", settings.summary()),
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    const POST: &str = "<!doctype html><html lang=\"en\"><head><title>Why we roast light | Kade Coffee</title>\
<meta name=\"description\" content=\"Beans &amp; brewing.\"><meta property=\"og:image\" content=\"/media/beans.jpg\">\
<link rel=\"canonical\" href=\"https://kade.example/blog/light-roast\"></head>\
<body><h1>Why we <em>roast</em> light</h1><time datetime=\"2026-03-14\">14 March</time><p>One two three.</p>\
<script>var a = 'x y z';</script></body></html>";

    fn configured() -> Settings {
        let mut values = Values::new();
        values.insert("type".into(), "restaurant".into());
        values.insert("name".into(), "Kade </script> Coffee".into());
        values.insert("phone".into(), "+31 20 123 4567".into());
        values.insert("city".into(), "Amsterdam".into());
        values.insert("country".into(), "nl".into());
        values.insert("hours".into(), "Mo-Fr 8:00-17:30\nSa,Su 09:00-16:00".into());
        values.insert("same-as".into(), "https://instagram.com/kade\nnot a url".into());
        Settings::from_values(&values)
    }

    #[test]
    fn zero_config_marks_up_a_post() {
        let out = render(POST, "blog-light-roast", &Settings::default()).unwrap();
        assert!(render(POST, "blog/light-roast", &Settings::default()).unwrap().contains("BlogPosting"));
        assert!(!render(POST, "blogroll", &Settings::default()).unwrap().contains("BlogPosting"));
        assert!(out.contains("\"@type\":\"BlogPosting\""), "{out}");
        assert!(out.contains("\"headline\":\"Why we roast light\""), "{out}");
        assert!(out.contains("\"name\":\"Kade Coffee\""), "{out}");
        assert!(out.contains("\"image\":[\"https://kade.example/media/beans.jpg\"]"), "{out}");
        assert!(out.contains("\"datePublished\":\"2026-03-14\""), "{out}");
        assert!(out.contains("\"description\":\"Beans \\u0026 brewing.\""), "{out}");
        assert!(out.contains("\"wordCount\":9"), "{out}");
        assert!(out.find("ld+json").unwrap() < out.find("</head>").unwrap());
    }

    #[test]
    fn nothing_twice_and_nothing_without_a_head() {
        let once = render(POST, "blog/x", &Settings::default()).unwrap();
        assert!(render(&once, "blog/x", &Settings::default()).is_none());
        assert!(render("<p>fragment</p>", "x", &Settings::default()).is_none());
    }

    #[test]
    fn the_blog_index_is_not_a_post() {
        let out = render(POST, "blog", &Settings::default()).unwrap();
        assert!(!out.contains("BlogPosting"));
        assert!(out.contains("\"@type\":\"WebSite\""));
    }

    #[test]
    fn a_local_business_carries_hours_and_cannot_break_out() {
        let out = render(POST, "home", &configured()).unwrap();
        assert!(out.contains("\"@type\":\"Restaurant\""), "{out}");
        assert!(!out.contains("</script> Coffee"), "{out}");
        assert!(out.contains("\\u003c/script\\u003e"), "{out}");
        assert!(out.contains("\"opens\":\"08:00\""), "{out}");
        assert!(out.contains("[\"Saturday\",\"Sunday\"]"), "{out}");
        assert!(out.contains("\"addressCountry\":\"NL\""), "{out}");
        assert!(out.contains("\"sameAs\":[\"https://instagram.com/kade\"]"), "{out}");
        let json_start = out.find("json\" data-stride-schema>").unwrap() + 25;
        let json_end = out[json_start..].find("</script>").unwrap() + json_start;
        let parsed: JsonValue = serde_json::from_str(&out[json_start..json_end]).unwrap();
        assert_eq!(parsed["@graph"][0]["name"], "Kade </script> Coffee");
    }

    #[test]
    fn disabled_leaves_the_page_alone() {
        let mut settings = Settings::default();
        settings.enabled = false;
        assert!(render(POST, "blog/x", &settings).is_none());
    }

    #[test]
    fn hours_parse_and_refuse() {
        let specs = parse_hours("Mo-We,Fr 09:00-17:00").unwrap();
        assert_eq!(specs[0]["dayOfWeek"], json!(["Monday", "Tuesday", "Wednesday", "Friday"]));
        assert!(parse_hours("Fr-Mo 10-2").unwrap()[0]["dayOfWeek"].as_array().unwrap().len() == 4);
        assert!(parse_hours("whenever").is_err());
        assert!(parse_hours("Xx 09:00-17:00").is_err());
        assert!(parse_hours("").unwrap().is_empty());
    }

    #[test]
    fn site_names_from_titles() {
        assert_eq!(site_name_from_title("About | Kade", false).as_deref(), Some("Kade"));
        assert_eq!(site_name_from_title("Kade", true).as_deref(), Some("Kade"));
        assert_eq!(site_name_from_title("About", false), None);
    }

    #[test]
    fn entities_decode() {
        assert_eq!(decode("a &amp; b &#39;c&#x27; &bogus; &"), "a & b 'c' &bogus; &");
    }
}
