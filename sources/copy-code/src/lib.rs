//! Copy Code: a Stride plugin.
//!
//! It gives every `<pre>` code block on a page a small toolbar with a copy
//! button, "Copied" feedback, an optional language label and optional line
//! numbers, and (unless the site owner keeps their theme's look) a clean dark
//! or light code block style. No highlighting library, no third-party
//! requests: a little inline CSS in `<head>` and a little inline JavaScript
//! before `</body>`, and only on pages that have a `<pre>` at all.
//!
//! The toolbar is built in the browser, because only the browser can reach
//! the clipboard: without JavaScript a visitor sees the plain code block,
//! which is exactly what they would have seen without the plugin.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "data-stride-cc";
const DEFAULT_COPY: &str = "Copy";
const DEFAULT_COPIED: &str = "Copied";
const LABEL_MAX: usize = 24;

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = match kv::get::<JsonValue>(SETTINGS_KEY) {
        Ok(Some(value)) => Settings::from_json(&value),
        Ok(None) => Settings::default(),
        // Refused storage is an answer, not a fault: run with the defaults.
        Err(error) if error.is_permission_denied() => Settings::default(),
        Err(error) => {
            stride_pdk::log("info", &format!("settings unreadable, using defaults: {error}"));
            Settings::default()
        }
    };
    let html = build(&page.html, &page.slug, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_copy_code(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                    "This plugin was not granted storage, so these settings cannot be saved. \
                     It still adds copy buttons with the defaults shown here."
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
                        values: current.to_values(),
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
                         nowhere to keep these settings. Nothing was saved; the defaults \
                         stay in effect."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                }));
            }
            let message = if submitted.enabled {
                "Saved. Publish the site again to update pages that are already live."
            } else {
                "Saved. Copy buttons are off. Publish the site again to remove them from \
                 pages that are already live."
            };
            Ok(Json(PanelResponse {
                values: submitted.to_values(),
                message: message.to_owned(),
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------- the settings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Dark,
    Light,
    Theme,
}

impl Look {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "dark" => Some(Look::Dark),
            "light" => Some(Look::Light),
            "theme" => Some(Look::Theme),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Look::Dark => "dark",
            Look::Light => "light",
            Look::Theme => "theme",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub look: Look,
    pub language: bool,
    pub line_numbers: bool,
    /// Empty for the default "Copy".
    pub copy_label: String,
    /// Empty for the default "Copied".
    pub copied_label: String,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            look: Look::Dark,
            language: true,
            line_numbers: false,
            copy_label: String::new(),
            copied_label: String::new(),
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
            look: s("look").and_then(Look::parse).unwrap_or(d.look),
            language: b("language").unwrap_or(d.language),
            line_numbers: b("lineNumbers").unwrap_or(d.line_numbers),
            copy_label: s("copyLabel").map(clean_label).unwrap_or_default(),
            copied_label: s("copiedLabel").map(clean_label).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("look".into(), self.look.name().into());
        map.insert("language".into(), self.language.into());
        map.insert("lineNumbers".into(), self.line_numbers.into());
        map.insert("copyLabel".into(), self.copy_label.clone().into());
        map.insert("copiedLabel".into(), self.copied_label.clone().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("look".into(), self.look.name().into());
        values.insert("language".into(), self.language.into());
        values.insert("line-numbers".into(), self.line_numbers.into());
        values.insert("copy-label".into(), self.copy_label.clone().into());
        values.insert("copied-label".into(), self.copied_label.clone().into());
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let b = |key: &str| values.get(key).and_then(JsonValue::as_bool);
        for (key, what) in [("copy-label", "button text"), ("copied-label", "text after copying")] {
            if s(key).is_some_and(|v| v.trim().chars().count() > LABEL_MAX) {
                return Err(format!(
                    "The {what} can be at most {LABEL_MAX} characters. Nothing was saved."
                ));
            }
        }
        Ok(Settings {
            enabled: b("enabled").unwrap_or(d.enabled),
            look: s("look").and_then(Look::parse).unwrap_or(d.look),
            language: b("language").unwrap_or(d.language),
            line_numbers: b("line-numbers").unwrap_or(d.line_numbers),
            copy_label: s("copy-label").map(clean_label).unwrap_or_default(),
            copied_label: s("copied-label").map(clean_label).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    fn copy_text(&self) -> &str {
        if self.copy_label.is_empty() { DEFAULT_COPY } else { &self.copy_label }
    }

    fn copied_text(&self) -> &str {
        if self.copied_label.is_empty() { DEFAULT_COPIED } else { &self.copied_label }
    }
}

/// Trimmed, control characters dropped, at most `LABEL_MAX` characters.
fn clean_label(text: &str) -> String {
    text.trim().chars().filter(|c| !c.is_control()).take(LABEL_MAX).collect()
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
}

/// Safe inside a double- or single-quoted HTML attribute.
fn escape_attr(text: &str) -> String {
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

/// The page with the style and script added, or `None` to leave it alone:
/// disabled, excluded, already done, no code block, or not a whole document.
pub fn build(html: &str, slug: &str, settings: &Settings) -> Option<String> {
    if !settings.enabled || settings.exclude.iter().any(|s| s == slug) {
        return None;
    }
    if html.contains(MARKER) {
        return None;
    }
    let bytes = html.as_bytes();
    let body = body_open_end(bytes)?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at >= body)?;
    // Stride has no code block of its own: an author writes a fenced block
    // into a text block, and it becomes a real `<pre><code>` here.
    let converted = convert_fences(&html[body..body_close]);
    let html = &format!("{}{}{}", &html[..body], converted, &html[body_close..]);
    let bytes = html.as_bytes();
    let body_close = body + converted.len();
    if !has_pre(&bytes[body..body_close]) {
        return None;
    }
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);

    let style = style(settings.look);
    let mut script = format!(
        "<script data-stride-cc data-copy=\"{}\" data-copied=\"{}\"",
        escape_attr(settings.copy_text()),
        escape_attr(settings.copied_text()),
    );
    if settings.language {
        script.push_str(" data-lang=\"1\"");
    }
    if settings.line_numbers {
        script.push_str(" data-lines=\"1\"");
    }
    script.push('>');
    script.push_str(SCRIPT);
    script.push_str("</script>");

    let mut out = String::with_capacity(html.len() + style.len() + script.len());
    match head_close {
        Some(at) => {
            out.push_str(&html[..at]);
            out.push_str(&style);
            out.push_str(&html[at..body_close]);
        }
        None => {
            out.push_str(&html[..body]);
            out.push_str(&style);
            out.push_str(&html[body..body_close]);
        }
    }
    out.push_str(&script);
    out.push_str(&html[body_close..]);
    Some(out)
}

/// Builds the toolbar around every `<pre>` and does the copying. The
/// clipboard API needs a secure context; elsewhere it falls back to a hidden
/// textarea, and if that fails too it selects the code and says which keys
/// to press, so the visitor is never left with a button that did nothing.
const SCRIPT: &str = "(function(){var d=document,s=d.currentScript,o=s.dataset,C=o.copy,K=o.copied,\
V='<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" aria-hidden=\"true\" focusable=\"false\" class=\"stride-cc-i',\
I=V+'1\"><rect x=\"9\" y=\"9\" width=\"11\" height=\"11\" rx=\"2\"/><path d=\"M5 15V6a2 2 0 0 1 2-2h9\"/></svg>'\
+V+'2\"><path d=\"M5 12.5l4.5 4.5L19 7.5\"/></svg>';\
function say(b,l,t,k){b.classList.toggle('stride-cc-done',k);l.textContent=t;clearTimeout(b.t);\
b.t=setTimeout(function(){b.classList.remove('stride-cc-done');l.textContent=C},2000)}\
function old(t){var a=d.createElement('textarea'),k=false;a.value=t;a.setAttribute('readonly','');\
a.style.cssText='position:fixed;top:0;left:0;opacity:0';d.body.appendChild(a);a.select();\
try{k=d.execCommand('copy')}catch(e){}a.remove();if(!k)throw 0}\
function copy(c,b,l){var t=c.textContent.replace(/\\n$/,''),n=navigator.clipboard;\
(n&&window.isSecureContext?n.writeText(t):Promise.reject()).catch(function(){old(t)}).then(function(){say(b,l,K,true)},\
function(){var r=d.createRange(),g=getSelection();r.selectNodeContents(c);g.removeAllRanges();g.addRange(r);\
say(b,l,/Mac|iP/.test(navigator.platform)?'Press \\u2318C':'Press Ctrl+C',false)})}\
d.querySelectorAll('pre').forEach(function(p){if(p.parentNode.classList.contains('stride-cc'))return;\
var c=p.querySelector('code')||p,m=/(?:^|\\s)(?:language|lang)-([\\w#+.-]+)/.exec(c.className+' '+p.className),\
g=((m&&m[1])||p.getAttribute('data-lang')||c.getAttribute('data-lang')||'').slice(0,20),\
w=d.createElement('div'),h=d.createElement('div'),b=d.createElement('button'),l=d.createElement('span');\
w.className='stride-cc';h.className='stride-cc-bar';\
if(o.lang&&g&&!/^(text|plain|plaintext|none)$/i.test(g)){var e=d.createElement('span');e.className='stride-cc-lang';\
e.textContent=g;h.appendChild(e)}\
b.type='button';b.className='stride-cc-btn';b.innerHTML=I;l.textContent=C;l.setAttribute('aria-live','polite');\
b.appendChild(l);b.addEventListener('click',function(){copy(c,b,l)});h.appendChild(b);\
p.parentNode.insertBefore(w,p);w.appendChild(h);w.appendChild(p);\
if(o.lines&&c!==p){var n=c.textContent.replace(/\\n$/,'').split('\\n').length,x=d.createElement('span'),a=[];\
for(var i=1;i<=n;i++)a.push(i);x.className='stride-cc-ln';x.setAttribute('aria-hidden','true');\
x.textContent=a.join('\\n');p.insertBefore(x,p.firstChild);p.classList.add('stride-cc-num')}})})();";

const MONO: &str = "ui-monospace,SFMono-Regular,Menlo,Consolas,'Liberation Mono',monospace";

fn style(look: Look) -> String {
    let mut css = format!(
        "<style>.stride-cc{{position:relative;margin:1.5em 0}}.stride-cc:has(>pre[data-stride-cc-fence]){{margin:0}}.stride-cc>pre{{margin:0}}\
.stride-cc-bar{{display:flex;align-items:center;justify-content:flex-end;gap:8px}}\
.stride-cc-lang{{margin-right:auto;font:600 12px/1 {MONO};letter-spacing:.02em;opacity:.7}}\
.stride-cc-btn{{display:inline-flex;align-items:center;gap:6px;min-height:30px;margin:0;padding:0 10px;\
border:1px solid rgba(127,127,127,.35);border-radius:6px;background:rgba(127,127,127,.12);color:inherit;\
font:500 13px/1 system-ui,-apple-system,'Segoe UI',sans-serif;cursor:pointer;transition:background .15s}}\
.stride-cc-btn:hover{{background:rgba(127,127,127,.24)}}\
.stride-cc-btn:focus-visible{{outline:2px solid #38bdf8;outline-offset:2px}}\
.stride-cc-btn svg{{flex:none;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}}\
.stride-cc-i2,.stride-cc-done .stride-cc-i1{{display:none}}.stride-cc-done .stride-cc-i2{{display:block}}\
.stride-cc-done{{color:#16a34a}}\
pre[data-stride-cc-fence]{{font-family:{MONO};font-size:.9em;white-space:pre;overflow:auto}}\
pre.stride-cc-num{{display:flex;gap:1em}}pre.stride-cc-num>code{{flex:1;min-width:0}}\
.stride-cc-ln{{flex:none;min-width:2ch;padding-right:.9em;border-right:1px solid rgba(127,127,127,.3);\
text-align:right;opacity:.5;user-select:none;-webkit-user-select:none}}\
@media print{{.stride-cc-bar{{display:none}}}}"
    );
    match look {
        Look::Theme => css.push_str(
            ".stride-cc-bar{position:absolute;top:8px;right:8px;z-index:1}\
.stride-cc-lang{margin-right:0}",
        ),
        Look::Dark | Look::Light => {
            let (bg, fg, bar, border, done) = if look == Look::Dark {
                ("#0f172a", "#e2e8f0", "#1e293b", "#334155", "#4ade80")
            } else {
                ("#f8fafc", "#0f172a", "#f1f5f9", "#e2e8f0", "#15803d")
            };
            css.push_str(&format!(
                ".stride-cc{{background:{bg};color:{fg};border:1px solid {border};border-radius:12px;overflow:hidden;\
box-shadow:0 1px 2px rgba(15,23,42,.06)}}\
.stride-cc-bar{{min-height:44px;padding:6px 8px 6px 16px;background:{bar};border-bottom:1px solid {border}}}\
.stride-cc>pre{{padding:16px 18px;overflow:auto;background:none;color:inherit;border:0;border-radius:0;\
box-shadow:none;font:14px/1.65 {MONO};tab-size:2;-moz-tab-size:2}}\
.stride-cc>pre code{{padding:0;background:none;color:inherit;border:0;font:inherit;white-space:pre}}\
.stride-cc .stride-cc-done{{color:{done}}}"
            ));
        }
    }
    css.push_str("</style>");
    css
}

/// Every `<p …>` whose whole content is a fenced block, ```` ```lang ````
/// on the first line and ```` ``` ```` on the last, as `<pre><code>`. The
/// paragraph's attributes move to the `<pre>`, so its layout classes still
/// apply. Its text is reused as it is: the renderer escaped it already, and
/// a paragraph with any tag inside is not plain text and is left alone.
pub fn convert_fences(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut done = 0;
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<p") {
        let at = from + rel;
        from = at + 2;
        match bytes.get(at + 2) {
            Some(c) if *c == b'>' || c.is_ascii_whitespace() => {}
            _ => continue,
        }
        let Some(open_end) = tag_end(bytes, at + 2) else { break };
        let Some(close) = find_ci(&bytes[open_end..], b"</p>").map(|r| open_end + r) else {
            break;
        };
        let inner = &body[open_end..close];
        let Some((lang, code)) = fenced(inner) else { continue };
        let attrs = &body[at + 2..open_end - 1];
        out.push_str(&body[done..at]);
        out.push_str("<pre");
        out.push_str(attrs);
        out.push_str(" data-stride-cc-fence><code");
        if !lang.is_empty() {
            out.push_str(" class=\"language-");
            out.push_str(lang);
            out.push('"');
        }
        out.push('>');
        out.push_str(code);
        out.push_str("</code></pre>");
        done = close + 4;
        from = done;
    }
    out.push_str(&body[done..]);
    out
}

/// The language and the code of a fenced block, or `None` if `text` is not
/// exactly one. The language is the first word after the opening fence, kept
/// only when it is made of letters, digits and `#+.-` (so it is safe in an
/// attribute), at most 20 characters.
fn fenced(text: &str) -> Option<(&str, &str)> {
    if text.contains('<') {
        return None;
    }
    let text = text.trim();
    let rest = text.strip_prefix("```")?;
    let (first, rest) = rest.split_once('\n')?;
    let code = rest.trim_end().strip_suffix("```")?;
    if code.contains("```") && code.lines().any(|l| l.trim() == "```") {
        return None;
    }
    let code = code.strip_suffix('\n').unwrap_or(code);
    let code = code.strip_suffix('\r').unwrap_or(code);
    let lang = first.split_whitespace().next().unwrap_or("");
    let lang = if lang.len() <= 20
        && lang.bytes().all(|b| b.is_ascii_alphanumeric() || b"#+.-_".contains(&b))
    {
        lang
    } else {
        ""
    };
    Some((lang, code))
}

/// The index just past the `>` that ends the tag whose attributes start at
/// `from`, skipping quoted values.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (i, &c) in bytes[from..].iter().enumerate() {
        match c {
            b'"' | b'\'' if quote == 0 => quote = c,
            c if c == quote => quote = 0,
            b'>' if quote == 0 => return Some(from + i + 1),
            _ => {}
        }
    }
    None
}

/// Whether there is a `<pre>` or `<pre …>` element start in `bytes`.
fn has_pre(bytes: &[u8]) -> bool {
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<pre") {
        let at = from + rel;
        match bytes.get(at + 4) {
            Some(c) if *c == b'>' || c.is_ascii_whitespace() => return true,
            _ => from = at + 4,
        }
    }
    false
}

/// The index just past the `>` of the opening `<body…>` tag.
fn body_open_end(bytes: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<body") {
        let at = from + rel;
        match bytes.get(at + 5) {
            Some(b'>') => return Some(at + 6),
            Some(c) if c.is_ascii_whitespace() || *c == b'/' => {
                let mut quote = 0u8;
                for (i, &c) in bytes[at + 5..].iter().enumerate() {
                    match c {
                        b'"' | b'\'' if quote == 0 => quote = c,
                        c if c == quote => quote = 0,
                        b'>' if quote == 0 => return Some(at + 5 + i + 1),
                        _ => {}
                    }
                }
                return None;
            }
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

    const PAGE: &str = "<!doctype html><html><head><title>T</title></head>\
<body class=\"x\" data-a='>'><main><pre><code class=\"language-bash\">ls</code></pre></main></body></html>";

    #[test]
    fn adds_style_and_script() {
        let html = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(html.contains("</style></head>"));
        assert!(html.contains("data-copy=\"Copy\" data-copied=\"Copied\" data-lang=\"1\">"));
        assert!(!html.contains("data-lines"));
        assert!(html.contains("</script></body></html>"));
        assert!(html.contains("background:#0f172a"));
    }

    #[test]
    fn leaves_pages_without_code_alone() {
        let page = "<html><head></head><body><p>Hi</p><preview></preview></body></html>";
        assert!(build(page, "x", &Settings::default()).is_none());
        let upper = "<html><head></head><body><PRE class=a>x</PRE></body></html>";
        assert!(build(upper, "x", &Settings::default()).is_some());
    }

    #[test]
    fn is_idempotent_and_respects_settings() {
        let once = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(build(&once, "guide", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "guide", &off).is_none());
        let skip = Settings { exclude: vec!["guide".into()], ..Settings::default() };
        assert!(build(PAGE, "guide", &skip).is_none());
        let theme = Settings { look: Look::Theme, language: false, line_numbers: true, ..Settings::default() };
        let html = build(PAGE, "guide", &theme).unwrap();
        assert!(html.contains("data-copied=\"Copied\" data-lines=\"1\">"));
        assert!(!html.contains("#0f172a"));
    }

    #[test]
    fn escapes_labels() {
        let s = Settings {
            copy_label: "<b>\"x'&".into(),
            ..Settings::default()
        };
        let html = build(PAGE, "guide", &s).unwrap();
        assert!(html.contains("data-copy=\"&lt;b&gt;&quot;x&#39;&amp;\""));
    }

    #[test]
    fn converts_fenced_paragraphs() {
        let body = "<p class=\"_s6\">```bash\nnpm i &amp;&amp; echo &quot;&lt;x&gt;&quot;\r\n```</p>\
<p>```\nplain\n```</p><p>```js only one line```</p><p>```\nunterminated</p>\
<p>```\n<b>x</b>\n```</p><p>```a b c\nx\n```</p><p>```<x>\ny\n```</p><pre>z</pre>";
        let out = convert_fences(body);
        assert!(out.starts_with("<pre class=\"_s6\" data-stride-cc-fence><code class=\"language-bash\">\
npm i &amp;&amp; echo &quot;&lt;x&gt;&quot;</code></pre>"));
        assert!(out.contains("<pre data-stride-cc-fence><code>plain</code></pre>"));
        assert!(out.contains("<p>```js only one line```</p>"));
        assert!(out.contains("<p>```\nunterminated</p>"));
        assert!(out.contains("<p>```\n<b>x</b>\n```</p>"));
        assert!(out.contains("<code class=\"language-a\">x</code>"));
        assert!(out.ends_with("<pre>z</pre>"));
        assert_eq!(convert_fences("<p>hi</p><para>"), "<p>hi</p><para>");
        let page = "<html><head></head><body><p>```sh\nls\n```</p></body></html>";
        let html = build(page, "x", &Settings::default()).unwrap();
        assert!(html.contains("<pre data-stride-cc-fence><code class=\"language-sh\">ls</code></pre><script"));
        assert!(build(&html, "x", &Settings::default()).is_none());
    }

    #[test]
    fn refuses_fragments() {
        assert!(build("<pre>no body</pre>", "x", &Settings::default()).is_none());
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            look: Look::Light,
            language: false,
            line_numbers: true,
            copy_label: "Kopiëren".into(),
            copied_label: "Gekopieerd".into(),
            exclude: vec!["home".into(), "contact".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        let mut bad = Map::new();
        bad.insert("copy-label".into(), "x".repeat(40).into());
        assert!(Settings::from_values(&bad).is_err());
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
        assert_eq!(clean_label("  a\u{7}b  "), "ab");
    }
}
