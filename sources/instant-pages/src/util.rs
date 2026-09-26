//! Small HTML helpers shared in spirit with the other first-party plugins.
//! Each plugin carries its own copy so its build depends on nothing else here;
//! not every plugin uses every helper.
#![allow(dead_code)]

use stride_pdk::{JsonValue, Map};

/// Case-insensitive `find`; the offset is valid in `haystack` because
/// ASCII lowercasing never changes a byte length.
pub fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

pub fn rfind_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().rfind(needle)
}

/// Put `style` just before `</head>`, or at the very top of a page without one.
pub fn insert_in_head(html: &mut String, style: &str) {
    match find_ci(html, "</head>") {
        Some(at) => html.insert_str(at, style),
        None => html.insert_str(0, style),
    }
}

/// Put `markup` just before the last `</body>`, or at the very end without one.
pub fn insert_before_body_end(html: &mut String, markup: &str) {
    match rfind_ci(html, "</body>") {
        Some(at) => html.insert_str(at, markup),
        None => html.push_str(markup),
    }
}

/// Put `markup` right after the opening `<body …>`, or at the top without one.
pub fn insert_after_body_start(html: &mut String, markup: &str) {
    let at = find_ci(html, "<body").and_then(|at| html[at..].find('>').map(|c| at + c + 1));
    html.insert_str(at.unwrap_or(0), markup);
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
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

/// Only schemes that cannot run script; `//host` is refused as ambiguous.
pub fn safe_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with('#')
        || (lower.starts_with('/') && !lower.starts_with("//"));
    ok.then(|| url.to_owned())
}

pub fn is_colour(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Dark or light text by WCAG relative luminance, whichever contrasts more.
pub fn readable_on(colour: &str) -> &'static str {
    if !is_colour(colour) {
        return "#fff";
    }
    let channel = |i: usize| {
        let v = u8::from_str_radix(&colour[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
    if (1.05 / (l + 0.05)) >= ((l + 0.05) / 0.0616) { "#fff" } else { "#111827" }
}

/// "home, blog/*, /contact/" -> ["home", "blog/*", "contact"].
pub fn slug_list(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(|s| s.trim().trim_matches('/').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A trailing `*` matches every page under that path: `blog/*` matches
/// `blog/first-post` but not `blog` itself.
pub fn slug_matches(list: &[String], slug: &str) -> bool {
    let slug = slug.trim_matches('/').to_ascii_lowercase();
    list.iter().any(|pattern| match pattern.strip_suffix('*') {
        Some(prefix) => slug.starts_with(prefix) && slug.len() > prefix.len(),
        None => *pattern == slug,
    })
}

pub fn text(values: &Map<String, JsonValue>, name: &str) -> String {
    values.get(name).and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned()
}

pub fn flag(values: &Map<String, JsonValue>, name: &str, default: bool) -> bool {
    values.get(name).and_then(JsonValue::as_bool).unwrap_or(default)
}

pub fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193))
}
