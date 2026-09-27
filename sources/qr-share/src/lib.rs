//! QR Share: a QR code of the page's own address, so a phone can open it.
//!
//! The code is generated here, in the plugin, as an inline SVG: no image
//! service, no library from a CDN, nothing a visitor's browser fetches from
//! anyone. By default it sits behind a small round button in a corner of the
//! page; it can also be a card at the bottom of every page. Either way it is
//! on printed pages too, which is what makes it useful on posters and menus.
//!
//! The address comes from, in order: the page's own absolute canonical link,
//! the site address in the settings, or the site's address learned when a
//! page is published with a domain. With none of these the page is left
//! alone: a QR code of a relative path would not open anything.
//!
//! Permissions: `storage`, for the panel's settings and the learned address.
//! Without it the plugin still works on pages with an absolute canonical link.

mod qr;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Published,
    kv,
};

pub use qr::QrCode;

const SETTINGS: &str = "settings";
const ORIGIN: &str = "origin";
const MARKER: &str = "id=\"qs-code\"";
const QUIET: usize = 4;

// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub placement: String,
    pub side: String,
    pub heading: String,
    pub accent: String,
    pub print: bool,
    pub download: bool,
    pub exclude: String,
    pub site_address: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            placement: "button".to_owned(),
            side: "right".to_owned(),
            heading: "Scan to open this page".to_owned(),
            accent: "#16A34A".to_owned(),
            print: true,
            download: true,
            exclude: String::new(),
            site_address: String::new(),
        }
    }
}

const PLACEMENTS: [&str; 2] = ["button", "footer"];
const SIDES: [&str; 2] = ["right", "left"];

impl Settings {
    /// Every read may fail: without `storage` the defaults apply.
    fn load() -> Self {
        match kv::get::<Map<String, JsonValue>>(SETTINGS) {
            Ok(Some(values)) => Settings::from_values(&values),
            _ => Settings::default(),
        }
    }

    /// Settings from panel values; anything missing or malformed falls back.
    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Settings::default();
        let boolean = |name: &str, fallback: bool| {
            values
                .get(name)
                .and_then(JsonValue::as_bool)
                .unwrap_or(fallback)
        };
        let text = |name: &str, fallback: &str| {
            values
                .get(name)
                .and_then(JsonValue::as_str)
                .map(|s| s.trim().to_owned())
                .unwrap_or_else(|| fallback.to_owned())
        };
        let choice = |name: &str, allowed: &[&str], fallback: &str| {
            let value = text(name, fallback);
            if allowed.contains(&value.as_str()) {
                value
            } else {
                fallback.to_owned()
            }
        };
        let accent = text("accent", &d.accent);
        Settings {
            enabled: boolean("enabled", d.enabled),
            placement: choice("placement", &PLACEMENTS, &d.placement),
            side: choice("side", &SIDES, &d.side),
            heading: text("heading", &d.heading),
            accent: if is_hex_color(&accent) {
                accent
            } else {
                d.accent
            },
            print: boolean("print", d.print),
            download: boolean("download", d.download),
            exclude: text("exclude", &d.exclude),
            site_address: text("site-address", &d.site_address),
        }
    }

    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut v = Map::new();
        v.insert("enabled".into(), self.enabled.into());
        v.insert("placement".into(), self.placement.clone().into());
        v.insert("side".into(), self.side.clone().into());
        v.insert("heading".into(), self.heading.clone().into());
        v.insert("accent".into(), self.accent.clone().into());
        v.insert("print".into(), self.print.into());
        v.insert("download".into(), self.download.into());
        v.insert("exclude".into(), self.exclude.clone().into());
        v.insert("site-address".into(), self.site_address.clone().into());
        v
    }
}

pub fn is_hex_color(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)
}

/// `https://example.com` from anything that starts like an http(s) URL.
pub fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url
        .strip_prefix("https://")
        .map(|r| ("https://", r))
        .or_else(|| url.strip_prefix("http://").map(|r| ("http://", r)))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b))
    {
        return None;
    }
    Some(format!("{scheme}{}", host.to_ascii_lowercase()))
}

