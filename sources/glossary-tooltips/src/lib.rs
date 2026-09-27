//! Glossary Tooltips: a Stride plugin.
//!
//! The site owner lists terms and definitions in the settings. When a page is
//! rendered, the first occurrence of each term in the page's running text is
//! wrapped in `<dfn>` with a real `<button>` (so keyboard users can reach it)
//! that is described by a `role="tooltip"` element holding the definition.
//! CSS shows the tooltip on hover and focus; a few hundred bytes of script add
//! tap-to-toggle, Escape to dismiss (WCAG 1.4.13) and keep it on screen.
//! Headings, links, code, forms, navigation and anything marked
//! `data-glossary="off"` are left alone. No third-party requests.
//!
//! It asks for `storage` only, to keep its settings. Without it there are no
//! terms, so pages are served unchanged.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "data-stride-gt";
const DEFAULT_COLOR: &str = "#ca8a04";
const MAX_TERMS: usize = 200;
const MAX_TERM_LEN: usize = 80;
const MAX_DEFINITION_LEN: usize = 400;

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = match kv::get::<JsonValue>(SETTINGS_KEY) {
        Ok(Some(value)) => Settings::from_json(&value),
        Ok(None) => Settings::default(),
        // Refused storage is an answer, not a fault: there are no terms.
        Err(error) if error.is_permission_denied() => Settings::default(),
        Err(error) => {
            stride_pdk::log("info", &format!("settings unreadable, leaving the page alone: {error}"));
            Settings::default()
        }
    };
    let html = build(&page.html, &page.slug, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_glossary_tooltips(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let stored = kv::get::<JsonValue>(SETTINGS_KEY);
    let current = match &stored {
        Ok(Some(value)) => Settings::from_json(value),
        _ => Settings::default(),
    };
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: current.to_values(),
            message: match stored {
                Err(error) if error.is_permission_denied() => {
                    "This plugin was not granted storage, so it cannot keep your terms \
                     and pages are left unchanged. Grant storage in Plugins to use it."
                        .to_owned()
                }
                _ => String::new(),
            },
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let submitted = match Settings::from_values(&request.values) {
                Ok(settings) => settings,
                Err(error) => {
                    return Ok(Json(PanelResponse {
                        values: request_values_or(&request.values, &current),
                        message: String::new(),
                        error,
                    }));
                }
            };
            if let Err(error) = kv::set(SETTINGS_KEY, &submitted.to_json()) {
                return Ok(Json(PanelResponse {
                    values: submitted.to_values(),
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is \
                         nowhere to keep your terms. Nothing was saved. Grant storage in \
                         Plugins and save again."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                }));
            }
            let count = parse_terms(&submitted.terms).map(|t| t.len()).unwrap_or(0);
            let message = if !submitted.enabled {
                "Saved. Tooltips are off. Publish the site again to remove them from pages \
                 that are already live."
                    .to_owned()
            } else if count == 0 {
                "Saved. There are no terms yet, so pages stay as they are.".to_owned()
            } else {
                format!(
                    "Saved {count} term{}. Publish the site again to update pages that are \
                     already live.",
                    if count == 1 { "" } else { "s" }
                )
            };
            Ok(Json(PanelResponse {
                values: submitted.to_values(),
                message,
                error: String::new(),
            }))
        }
    }
}

/// On a refused submit, keep what was typed so nobody loses their list.
fn request_values_or(values: &Map<String, JsonValue>, current: &Settings) -> Map<String, JsonValue> {
    let mut out = current.to_values();
    for (key, value) in values {
        out.insert(key.clone(), value.clone());
    }
    out
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Dotted,
    Highlight,
}

impl Look {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "dotted" => Some(Look::Dotted),
            "highlight" => Some(Look::Highlight),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Look::Dotted => "dotted",
            Look::Highlight => "highlight",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// The list as typed, so the panel shows it back exactly.
    pub terms: String,
    pub match_case: bool,
    pub look: Look,
    /// `#rrggbb`, or empty for the default amber.
    pub color: String,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            terms: String::new(),
            match_case: false,
            look: Look::Dotted,
            color: String::new(),
            exclude: Vec::new(),
        }
    }
}

