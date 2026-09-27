//! QR Code: a QR code of each page's address, drawn on the server as SVG.
//! Visitors can open it from a small button and download it; editors can
//! place it in a page with `[qr-code]`; and printed pages can carry it in a
//! corner, so a printed menu, flyer or recipe leads back to the page.
//!
//! A QR code needs the page's full address. It comes from the "Site address"
//! setting, else from the address Stride reports in `on_publish` (only once
//! the site has a domain), else from the page's canonical link. Without any
//! of them the plugin adds nothing: a QR code of `/menu` would scan to
//! nowhere.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use qrcodegen::{QrCode, QrCodeEcc};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Published, kv};
use util::*;

const FIELDS: [&str; 8] = ["enabled", "button", "print", "site-address", "track", "colour", "language", "skip-pages"];
const LEARNED_ORIGIN: &str = "origin";
const CODE: &str = "[qr-code]";
const MARKER: &str = "id=\"stride-qr\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    /// Show a small QR button on every page.
    button: bool,
    /// Put the code in a corner of printed pages.
    print: bool,
    site_address: String,
    /// Add `?ref=qr`, so scans can be told apart in analytics.
    track: bool,
    colour: String,
    language: String,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            button: true,
            print: true,
            site_address: String::new(),
            track: false,
            colour: "#111827".into(),
            language: "auto".into(),
            skip_pages: Vec::new(),
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let colour = text(values, "colour").to_ascii_lowercase();
        let language = text(values, "language");
        Config {
            enabled: flag(values, "enabled", d.enabled),
            button: flag(values, "button", d.button),
            print: flag(values, "print", d.print),
            site_address: text(values, "site-address"),
            track: flag(values, "track", d.track),
            colour: if is_colour(&colour) { colour } else { d.colour },
            language: if ["auto", "en", "nl", "de", "fr"].contains(&language.as_str()) { language } else { d.language },
            skip_pages: slug_list(&text(values, "skip-pages")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("button".into(), self.button.into());
        values.insert("print".into(), self.print.into());
        values.insert("site-address".into(), self.site_address.clone().into());
        values.insert("track".into(), self.track.into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
    }
}

/// `https://example.com/anything` -> `https://example.com`.
fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let scheme = if lower.starts_with("https://") { 8 } else if lower.starts_with("http://") { 7 } else { return None };
    let end = url[scheme..].find(['/', '?', '#']).map(|i| scheme + i).unwrap_or(url.len());
    let host = &url[scheme..end];
    (!host.is_empty() && !host.contains(char::is_whitespace) && !host.contains(['"', '<', '>', '\'']))
        .then(|| format!("{}{}", &lower[..scheme], host.to_ascii_lowercase()))
}

fn canonical_origin(html: &str) -> Option<String> {
    let head = inner_of(html, "head")?;
    let lower = head.to_ascii_lowercase();
    let at = lower.find("rel=\"canonical\"")?;
    let start = lower[..at].rfind('<')?;
    let tag = &head[start..at + lower[at..].find('>')?];
    origin_of(tag.split("href=\"").nth(1)?.split('"').next()?)
}

fn page_url(origin: &str, slug: &str, track: bool) -> String {
    let path = if slug == "home" { String::new() } else { slug.trim_matches('/').to_owned() };
    format!("{origin}/{path}{}", if track { "?ref=qr" } else { "" })
}

/// The code as one SVG path, with a quiet zone of four modules.
fn qr_svg(text: &str, colour: &str, label: &str) -> Option<String> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    let size = qr.size();
    let border = 4;
    let mut d = String::new();
    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x, y) {
                d.push_str(&format!("M{},{}h1v1h-1z", x + border, y + border));
            }
        }
    }
    let dim = size + border * 2;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {dim} {dim}\" shape-rendering=\"crispEdges\" role=\"img\" aria-label=\"{}\">\
         <rect width=\"{dim}\" height=\"{dim}\" fill=\"#fff\"/><path d=\"{d}\" fill=\"{colour}\"/></svg>",
        escape(label)
    ))
}