/// An absolute http(s) URL that is safe to encode and to show, or `None`.
/// Only printable ASCII without quotes, angle brackets or spaces survives.
pub fn absolute_url(url: &str) -> Option<String> {
    let url = url.trim();
    origin_of(url)?;
    if url.len() > 1000
        || !url
            .bytes()
            .all(|b| (0x21..0x7F).contains(&b) && !b"\"'<>`\\".contains(&b))
    {
        return None;
    }
    Some(url.to_owned())
}

// ----------------------------------------------------------------- the hooks

#[plugin_fn]
pub fn on_publish(Json(event): Json<Published>) -> FnResult<Json<JsonValue>> {
    // Remember the site's origin when the host tells us an absolute address.
    // Refused storage only means a site address or canonical link is needed.
    if let Some(origin) = origin_of(&event.url) {
        let known = kv::get::<String>(ORIGIN).ok().flatten();
        if known.as_deref() != Some(origin.as_str()) {
            let _ = kv::set(ORIGIN, &origin);
        }
    }
    Ok(Json(JsonValue::Object(Map::new())))
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = Settings::load();
    let origin =
        origin_of(&settings.site_address).or_else(|| kv::get::<String>(ORIGIN).ok().flatten());
    let html = render(&page.html, &page.slug, &settings, origin.as_deref()).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

/// The page's address: its absolute canonical link, else origin plus slug.
pub fn page_url(html: &str, slug: &str, origin: Option<&str>) -> Option<String> {
    if let Some(canonical) = canonical(html).and_then(|c| absolute_url(&c)) {
        return Some(canonical);
    }
    let origin = origin?;
    let slug = slug.trim_matches('/');
    let url = if slug == "home" || slug.is_empty() {
        format!("{origin}/")
    } else {
        format!("{origin}/{slug}")
    };
    absolute_url(&url)
}

/// The `href` of `<link rel="canonical">` in the head, entity-decoded.
pub fn canonical(html: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let head_end = find_ci(bytes, b"</head>").unwrap_or(bytes.len());
    let mut from = 0;
    while let Some(at) = find_ci(&bytes[from..head_end], b"<link") {
        let start = from + at;
        let end = start + html[start..head_end].find('>')?;
        let tag = &html[start..end];
        if attr(tag, "rel").is_some_and(|rel| rel.eq_ignore_ascii_case("canonical")) {
            return attr(tag, "href").map(|h| h.replace("&amp;", "&"));
        }
        from = end;
    }
    None
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(at) = lower.find(&needle) {
            let value = &tag[at + needle.len()..];
            return value.find(quote).map(|end| &value[..end]);
        }
    }
    None
}

/// The page with the QR code in it, or `None` to leave it alone.
pub fn render(html: &str, slug: &str, settings: &Settings, origin: Option<&str>) -> Option<String> {
    if !settings.enabled || html.contains(MARKER) || !wanted(slug, &settings.exclude) {
        return None;
    }
    let body_end = rfind_ci(html.as_bytes(), b"</body>")?;
    let url = page_url(html, slug, origin)?;
    let code = QrCode::encode(url.as_bytes())?;
    let block = qr_block(settings, &url, &code, slug);
    let style = format!("<style>{}</style>", css(settings));
    let mut out = String::with_capacity(html.len() + block.len() + style.len());
    let at = if settings.placement == "footer" {
        inside_wrapper(html, body_end)
    } else {
        body_end
    };
    match find_ci(html.as_bytes(), b"</head>") {
        Some(head) if head < at => {
            out.push_str(&html[..head]);
            out.push_str(&style);
            out.push_str(&html[head..at]);
            out.push_str(&block);
        }
        _ => {
            out.push_str(&html[..at]);
            out.push_str(&style);
            out.push_str(&block);
        }
    }
    out.push_str(&html[at..]);
    Some(out)
}