impl Settings {
    /// Whatever is stored, never fail: a field that is missing or the wrong
    /// shape falls back to its default.
    pub fn from_json(value: &JsonValue) -> Self {
        let d = Settings::default();
        let s = |key: &str| value.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| value.get(key).and_then(JsonValue::as_bool);
        Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            terms: s("terms").unwrap_or_default().to_owned(),
            match_case: b("matchCase").unwrap_or(d.match_case),
            look: s("look").and_then(Look::parse).unwrap_or(d.look),
            color: s("color").and_then(valid_color).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("terms".into(), self.terms.clone().into());
        map.insert("matchCase".into(), self.match_case.into());
        map.insert("look".into(), self.look.name().into());
        map.insert("color".into(), self.color.clone().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("terms".into(), self.terms.clone().into());
        values.insert("match-case".into(), self.match_case.into());
        values.insert("look".into(), self.look.name().into());
        if !self.color.is_empty() {
            values.insert("color".into(), self.color.clone().into());
        }
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| values.get(key).and_then(JsonValue::as_bool);
        let color = match s("color").map(str::trim) {
            None | Some("") => String::new(),
            Some(color) => valid_color(color).ok_or_else(|| {
                "The colour has to look like #ca8a04. Nothing was saved.".to_owned()
            })?,
        };
        let terms = s("terms").unwrap_or_default().replace("\r\n", "\n").trim().to_owned();
        parse_terms(&terms)?;
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            terms,
            match_case: b("match-case").unwrap_or(d.match_case),
            look: s("look").and_then(Look::parse).unwrap_or(d.look),
            color,
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn effective_color(&self) -> &str {
        if self.color.is_empty() { DEFAULT_COLOR } else { &self.color }
    }
}

/// One glossary entry: every spelling that should match, and the definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    pub spellings: Vec<String>,
    pub definition: String,
}

/// "API | APIs: A way for programs to talk" per line. Blank lines and lines
/// starting with `#` are skipped. A line without a colon, an empty side or an
/// over-long one is an error that names the line.
pub fn parse_terms(text: &str) -> Result<Vec<Term>, String> {
    let mut terms: Vec<Term> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let n = index + 1;
        let Some((names, definition)) = line.split_once(':') else {
            return Err(format!(
                "Line {n} has no colon. Write the term, a colon, then the definition, like \
                 \"SEO: Search engine optimisation\". Nothing was saved."
            ));
        };
        let definition = definition.trim();
        let spellings: Vec<String> = names
            .split('|')
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
            .collect();
        if spellings.is_empty() || definition.is_empty() {
            return Err(format!(
                "Line {n} needs both a term and a definition around the colon. Nothing was saved."
            ));
        }
        if let Some(long) = spellings.iter().find(|s| s.chars().count() > MAX_TERM_LEN) {
            return Err(format!(
                "Line {n}: \"{long}\" is longer than {MAX_TERM_LEN} characters. Use a shorter term. \
                 Nothing was saved."
            ));
        }
        if definition.chars().count() > MAX_DEFINITION_LEN {
            return Err(format!(
                "Line {n}: the definition is longer than {MAX_DEFINITION_LEN} characters. Keep \
                 tooltips short. Nothing was saved."
            ));
        }
        terms.push(Term { spellings, definition: definition.to_owned() });
        if terms.len() > MAX_TERMS {
            return Err(format!("There are more than {MAX_TERMS} terms. Nothing was saved."));
        }
    }
    Ok(terms)
}

/// `#rgb` or `#rrggbb`, normalised to lowercase `#rrggbb`. Anything else is
/// refused, which is what keeps the value safe to put into CSS.
pub fn valid_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        6 => Some(format!("#{hex}")),
        3 => Some(hex.chars().fold(String::from("#"), |mut out, c| {
            out.push(c);
            out.push(c);
            out
        })),
        _ => None,
    }
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

// ------------------------------------------------------------ the plain work

