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

// ------------------------------------------------------------- reading HTML

/// The inner HTML of the first `<name …>…</name>`, when there is one.
pub fn inner_of<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut at = 0;
    while let Some(start) = lower[at..].find(&open).map(|i| at + i) {
        let after = lower[start + open.len()..].chars().next();
        if matches!(after, Some(c) if c.is_whitespace() || c == '>') {
            let body = start + lower[start..].find('>')? + 1;
            let end = body + lower[body..].find(&format!("</{name}>"))?;
            return Some(&html[body..end]);
        }
        at = start + open.len();
    }
    None
}

/// `html` without any `<name …>…</name>` element of the given names (and
/// without HTML comments). Same-name nesting is not tracked, which is fine
/// for the elements this is used on (script, style, nav, header, footer …).
pub fn without_elements(html: &str, names: &[&str]) -> String {
    let lower = html.to_ascii_lowercase();
    let mut cut: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<!--").map(|i| at + i) {
        let end = lower[start..].find("-->").map(|i| start + i + 3).unwrap_or(lower.len());
        cut.push((start, end));
        at = end;
    }
    for name in names {
        let (open, close) = (format!("<{name}"), format!("</{name}>"));
        let mut at = 0;
        while let Some(start) = lower[at..].find(&open).map(|i| at + i) {
            let next = lower[start + open.len()..].chars().next();
            if !matches!(next, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
                at = start + open.len();
                continue;
            }
            let end = lower[start..].find(&close).map(|i| start + i + close.len()).unwrap_or(lower.len());
            cut.push((start, end));
            at = end;
        }
    }
    cut.sort();
    let mut out = String::with_capacity(html.len());
    let mut copied = 0;
    for (start, end) in cut {
        if start < copied {
            copied = copied.max(end);
            continue;
        }
        out.push_str(&html[copied..start]);
        out.push(' ');
        copied = end;
    }
    out.push_str(&html[copied.min(html.len())..]);
    out
}

/// The visible text of a fragment: tags dropped, the common entities
/// decoded, whitespace collapsed.
pub fn text_of(html: &str) -> String {
    let mut raw = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                raw.push(' ');
            }
            '>' if in_tag => in_tag = false,
            c if !in_tag => raw.push(c),
            _ => {}
        }
    }
    decode_entities(&raw).split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "rsquo" => Some('’'),
            "lsquo" => Some('‘'),
            "rdquo" => Some('”'),
            "ldquo" => Some('“'),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
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

/// A string for inside a single-quoted JavaScript literal inside HTML.
pub fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// A JSON string literal that is also safe inside a `<script>` element.
pub fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `<html lang="nl-NL">` -> `nl`.
pub fn page_language(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    lower
        .find("<html")
        .and_then(|at| {
            let tag = &lower[at..at + lower[at..].find('>')?];
            let v = tag.split("lang=").nth(1)?.trim_start_matches(['"', '\'']);
            Some(v.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>())
        })
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "en".into())
}