/// Slugs separated by commas; a trailing `*` matches a prefix.
pub fn wanted(slug: &str, exclude: &str) -> bool {
    let slug = slug.trim_matches('/');
    !exclude
        .split([',', ' ', '\n'])
        .map(|s| s.trim().trim_matches('/'))
        .filter(|s| !s.is_empty())
        .any(|pattern| match pattern.strip_suffix('*') {
            Some(prefix) => slug.starts_with(prefix.trim_end_matches('/')),
            None => slug == pattern,
        })
}

/// The end of the page's outermost wrapper when it closes right before
/// `</body>`, so a footer card picks up the page's own width and colours.
fn inside_wrapper(html: &str, body_end: usize) -> usize {
    let before = html[..body_end].trim_end();
    ["</div>", "</main>"]
        .iter()
        .find(|close| {
            before.len() >= close.len()
                && before.as_bytes()[before.len() - close.len()..]
                    .eq_ignore_ascii_case(close.as_bytes())
        })
        .map_or(body_end, |close| before.len() - close.len())
}

// ------------------------------------------------------------------ the HTML

const QR_GLYPH: &str = r#"<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path fill="currentColor" d="M3 3h8v8H3zm2 2v4h4V5zm8-2h8v8h-8zm2 2v4h4V5zM3 13h8v8H3zm2 2v4h4v-4zm1-9h2v2H6zm10 0h2v2h-2zM6 16h2v2H6zm7-3h2v2h-2zm2 2h2v2h-2zm-2 2h2v4h-2zm4-4h4v2h-4zm2 4h2v4h-4v-2h2zm-4 2h2v2h-2z"/></svg>"#;

fn css(s: &Settings) -> String {
    let side = if s.side == "left" { "left" } else { "right" };
    format!(
        ".qs{{--qs-a:{accent};font:500 14px/1.4 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif;color:#111827}}\
.qs *{{box-sizing:border-box}}\
.qs-fab{{position:fixed;{side}:20px;bottom:20px;z-index:2147483000}}\
.qs-fab>summary{{list-style:none;display:grid;place-items:center;width:52px;height:52px;border-radius:50%;background:var(--qs-a);color:#fff;cursor:pointer;box-shadow:0 6px 20px rgba(0,0,0,.22);transition:transform .15s}}\
.qs-fab>summary::-webkit-details-marker{{display:none}}\
.qs-fab>summary:hover{{transform:translateY(-2px)}}\
.qs-fab>summary svg{{width:24px;height:24px}}\
.qs summary:focus-visible,.qs a:focus-visible,.qs button:focus-visible{{outline:3px solid var(--qs-a);outline-offset:3px}}\
.qs-pop{{position:absolute;{side}:0;bottom:64px;width:248px;padding:18px;border-radius:16px;background:#fff;box-shadow:0 18px 50px rgba(15,23,42,.25),0 0 0 1px rgba(15,23,42,.06);text-align:center;animation:qs-in .16s ease-out}}\
@keyframes qs-in{{from{{opacity:0;transform:translateY(6px)}}}}\
.qs-h{{margin:0 0 10px;font-size:15px;font-weight:650;color:#111827}}\
.qs-code{{display:block;width:100%;height:auto;border-radius:8px;background:#fff}}\
.qs-pop .qs-code{{width:212px;height:212px}}\
.qs-url{{margin:10px 0 0;font-size:12px;color:#4B5563;overflow-wrap:anywhere}}\
.qs-dl{{display:inline-flex;align-items:center;gap:6px;margin-top:12px;padding:8px 14px;border:0;border-radius:999px;background:var(--qs-a);color:#fff;font:inherit;font-size:13px;font-weight:600;cursor:pointer;text-decoration:none}}\
.qs-dl[hidden]{{display:none}}\
.qs-dl:hover{{filter:brightness(1.1)}}\
.qs-card{{display:flex;align-items:center;gap:22px;width:fit-content;max-width:calc(100% - 2.5rem);margin:2.5rem auto 0;padding:16px 28px 16px 16px;border:1px solid rgba(127,127,127,.22);border-radius:18px;background:#fff;box-shadow:0 1px 2px rgba(15,23,42,.06)}}\
.qs-card .qs-code{{flex:none;width:132px;height:132px}}\
.qs-card .qs-h{{font-size:17px}}\
.qs-card .qs-url{{margin-top:4px}}\
.qs-wrap{{display:flow-root;padding-bottom:2.5rem}}.qs-print{{display:none}}\
@media print{{.qs-fab,.qs-dl{{display:none!important}}.qs-print{{display:block}}.qs-card{{break-inside:avoid;box-shadow:none;border-color:#999}}}}\
@media (prefers-reduced-motion:reduce){{.qs-fab>summary{{transition:none}}.qs-fab>summary:hover{{transform:none}}.qs-pop{{animation:none}}}}",
        accent = s.accent,
    )
}