/// Elements whose text is never marked: headings, links, code, interactive
/// and form controls, page chrome, and anything already a definition.
const SKIP: &[&str] = &[
    "h1", "h2", "h3", "h4", "h5", "h6", "a", "code", "pre", "kbd", "samp", "var", "button",
    "label", "select", "option", "textarea", "summary", "dfn", "abbr", "svg", "math", "nav",
    "header", "footer", "figcaption", "iframe", "object", "video", "audio", "canvas", "template",
    "noscript", "script", "style", "title", "head", "form",
];
/// Elements whose content is not markup at all.
const RAW: &[&str] = &["script", "style", "textarea", "title", "template", "noscript", "xmp"];
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr",
];

/// A term ready for matching: each spelling HTML-escaped, the way it appears
/// in the page source.
struct Needle {
    spellings: Vec<Vec<char>>,
    definition: String,
    used: bool,
}

/// The page with tooltips added, or `None` to leave it alone: disabled,
/// excluded, no terms, already done, no body, or nothing matched.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || settings.exclude.iter().any(|s| s == slug) || html.contains(MARKER) {
        return None;
    }
    let terms = parse_terms(&settings.terms).ok()?;
    if terms.is_empty() {
        return None;
    }
    let bytes = html.as_bytes();
    let body = body_open_end(bytes)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);

    let mut needles: Vec<Needle> = terms
        .into_iter()
        .map(|t| Needle {
            spellings: t
                .spellings
                .iter()
                .map(|s| fold(&escape(s), settings.match_case))
                .collect(),
            definition: t.definition,
            used: false,
        })
        .collect();

    let (content, count) = mark(&html[body..body_close], &mut needles, settings.match_case);
    if count == 0 {
        return None;
    }
    let style = style(settings);
    let script = format!("<script {MARKER}>{SCRIPT}</script>");
    let mut out = String::with_capacity(html.len() + content.len() + style.len() + script.len());
    match head_close {
        Some(at) => {
            out.push_str(&html[..at]);
            out.push_str(&style);
            out.push_str(&html[at..body]);
        }
        None => {
            out.push_str(&html[..body]);
            out.push_str(&style);
        }
    }
    out.push_str(&content);
    out.push_str(&script);
    out.push_str(&html[body_close..]);
    Some(out)
}

fn fold(text: &str, match_case: bool) -> Vec<char> {
    if match_case {
        text.chars().collect()
    } else {
        // Per-char simple folding keeps positions one to one with the source.
        text.chars().map(lower).collect()
    }
}

fn lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// Walk the body: copy tags through, track whether we are inside something
/// to skip, and mark terms in the text between tags.
fn mark(body: &str, needles: &mut [Needle], match_case: bool) -> (String, usize) {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 1024);
    let mut skip: Option<(String, usize)> = None;
    let mut count = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = bytes[i..].iter().position(|&b| b == b'<').map_or(bytes.len(), |p| i + p);
            let text = &body[i..end];
            if skip.is_none() && needles.iter().any(|n| !n.used) {
                out.push_str(&mark_text(text, needles, match_case, &mut count));
            } else {
                out.push_str(text);
            }
            i = end;
            continue;
        }
        if body[i..].starts_with("<!--") {
            let end = body[i + 4..].find("-->").map_or(bytes.len(), |p| i + 4 + p + 3);
            out.push_str(&body[i..end]);
            i = end;
            continue;
        }
        let Some(end) = tag_end(bytes, i) else {
            out.push_str(&body[i..]);
            break;
        };
        let tag = &body[i..end];
        out.push_str(tag);
        i = end;
        let (name, closing) = tag_name(tag);
        if name.is_empty() {
            continue;
        }
        let self_closing = tag.ends_with("/>") || VOID.contains(&name.as_str());
        if RAW.contains(&name.as_str()) && !closing {
            // Copy raw text through to its closing tag untouched.
            let close = format!("</{name}");
            let stop = find_ci(&bytes[i..], close.as_bytes()).map_or(bytes.len(), |p| i + p);
            out.push_str(&body[i..stop]);
            i = stop;
            continue;
        }
        match &mut skip {
            Some((open, depth)) if *open == name => {
                if closing {
                    *depth -= 1;
                    if *depth == 0 {
                        skip = None;
                    }
                } else if !self_closing {
                    *depth += 1;
                }
            }
            Some(_) => {}
            None => {
                if !closing
                    && !self_closing
                    && (SKIP.contains(&name.as_str()) || opted_out(tag))
                {
                    skip = Some((name, 1));
                }
            }
        }
    }
    (out, count)
}