struct Words {
    button: &'static str,
    title: &'static str,
    scan: &'static str,
    download: &'static str,
    close: &'static str,
    label: &'static str,
}

fn words(language: &str) -> Words {
    match language {
        "nl" => Words { button: "QR-code", title: "Open deze pagina op je telefoon", scan: "Scan de code met de camera van je telefoon.", download: "Download", close: "Sluiten", label: "QR-code naar deze pagina" },
        "de" => Words { button: "QR-Code", title: "Diese Seite auf dem Handy öffnen", scan: "Scanne den Code mit der Kamera deines Handys.", download: "Herunterladen", close: "Schließen", label: "QR-Code zu dieser Seite" },
        "fr" => Words { button: "QR code", title: "Ouvrir cette page sur votre téléphone", scan: "Scannez le code avec l'appareil photo de votre téléphone.", download: "Télécharger", close: "Fermer", label: "QR code vers cette page" },
        _ => Words { button: "QR code", title: "Open this page on your phone", scan: "Scan the code with your phone's camera.", download: "Download", close: "Close", label: "QR code to this page" },
    }
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
pub fn on_publish(Json(event): Json<Published>) -> FnResult<Json<JsonValue>> {
    if let Some(origin) = origin_of(&event.url) {
        if kv::get::<String>(LEARNED_ORIGIN).ok().flatten().as_deref() != Some(origin.as_str()) {
            let _ = kv::set(LEARNED_ORIGIN, &origin);
        }
    }
    Ok(Json(JsonValue::Object(Map::new())))
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let (config, learned) = match load_values() {
        Ok(values) => (Config::from_values(&values), kv::get::<String>(LEARNED_ORIGIN).ok().flatten()),
        Err(error) if error.is_permission_denied() => (Config::default(), None),
        Err(error) => {
            stride_pdk::log("warn", &format!("QR code left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config, learned.as_deref()) }))
}

const ICON: &str = "<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\">\
<rect x=\"3\" y=\"3\" width=\"7\" height=\"7\" rx=\"1\"/><rect x=\"14\" y=\"3\" width=\"7\" height=\"7\" rx=\"1\"/><rect x=\"3\" y=\"14\" width=\"7\" height=\"7\" rx=\"1\"/>\
<path d=\"M14 14h3v3h-3zM20 14v.01M14 20h.01M17 17h4v4h-4\"/></svg>";

fn render(html: String, slug: &str, config: &Config, learned: Option<&str>) -> String {
    let has_code = html.contains(CODE);
    if !config.enabled || html.contains(MARKER) || (slug_matches(&config.skip_pages, slug) && !has_code) {
        return html;
    }
    let Some(origin) = origin_of(&config.site_address).or_else(|| learned.and_then(origin_of)).or_else(|| canonical_origin(&html)) else {
        return html;
    };
    let url = page_url(&origin, slug, config.track);
    let language = if config.language == "auto" { page_language(&html) } else { config.language.clone() };
    let w = words(&language);
    let Some(svg) = qr_svg(&url, &config.colour, w.label) else { return html };
    let shown_url = escape(&url.replace("?ref=qr", ""));

    let mut html = html;
    if has_code {
        let block = format!("<figure class=\"sqr-inline\">{svg}<figcaption>{shown_url}</figcaption></figure>");
        // A paragraph holding only the code is replaced whole: a figure may not sit inside <p>.
        let mut out = String::with_capacity(html.len());
        let mut rest = html.as_str();
        while let Some(at) = rest.find(CODE) {
            let before = &rest[..at];
            let after = &rest[at + CODE.len()..];
            let p = before.rfind("<p").filter(|&p| before[p..].find('>').is_some_and(|g| before[p + g + 1..].trim().is_empty()));
            match p.filter(|_| after.trim_start().starts_with("</p>")) {
                Some(p) => {
                    out.push_str(&before[..p]);
                    out.push_str(&block);
                    rest = &after[after.find("</p>").map(|c| c + 4).unwrap_or(0)..];
                }
                None => {
                    out.push_str(before);
                    out.push_str(&block);
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        html = out;
    }

    let skip = slug_matches(&config.skip_pages, slug);
    let mut extra = String::new();
    if config.button && !skip {
        let data = format!("data:image/svg+xml;charset=utf-8,{}", svg.replace('%', "%25").replace('#', "%23").replace('"', "'").replace('<', "%3C").replace('>', "%3E"));
        extra.push_str(&format!(
            "<button type=\"button\" class=\"sqr-open\" aria-haspopup=\"dialog\" aria-controls=\"stride-qr\">{ICON}<span>{button}</span></button>\
             <dialog {MARKER} aria-labelledby=\"sqr-title\"><h2 id=\"sqr-title\">{title}</h2>{svg}<p class=\"sqr-url\">{shown_url}</p>\
             <p class=\"sqr-hint\">{scan}</p><div class=\"sqr-actions\"><a href=\"{data}\" download=\"qr-{file}.svg\">{download}</a>\
             <button type=\"button\" class=\"sqr-close\">{close}</button></div></dialog>\
             <script>(function(){{var d=document.getElementById('stride-qr');if(!d||!d.showModal)return;\
             document.querySelector('.sqr-open').addEventListener('click',function(){{d.showModal()}});\
             d.querySelector('.sqr-close').addEventListener('click',function(){{d.close()}});\
             d.addEventListener('click',function(e){{if(e.target===d)d.close()}})}})()</script>",
            button = w.button,
            title = w.title,
            scan = w.scan,
            download = w.download,
            close = w.close,
            data = escape(&data),
            file = if slug == "home" { "home".to_owned() } else { slug.replace('/', "-") },
        ));
    }
    if config.print && !skip {
        extra.push_str(&format!("<div class=\"sqr-print\" aria-hidden=\"true\">{svg}<span>{shown_url}</span></div>"));
    }
    let style = format!(
        "<style id=\"stride-qr-style\">.sqr-inline{{display:inline-grid;justify-items:center;gap:6px;margin:1em 0}}\
         .sqr-inline svg{{width:180px;height:180px}}.sqr-inline figcaption{{font:13px/1.3 system-ui,sans-serif;opacity:.75;word-break:break-all;max-width:220px;text-align:center}}\
         .sqr-open{{position:fixed;right:18px;bottom:18px;z-index:2147481000;display:inline-flex;align-items:center;gap:7px;padding:10px 14px;\
         border:1px solid rgba(0,0,0,.1);border-radius:999px;background:#fff;color:#111827;box-shadow:0 8px 24px rgba(0,0,0,.14);\
         font:600 13.5px/1 system-ui,-apple-system,\"Segoe UI\",sans-serif;cursor:pointer}}\
         .sqr-open:focus-visible,#stride-qr :focus-visible{{outline:2px solid {c};outline-offset:2px}}\
         #stride-qr{{border:0;border-radius:18px;padding:26px 28px 22px;width:min(340px,calc(100vw - 32px));text-align:center;\
         box-shadow:0 30px 80px rgba(0,0,0,.3);font:15px/1.45 system-ui,-apple-system,\"Segoe UI\",sans-serif;color:#111827}}\
         #stride-qr::backdrop{{background:rgba(15,23,42,.5)}}#stride-qr h2{{font:700 19px/1.3 inherit;margin:0 0 14px}}\
         #stride-qr svg{{width:220px;height:220px;display:block;margin:0 auto}}\
         #stride-qr .sqr-url{{font-size:13px;color:#4b5563;word-break:break-all;margin:10px 0 4px}}#stride-qr .sqr-hint{{font-size:13.5px;color:#6b7280;margin:0 0 16px}}\
         #stride-qr .sqr-actions{{display:flex;gap:10px;justify-content:center}}\
         #stride-qr .sqr-actions a,#stride-qr .sqr-actions button{{padding:9px 16px;border-radius:10px;font:600 14px/1 inherit;cursor:pointer;text-decoration:none}}\
         #stride-qr .sqr-actions a{{background:{c};color:{fg}}}#stride-qr .sqr-actions button{{background:#f3f4f6;border:0;color:#111827}}\
         .sqr-print{{display:none}}\
         @media print{{.sqr-open,#stride-qr{{display:none!important}}.sqr-print{{display:flex;position:fixed;right:10mm;bottom:10mm;\
         flex-direction:column;align-items:center;gap:2mm;font:8pt/1.2 system-ui,sans-serif}}.sqr-print svg{{width:28mm;height:28mm}}}}</style>",
        c = config.colour,
        fg = readable_on(&config.colour),
    );
    insert_in_head(&mut html, &style);
    if !extra.is_empty() {
        insert_before_body_end(&mut html, &extra);
    }
    html
}

#[plugin_fn]
pub fn panel_qr_code(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            let known = kv::get::<String>(LEARNED_ORIGIN).ok().flatten();
            let message = match (text(&values, "site-address").is_empty(), known) {
                (false, _) => String::new(),
                (true, Some(origin)) => format!("Codes point to {origin}, the address Stride reported when you last published."),
                (true, None) => "Fill in your site address, or publish a page once your site has a domain, to show QR codes.".to_owned(),
            };
            Ok(Json(PanelResponse { values, message, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            if !config.site_address.is_empty() && origin_of(&config.site_address).is_none() {
                return Ok(Json(PanelResponse {
                    error: "Write the site address with https:// in front, for example https://example.com.".to_owned(),
                    ..PanelResponse::default()
                }));
            }
            let mut values = config.to_values();
            if let Some(origin) = origin_of(&config.site_address) {
                values.insert("site-address".into(), origin.into());
            }
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return Ok(Json(PanelResponse {
                        error: if error.is_permission_denied() {
                            "This plugin was not granted the storage permission, so it cannot keep your site address.".to_owned()
                        } else {
                            error.to_string()
                        },
                        ..PanelResponse::default()
                    }));
                }
            }
            Ok(Json(PanelResponse { values, message: "Saved. Your pages show the change right away.".to_owned(), error: String::new() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html lang=\"en\"><head><title>Menu</title></head><body><main><h1>Menu</h1><p class=\"x\">[qr-code]</p></main></body></html>";

    #[test]
    fn nothing_without_an_address() {
        assert_eq!(render(PAGE.into(), "menu", &Config::default(), None), PAGE);
    }

    #[test]
    fn inline_button_and_print_with_a_learned_address() {
        let out = render(PAGE.into(), "menu", &Config::default(), Some("https://Harbor.test/whatever"));
        assert!(out.contains("<figure class=\"sqr-inline\"><svg"));
        assert!(!out.contains("[qr-code]") && !out.contains("<p class=\"x\">"));
        assert!(out.contains("<figcaption>https://harbor.test/menu</figcaption>"));
        assert!(out.contains("class=\"sqr-open\"") && out.contains("class=\"sqr-print\""));
        assert!(out.contains("download=\"qr-menu.svg\""));
        assert_eq!(render(out.clone(), "menu", &Config::default(), Some("https://harbor.test")), out);
    }

    #[test]
    fn setting_beats_learned_and_home_is_the_root() {
        let c = Config { site_address: "https://example.com/".into(), track: true, ..Config::default() };
        let out = render("<html><body><p>x</p></body></html>".into(), "home", &c, Some("https://other.test"));
        assert!(out.contains("<p class=\"sqr-url\">https://example.com/</p>"));
    }

    #[test]
    fn svg_is_a_valid_square() {
        let svg = qr_svg("https://example.com/menu", "#111827", "QR").unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("viewBox=\"0 0 33 33\""));
        assert!(origin_of("javascript:alert(1)").is_none());
        assert!(origin_of("https://a.b\"onload=x").is_none());
    }
}