/// Closes the popover on Escape or an outside click, and turns the SVG into a
/// downloadable file. Without JavaScript the popover still opens and closes.
const JS: &str = "(function(){var d=document.querySelector('.qs-fab');if(d){document.addEventListener('keydown',function(e){if(e.key==='Escape'&&d.open){d.open=false;d.querySelector('summary').focus()}});document.addEventListener('click',function(e){if(d.open&&!d.contains(e.target))d.open=false})}\
document.querySelectorAll('.qs-dl').forEach(function(b){b.hidden=false;b.addEventListener('click',function(){var p=document.getElementById('qs-code'),v=p.closest('svg').getAttribute('viewBox'),n=v.split(' ')[2],s='<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"'+v+'\" width=\"'+n*16+'\" height=\"'+n*16+'\" shape-rendering=\"crispEdges\"><rect width=\"100%\" height=\"100%\" fill=\"#fff\"/><path d=\"'+p.getAttribute('d')+'\"/></svg>',a=document.createElement('a');a.href=URL.createObjectURL(new Blob([s],{type:'image/svg+xml'}));a.download=b.getAttribute('data-name');document.body.appendChild(a);a.click();a.remove()})})})();";

/// The code as an SVG. The first one on the page carries the path; the rest
/// reuse it, so the modules are in the page once.
fn code_svg(code: &QrCode, url: &str, first: bool) -> String {
    let n = code.size + 2 * QUIET;
    let shape = if first {
        format!("<path id=\"qs-code\" d=\"{}\"/>", qr::svg_path(code, QUIET))
    } else {
        "<use href=\"#qs-code\"/>".to_owned()
    };
    format!(
        "<svg class=\"qs-code\" viewBox=\"0 0 {n} {n}\" role=\"img\" aria-label=\"QR code for {}\" shape-rendering=\"crispEdges\"><rect width=\"{n}\" height=\"{n}\" fill=\"#fff\"/>{shape}</svg>",
        escape(url)
    )
}

fn display_url(url: &str) -> &str {
    let bare = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    bare.strip_suffix('/').unwrap_or(bare)
}

fn file_name(slug: &str) -> String {
    let slug: String = slug
        .trim_matches('/')
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("qr-{}.svg", if slug.is_empty() { "home" } else { &slug })
}

fn heading(s: &Settings, tag: &str) -> String {
    if s.heading.is_empty() {
        String::new()
    } else {
        format!("<{tag} class=\"qs-h\">{}</{tag}>", escape(&s.heading))
    }
}