fn opted_out(tag: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    lower.contains("data-glossary=\"off\"") || lower.contains("data-glossary='off'")
}

/// Lowercase tag name and whether it is a closing tag. Empty for `<!doctype>`,
/// `<?…>` and anything that is not a tag.
fn tag_name(tag: &str) -> (String, bool) {
    let rest = &tag[1..];
    let (rest, closing) = match rest.strip_prefix('/') {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if name.is_empty() || !name.as_bytes()[0].is_ascii_alphabetic() {
        return (String::new(), closing);
    }
    (name, closing)
}

/// Mark the first unused term occurrences in one run of text.
fn mark_text(text: &str, needles: &mut [Needle], match_case: bool, count: &mut usize) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let folded: Vec<char> = chars
        .iter()
        .map(|&(_, c)| if match_case { c } else { lower(c) })
        .collect();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0usize; // byte offset in `text` copied so far
    let mut k = 0usize;
    while k < folded.len() {
        // A term starts only at a word boundary.
        if k > 0 && is_word(folded[k - 1]) {
            k += 1;
            continue;
        }
        let mut best: Option<(usize, usize)> = None; // (needle, length in chars)
        for (n, needle) in needles.iter().enumerate() {
            if needle.used {
                continue;
            }
            for spelling in &needle.spellings {
                let len = spelling.len();
                if len == 0 || k + len > folded.len() || folded[k..k + len] != spelling[..] {
                    continue;
                }
                if k + len < folded.len() && is_word(folded[k + len]) {
                    continue;
                }
                if best.is_none_or(|(_, l)| len > l) {
                    best = Some((n, len));
                }
            }
        }
        let Some((n, len)) = best else {
            k += 1;
            continue;
        };
        let start = chars[k].0;
        let end = chars.get(k + len).map_or(text.len(), |&(b, _)| b);
        *count += 1;
        needles[n].used = true;
        out.push_str(&text[copied..start]);
        let id = format!("stride-gt-{count}");
        out.push_str(&format!(
            "<dfn class=\"stride-gt\"><button type=\"button\" class=\"stride-gt-t\" \
aria-describedby=\"{id}\">{}</button><span class=\"stride-gt-tip\" role=\"tooltip\" id=\"{id}\">{}</span></dfn>",
            &text[start..end],
            escape(&needles[n].definition)
        ));
        copied = end;
        k += len;
    }
    out.push_str(&text[copied..]);
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Tap to toggle, Escape to dismiss, and keep the tooltip inside the
/// viewport. Delegated, so it costs nothing per term.
const SCRIPT: &str = "(function(){var d=document,o=null,C='stride-gt';\
function g(e){return e.target.closest&&e.target.closest('.'+C)}\
function p(x){var t=x.querySelector('.'+C+'-tip');if(!t)return;x.classList.remove(C+'-below');\
t.style.setProperty('--sgt-x','0px');var r=t.getBoundingClientRect();\
if(r.top<8){x.classList.add(C+'-below');r=t.getBoundingClientRect()}\
var w=d.documentElement.clientWidth,m=r.left<8?8-r.left:r.right>w-8?w-8-r.right:0;\
t.style.setProperty('--sgt-x',m+'px')}\
function h(){if(o){o.classList.remove(C+'-on');o=null}}\
d.addEventListener('pointerover',function(e){var x=g(e);if(x)p(x)});\
d.addEventListener('focusin',function(e){var x=g(e);if(x)p(x)});\
d.addEventListener('focusout',function(e){var x=g(e);if(x)x.classList.remove(C+'-x')});\
d.addEventListener('pointerout',function(e){var x=g(e);if(x&&!x.contains(e.relatedTarget))x.classList.remove(C+'-x')});\
d.addEventListener('click',function(e){var x=g(e);if(x&&e.target.closest('.'+C+'-t')){\
var n=o!==x;h();if(n){o=x;x.classList.remove(C+'-x');x.classList.add(C+'-on');p(x)}}else if(!x)h()});\
d.addEventListener('keydown',function(e){if(e.key==='Escape'){h();\
d.querySelectorAll('.'+C+':hover,.'+C+':focus-within').forEach(function(x){x.classList.add(C+'-x')})}})})();";

fn style(settings: &Settings) -> String {
    let color = settings.effective_color();
    let mark = match settings.look {
        Look::Dotted => format!(
            "text-decoration:underline dotted {color};text-decoration-thickness:2px;\
text-underline-offset:.22em"
        ),
        // The highlight is a tint of the colour, so body text keeps its contrast.
        Look::Highlight => format!(
            "background:{color}29;box-shadow:inset 0 -2px 0 {color};border-radius:3px;\
padding:0 .12em;margin:0 -.12em"
        ),
    };
    format!(
        "<style {MARKER}>.stride-gt{{position:relative;font-style:inherit}}\
.stride-gt-t{{font:inherit;color:inherit;letter-spacing:inherit;background:none;border:0;padding:0;\
margin:0;cursor:help;-webkit-appearance:none;appearance:none;{mark}}}\
.stride-gt-t:focus-visible{{outline:2px solid currentColor;outline-offset:2px;border-radius:2px}}\
.stride-gt-tip{{position:absolute;left:50%;bottom:calc(100% + 10px);z-index:2147483000;\
transform:translateX(calc(-50% + var(--sgt-x,0px)));width:max-content;\
max-width:min(20rem,calc(100vw - 16px));box-sizing:border-box;padding:.6em .85em;border-radius:10px;\
background:#111827;color:#f9fafb;border-top:3px solid {color};\
font:400 .875rem/1.5 system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;text-align:left;\
text-transform:none;letter-spacing:normal;white-space:normal;\
box-shadow:0 10px 30px rgba(17,24,39,.22),0 2px 6px rgba(17,24,39,.14);\
opacity:0;visibility:hidden;transition:opacity .15s,visibility .15s}}\
.stride-gt-tip::before{{content:'';position:absolute;left:0;right:0;top:100%;height:12px}}\
.stride-gt-tip::after{{content:'';position:absolute;top:100%;left:calc(50% - var(--sgt-x,0px) - 6px);\
border:6px solid transparent;border-top-color:#111827}}\
.stride-gt-below .stride-gt-tip{{bottom:auto;top:calc(100% + 10px);border-top:0;\
border-bottom:3px solid {color}}}\
.stride-gt-below .stride-gt-tip::before{{top:auto;bottom:100%}}\
.stride-gt-below .stride-gt-tip::after{{top:auto;bottom:100%;border-top-color:transparent;\
border-bottom-color:#111827}}\
.stride-gt:hover .stride-gt-tip,.stride-gt:focus-within .stride-gt-tip,.stride-gt-on .stride-gt-tip\
{{opacity:1;visibility:visible}}\
.stride-gt.stride-gt-x .stride-gt-tip{{opacity:0;visibility:hidden}}\
@media (prefers-reduced-motion:reduce){{.stride-gt-tip{{transition:none}}}}\
@media print{{.stride-gt-tip{{display:none}}.stride-gt-t{{text-decoration:none;background:none;\
box-shadow:none}}}}</style>"
    )
}

/// The index just past the `>` of a tag starting at `from`, honouring quotes.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (i, &c) in bytes[from + 1..].iter().enumerate() {
        match c {
            b'"' | b'\'' if quote == 0 => quote = c,
            c if c == quote => quote = 0,
            b'>' if quote == 0 => return Some(from + 1 + i + 1),
            _ => {}
        }
    }
    None
}