pub fn qr_block(s: &Settings, url: &str, code: &QrCode, slug: &str) -> String {
    let shown = escape(display_url(url));
    let download = if s.download {
        format!(
            "<button type=\"button\" class=\"qs-dl\" data-name=\"{}\" hidden>Download QR code</button>",
            file_name(slug)
        )
    } else {
        String::new()
    };
    let mut out = String::new();
    if s.placement == "footer" {
        out.push_str("<div class=\"qs qs-wrap\"><aside class=\"qs-card\" aria-label=\"QR code for this page\">");
        out.push_str(&code_svg(code, url, true));
        out.push_str("<div>");
        out.push_str(&heading(s, "p"));
        out.push_str(&format!(
            "<p class=\"qs-url\">{shown}</p>{download}</div></aside></div>"
        ));
    } else {
        out.push_str("<details class=\"qs qs-fab\"><summary aria-label=\"Show a QR code for this page\" title=\"QR code for this page\">");
        out.push_str(QR_GLYPH);
        out.push_str(
            "</summary><div class=\"qs-pop\" role=\"group\" aria-label=\"QR code for this page\">",
        );
        out.push_str(&heading(s, "p"));
        out.push_str(&code_svg(code, url, true));
        out.push_str(&format!(
            "<p class=\"qs-url\">{shown}</p>{download}</div></details>"
        ));
        if s.print {
            // Screen readers skip it: display:none until the page is printed.
            out.push_str("<div class=\"qs qs-wrap qs-print\"><div class=\"qs-card\">");
            out.push_str(&code_svg(code, url, false));
            out.push_str("<div>");
            out.push_str(&heading(s, "p"));
            out.push_str(&format!(
                "<p class=\"qs-url\">{shown}</p></div></div></div>"
            ));
        }
    }
    if s.placement == "button" || s.download {
        out.push_str("<script>");
        out.push_str(JS);
        out.push_str("</script>");
    }
    out
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

// ------------------------------------------------------------------ the panel

fn refuse(values: &Map<String, JsonValue>, error: &str) -> FnResult<Json<PanelResponse>> {
    Ok(Json(PanelResponse {
        values: values.clone(),
        message: String::new(),
        error: error.to_owned(),
    }))
}

#[plugin_fn]
pub fn panel_qr_share(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: Settings::load().to_values(),
            message: String::new(),
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let values = &request.values;
            let site = values
                .get("site-address")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim();
            if !site.is_empty() && origin_of(site).is_none() {
                return refuse(
                    values,
                    "The site address has to start with https:// (or http://) followed by a \
                     domain, like https://example.com. Nothing was saved.",
                );
            }
            if let Some(accent) = values.get("accent").and_then(JsonValue::as_str)
                && !accent.trim().is_empty()
                && !is_hex_color(accent.trim())
            {
                return refuse(
                    values,
                    "The button colour has to be a hex colour like #16A34A. Nothing was saved.",
                );
            }
            let settings = Settings::from_values(values);
            if let Err(error) = kv::set(SETTINGS, &settings.to_values()) {
                let error = if error.is_permission_denied() {
                    "This plugin was not granted the storage permission, so there is nowhere to \
                     keep these settings. It still shows QR codes, with the defaults, on pages \
                     that have an absolute canonical link."
                        .to_owned()
                } else {
                    error.to_string()
                };
                return Ok(Json(PanelResponse {
                    values: settings.to_values(),
                    message: String::new(),
                    error,
                }));
            }
            let known = origin_of(&settings.site_address).is_some()
                || matches!(kv::get::<String>(ORIGIN), Ok(Some(_)));
            let message = if !settings.enabled {
                "Saved. QR codes are off on this site."
            } else if !known {
                "Saved. Add the site address, or publish the site on its domain, and every page \
                 gets its QR code. Until then pages are left as they are."
            } else if settings.placement == "footer" {
                "Saved. Every page now ends with a card showing its QR code."
            } else {
                "Saved. Every page now has a QR code button in the corner."
            };
            Ok(Json(PanelResponse {
                values: settings.to_values(),
                message: message.to_owned(),
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>Menu</title></head>\
<body><div><h1>Menu</h1><p>Soup.</p></div></body></html>";

    #[test]
    fn button_by_default_with_one_path() {
        let out = render(
            PAGE,
            "menu",
            &Settings::default(),
            Some("https://cafe.example"),
        )
        .unwrap();
        assert!(out.contains("</style></head>"));
        assert!(out.contains("aria-label=\"QR code for https://cafe.example/menu\""));
        assert!(out.contains("<p class=\"qs-url\">cafe.example/menu</p>"));
        assert_eq!(out.matches("id=\"qs-code\"").count(), 1);
        assert!(out.contains("<use href=\"#qs-code\"/>"));
        assert!(out.contains("</script></body>"));
        assert!(!out.contains("src="));
    }

    #[test]
    fn footer_goes_inside_the_wrapper() {
        let mut s = Settings::default();
        s.placement = "footer".into();
        s.print = false;
        let out = render(PAGE, "home", &s, Some("https://cafe.example")).unwrap();
        assert!(out.contains("<p>Soup.</p><div class=\"qs qs-wrap\"><aside"));
        assert!(out.contains("QR code for https://cafe.example/\""));
        assert!(!out.contains("<details"));
    }

    #[test]
    fn canonical_wins_and_relative_is_ignored() {
        let page = PAGE.replace(
            "<title>",
            "<link rel=\"canonical\" href=\"https://x.example/a?b=1&amp;c=2\"><title>",
        );
        let out = render(&page, "menu", &Settings::default(), None).unwrap();
        assert!(out.contains("QR code for https://x.example/a?b=1&amp;c=2\""));
        let rel = PAGE.replace("<title>", "<link rel=\"canonical\" href=\"/\"><title>");
        assert!(render(&rel, "menu", &Settings::default(), None).is_none());
    }

    #[test]
    fn leaves_pages_alone_when_it_should() {
        let s = Settings::default();
        assert!(render(PAGE, "menu", &s, None).is_none());
        assert!(render("<p>x</p>", "menu", &s, Some("https://a.example")).is_none());
        let once = render(PAGE, "menu", &s, Some("https://a.example")).unwrap();
        assert!(render(&once, "menu", &s, Some("https://a.example")).is_none());
        let mut ex = Settings::default();
        ex.exclude = "legal/*, menu".into();
        assert!(render(PAGE, "menu", &ex, Some("https://a.example")).is_none());
        assert!(!wanted("legal/terms", "legal/*"));
    }

    #[test]
    fn escapes_admin_text_and_rejects_bad_urls() {
        let mut s = Settings::default();
        s.heading = "<script>x</script>".into();
        let out = render(PAGE, "menu", &s, Some("https://a.example")).unwrap();
        assert!(out.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert_eq!(origin_of("https://a.example\"><x"), None);
        assert_eq!(absolute_url("https://a.example/\"x"), None);
    }

    #[test]
    fn too_long_addresses_are_left_alone() {
        let long = format!("https://a.example/{}", "x".repeat(313));
        assert!(QrCode::encode(long.as_bytes()).is_some());
        let longer = format!("{long}y");
        assert!(QrCode::encode(longer.as_bytes()).is_none());
        let page = PAGE.replace(
            "<title>",
            &format!("<link rel=\"canonical\" href=\"{longer}\"><title>"),
        );
        assert!(render(&page, "menu", &Settings::default(), None).is_none());
    }

    #[test]
    fn bad_values_fall_back() {
        let mut v = Map::new();
        v.insert("placement".into(), "evil".into());
        v.insert("accent".into(), "red;}".into());
        let s = Settings::from_values(&v);
        assert_eq!(s.placement, "button");
        assert_eq!(s.accent, "#16A34A");
    }

    /// Module-for-module comparison with the `qrcode` npm package, when
    /// `QR_FIXTURES` points at the JSON it produced.
    #[test]
    fn matches_reference_encoder() {
        let Ok(path) = std::env::var("QR_FIXTURES") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let cases: Vec<JsonValue> = stride_pdk::serde_json::from_str(&text).unwrap();
        for case in cases {
            let data = case["text"].as_str().unwrap();
            let version = case["version"].as_u64().unwrap() as usize;
            let mask = case["mask"].as_u64().unwrap() as u8;
            let code = QrCode::encode_version(data.as_bytes(), version, Some(mask));
            assert_eq!(
                qr::to_bits(&code),
                case["bits"].as_str().unwrap(),
                "v{version} m{mask} {data}"
            );
            let auto = QrCode::encode(data.as_bytes());
            if version <= qr::MAX_VERSION {
                let size = case["minSize"].as_u64().unwrap() as usize;
                assert_eq!(auto.map(|c| c.size), Some(size), "{data}");
            } else {
                assert!(auto.is_none(), "{data}");
            }
        }
    }
}