/// The index just past the `>` of the opening `<body…>` tag.
fn body_open_end(bytes: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<body") {
        let at = from + rel;
        match bytes.get(at + 5) {
            Some(b'>') => return Some(at + 6),
            Some(c) if c.is_ascii_whitespace() || *c == b'/' => return tag_end(bytes, at),
            _ => from = at + 5,
        }
    }
    None
}

/// `needle` must be ASCII, which keeps the answer on a char boundary.
fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

fn rfind_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&at| haystack[at..at + needle.len()].eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> String {
        format!("<!doctype html><html><head><title>API docs</title></head><body class=\"x\">{body}</body></html>")
    }

    fn with(terms: &str) -> Settings {
        Settings { terms: terms.into(), ..Settings::default() }
    }

    #[test]
    fn marks_first_occurrence_only() {
        let html = build(
            &page("<p>An API is great. Another API too.</p>"),
            "docs",
            &with("API: Application programming interface"),
        )
        .unwrap();
        assert_eq!(html.matches("<dfn class=\"stride-gt\">").count(), 1);
        assert!(html.contains("aria-describedby=\"stride-gt-1\">API</button>"));
        assert!(html.contains("role=\"tooltip\" id=\"stride-gt-1\">Application programming interface</span>"));
        assert!(html.contains("Another API too."));
        assert!(html.contains("</style></head>"));
        assert!(html.contains("</script></body></html>"));
        assert!(html.contains("<title>API docs</title>"));
    }

    #[test]
    fn skips_headings_links_code_and_opt_outs() {
        let body = "<h2>API</h2><p><a href=\"/api\">API</a> <code>API</code></p>\
<div data-glossary=\"off\"><div>API</div></div><pre><b>API</b></pre><p>The <em>API</em>.</p>";
        let html = build(&page(body), "docs", &with("API: Interface")).unwrap();
        assert_eq!(html.matches("<dfn").count(), 1);
        assert!(html.contains("<h2>API</h2>"));
        assert!(html.contains("<code>API</code>"));
        assert!(html.contains("<em><dfn"));
    }

    #[test]
    fn whole_words_case_and_longest_first() {
        let html = build(
            &page("<p>Rapid apis. Static site generator and site.</p>"),
            "x",
            &with("API | APIs: Interface\nsite: A website\nstatic site generator: Builds HTML"),
        )
        .unwrap();
        assert!(html.contains(">apis</button>"));
        assert!(html.contains(">Static site generator</button>"));
        assert!(html.contains("and <dfn class=\"stride-gt\"><button type=\"button\" class=\"stride-gt-t\" aria-describedby=\"stride-gt-3\">site</button>"));
        assert!(html.contains("Rapid "));
        let exact = Settings { match_case: true, ..with("API: Interface") };
        assert!(build(&page("<p>api only</p>"), "x", &exact).is_none());
    }

    #[test]
    fn escapes_definitions_and_matches_entities() {
        let html = build(
            &page("<p>Use R&amp;D wisely.</p>"),
            "x",
            &with("R&D: Research <b>\"and\"</b> development"),
        )
        .unwrap();
        assert!(html.contains(">R&amp;D</button>"));
        assert!(html.contains("Research &lt;b&gt;&quot;and&quot;&lt;/b&gt; development"));
    }

    #[test]
    fn leaves_page_alone_when_it_should() {
        let p = page("<p>API</p>");
        assert!(build(&p, "x", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..with("API: x") };
        assert!(build(&p, "x", &off).is_none());
        let skip = Settings { exclude: vec!["x".into()], ..with("API: x") };
        assert!(build(&p, "x", &skip).is_none());
        let once = build(&p, "x", &with("API: x")).unwrap();
        assert!(build(&once, "x", &with("API: x")).is_none());
        assert!(build("<p>API</p>", "x", &with("API: x")).is_none());
        assert!(build(&page("<script>var API=1</script><p>none</p>"), "x", &with("API: x")).is_none());
    }

    #[test]
    fn parses_and_rejects_terms() {
        let terms = parse_terms("# comment\n\nSEO: Search engine optimisation: finding\nA | B : c").unwrap();
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0].definition, "Search engine optimisation: finding");
        assert_eq!(terms[1].spellings, vec!["A", "B"]);
        assert!(parse_terms("no colon").unwrap_err().contains("Line 1"));
        assert!(parse_terms("ok: x\n: y").unwrap_err().contains("Line 2"));
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            terms: "API: Interface".into(),
            match_case: true,
            look: Look::Highlight,
            color: "#0ea5e9".into(),
            exclude: vec!["home".into(), "contact".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        let mut bad = Map::new();
        bad.insert("color".into(), "url(x)".into());
        assert!(Settings::from_values(&bad).is_err());
        let mut bad = Map::new();
        bad.insert("terms".into(), "oops".into());
        assert!(Settings::from_values(&bad).is_err());
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
        assert_eq!(valid_color("#ABC"), Some("#aabbcc".into()));
    }
}
