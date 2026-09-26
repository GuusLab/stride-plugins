//! Promo Popup: one friendly popup — a sale, a newsletter, an event — shown
//! after a delay, after scrolling, or when a visitor is about to leave, and
//! not again for as long as the site owner chooses.
//!
//! A native `<dialog>`, so Escape, focus and screen readers work without
//! extra code. Whether it was already seen is kept in the visitor's own
//! browser (localStorage); nothing is sent anywhere. Changing the popup's
//! text makes it a new popup, which is shown again.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 16] = [
    "enabled", "title", "text", "button-text", "button-url", "image-url", "trigger", "delay", "scroll", "frequency",
    "days", "look", "colour", "close-text", "pages", "page-list",
];
const MARKER: &str = "id=\"stride-popup\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    title: String,
    text: String,
    button_text: String,
    button_url: String,
    image_url: String,
    /// `delay`, `scroll` or `exit`.
    trigger: String,
    delay: i64,
    scroll: i64,
    /// `session`, `days` or `once`.
    frequency: String,
    days: i64,
    /// `modal` or `corner`.
    look: String,
    colour: String,
    close_text: String,
    /// `all`, `only` or `except`.
    pages: String,
    page_list: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            title: String::new(),
            text: String::new(),
            button_text: String::new(),
            button_url: String::new(),
            image_url: String::new(),
            trigger: "delay".into(),
            delay: 8,
            scroll: 50,
            frequency: "days".into(),
            days: 7,
            look: "modal".into(),
            colour: "#4f46e5".into(),
            close_text: "No thanks".into(),
            pages: "all".into(),
            page_list: Vec::new(),
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let pick = |name: &str, allowed: &[&str], fallback: &str| {
            let v = text(values, name);
            if allowed.contains(&v.as_str()) { v } else { fallback.to_owned() }
        };
        let int = |name: &str, lo: i64, hi: i64, fallback: i64| {
            values.get(name).and_then(JsonValue::as_i64).map(|v| v.clamp(lo, hi)).unwrap_or(fallback)
        };
        let colour = text(values, "colour").to_ascii_lowercase();
        let close = text(values, "close-text");
        Config {
            enabled: flag(values, "enabled", d.enabled),
            title: text(values, "title"),
            text: values.get("text").and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned(),
            button_text: text(values, "button-text"),
            button_url: text(values, "button-url"),
            image_url: text(values, "image-url"),
            trigger: pick("trigger", &["delay", "scroll", "exit"], &d.trigger),
            delay: int("delay", 0, 300, d.delay),
            scroll: int("scroll", 10, 100, d.scroll),
            frequency: pick("frequency", &["session", "days", "once"], &d.frequency),
            days: int("days", 1, 365, d.days),
            look: pick("look", &["modal", "corner"], &d.look),
            colour: if is_colour(&colour) { colour } else { d.colour },
            close_text: if close.is_empty() { d.close_text } else { close },
            pages: pick("pages", &["all", "only", "except"], &d.pages),
            page_list: slug_list(&text(values, "page-list")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("title".into(), self.title.clone().into());
        values.insert("text".into(), self.text.clone().into());
        values.insert("button-text".into(), self.button_text.clone().into());
        values.insert("button-url".into(), self.button_url.clone().into());
        values.insert("image-url".into(), self.image_url.clone().into());
        values.insert("trigger".into(), self.trigger.clone().into());
        values.insert("delay".into(), self.delay.into());
        values.insert("scroll".into(), self.scroll.into());
        values.insert("frequency".into(), self.frequency.clone().into());
        values.insert("days".into(), self.days.into());
        values.insert("look".into(), self.look.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("close-text".into(), self.close_text.clone().into());
        values.insert("pages".into(), self.pages.clone().into());
        values.insert("page-list".into(), self.page_list.join(", ").into());
        values
    }

    fn applies_to(&self, slug: &str) -> bool {
        match self.pages.as_str() {
            "only" => slug_matches(&self.page_list, slug),
            "except" => !slug_matches(&self.page_list, slug),
            _ => true,
        }
    }
}

/// Images must come over https (or from this site), never `data:` or script.
fn image_url(url: &str) -> Option<String> {
    let url = safe_url(url)?;
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with('/')).then_some(url)
}

fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", escape(p).replace('\n', "<br>")))
        .collect()
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
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let values = match load_values() {
        Ok(values) => values,
        Err(error) => {
            stride_pdk::log("warn", &format!("no popup shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &Config::from_values(&values)) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || config.title.is_empty() || !config.applies_to(slug) || html.contains(MARKER) {
        return html;
    }
    // A new title, text or link is a new campaign, and is shown again.
    let key = format!(
        "stride-popup:{:08x}",
        fnv(&format!("{}\u{1f}{}\u{1f}{}", config.title, config.text, config.button_url))
    );
    let fg = readable_on(&config.colour);
    let image = image_url(&config.image_url)
        .map(|url| format!("<img class=\"spp-img\" src=\"{}\" alt=\"\">", escape(&url)))
        .unwrap_or_default();
    let button = match safe_url(&config.button_url) {
        Some(url) if !config.button_text.is_empty() => {
            format!("<a class=\"spp-cta\" href=\"{}\">{}</a>", escape(&url), escape(&config.button_text))
        }
        _ => String::new(),
    };
    let dialog = format!(
        "<dialog {MARKER} class=\"spp-{look}\" aria-labelledby=\"spp-title\">{image}<div class=\"spp-body\">\
         <h2 id=\"spp-title\">{title}</h2>{text}<div class=\"spp-actions\">{button}\
         <button type=\"button\" class=\"spp-no\">{close}</button></div></div>\
         <button type=\"button\" class=\"spp-x\" aria-label=\"Close\">&times;</button></dialog>",
        look = config.look,
        title = escape(&config.title),
        text = paragraphs(&config.text),
        close = escape(&config.close_text),
    );
    let corner = config.look == "corner";
    let style = format!(
        "<style>#stride-popup{{border:0;padding:0;border-radius:18px;background:#fff;color:#1f2937;overflow:hidden;\
         width:min(440px,calc(100vw - 32px));box-shadow:0 24px 70px rgba(0,0,0,.28);\
         font:16px/1.55 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         #stride-popup::backdrop{{background:rgba(17,24,39,.55)}}\
         #stride-popup.spp-corner{{position:fixed;inset:auto 20px 20px auto;margin:0;width:min(360px,calc(100vw - 40px))}}\
         #stride-popup .spp-img{{display:block;width:100%;height:190px;object-fit:cover}}\
         #stride-popup .spp-body{{padding:26px 28px 24px}}\
         #stride-popup h2{{margin:0 0 8px;font:700 23px/1.25 system-ui,-apple-system,\"Segoe UI\",sans-serif;letter-spacing:-.01em;color:inherit}}\
         #stride-popup p{{margin:0 0 10px;color:#4b5563}}\
         #stride-popup .spp-actions{{display:flex;flex-wrap:wrap;align-items:center;gap:10px 16px;margin-top:18px}}\
         #stride-popup .spp-cta{{display:inline-block;padding:11px 20px;border-radius:10px;background:{c};color:{fg};\
         font-weight:650;text-decoration:none}}#stride-popup .spp-cta:hover{{filter:brightness(1.08)}}\
         #stride-popup .spp-no{{border:0;background:none;color:#6b7280;font:inherit;font-size:14.5px;cursor:pointer;\
         text-decoration:underline;text-underline-offset:3px;padding:6px 2px}}\
         #stride-popup .spp-x{{position:absolute;top:10px;right:10px;width:36px;height:36px;border:0;border-radius:999px;\
         background:rgba(255,255,255,.85);color:#111827;font:400 24px/1 system-ui,sans-serif;cursor:pointer}}\
         #stride-popup :focus-visible{{outline:2px solid {c};outline-offset:2px}}\
         @media (prefers-reduced-motion:no-preference){{#stride-popup[open]{{animation:spp-in .22s ease-out}}}}\
         @keyframes spp-in{{from{{opacity:0;transform:translateY({dy}px)}}}}</style>",
        c = config.colour,
        dy = if corner { 16 } else { 10 },
    );
    let script = format!(
        "<script>(function(){{var d=document.getElementById('stride-popup');if(!d||!d.showModal)return;\
         var K='{key}',F='{freq}',DAYS={days},T='{trigger}',DELAY={delay},SCROLL={scroll},CORNER={corner};\
         function seen(){{try{{if(F==='session')return !!sessionStorage.getItem(K);var v=localStorage.getItem(K);\
         if(!v)return false;return F==='once'||Date.now()-(+v)<DAYS*864e5}}catch(e){{return false}}}}\
         function mark(){{try{{if(F==='session')sessionStorage.setItem(K,'1');else localStorage.setItem(K,''+Date.now())}}catch(e){{}}}}\
         if(seen())return;var done=false;\
         function open(){{if(done||document.querySelector('dialog[open]:not(#stride-popup)'))return;done=true;mark();\
         CORNER?d.show():d.showModal()}}\
         function close(){{d.close()}}d.querySelector('.spp-x').onclick=close;d.querySelector('.spp-no').onclick=close;\
         d.addEventListener('click',function(e){{if(e.target===d&&!CORNER)close()}});\
         d.addEventListener('keydown',function(e){{if(e.key==='Escape')close()}});\
         function onScroll(){{var h=document.documentElement,p=(h.scrollTop+innerHeight)/h.scrollHeight*100;\
         if(p>=SCROLL){{removeEventListener('scroll',onScroll);open()}}}}\
         if(T==='delay')setTimeout(open,DELAY*1000);\
         else if(T==='scroll')addEventListener('scroll',onScroll,{{passive:true}});\
         else{{document.addEventListener('mouseout',function(e){{if(!e.relatedTarget&&e.clientY<=0)open()}});\
         if(matchMedia('(hover:none)').matches){{SCROLL=60;addEventListener('scroll',onScroll,{{passive:true}})}}}}\
         }})()</script>",
        freq = config.frequency,
        days = config.days,
        trigger = config.trigger,
        delay = config.delay,
        scroll = config.scroll,
    );
    let mut html = html;
    insert_in_head(&mut html, &style);
    insert_before_body_end(&mut html, &format!("{dialog}{script}"));
    html
}

#[plugin_fn]
pub fn panel_promo_popup(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let refuse = |error: &str| Ok(Json(PanelResponse { error: error.to_owned(), ..PanelResponse::default() }));
            if !config.button_url.is_empty() && safe_url(&config.button_url).is_none() {
                return refuse("The button link must start with https://, http://, mailto:, tel:, / or #.");
            }
            if config.button_url.is_empty() != config.button_text.is_empty() {
                return refuse("Fill in both the button text and its link, or leave both empty.");
            }
            if !config.image_url.is_empty() && image_url(&config.image_url).is_none() {
                return refuse("The image address must start with https:// or with / for an image on this site.");
            }
            if config.pages != "all" && config.page_list.is_empty() {
                return refuse("List at least one page, or choose \"Every page\".");
            }
            if !config.title.is_empty() && config.text.is_empty() && config.button_text.is_empty() {
                return refuse("Add a text or a button, so the popup tells visitors something.");
            }
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return refuse(&if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is nowhere to keep the popup.".to_owned()
                    } else {
                        error.to_string()
                    });
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: if config.enabled && !config.title.is_empty() {
                    "Saved. Publish your pages again to show the popup. Visitors who saw an earlier version see this one too.".to_owned()
                } else {
                    "Saved. No popup is shown.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head></head><body><main>x</main></body></html>";

    fn config() -> Config {
        Config {
            title: "Join the workshop".into(),
            text: "Build your own stool.\n\nSaturdays, 10:00.".into(),
            button_text: "Book a place".into(),
            button_url: "/workshops".into(),
            ..Config::default()
        }
    }

    #[test]
    fn renders_dialog_once_with_paragraphs() {
        let out = render(PAGE.into(), "home", &config());
        assert!(out.contains("<h2 id=\"spp-title\">Join the workshop</h2><p>Build your own stool.</p><p>Saturdays, 10:00.</p>"));
        assert!(out.contains("<a class=\"spp-cta\" href=\"/workshops\">Book a place</a>"));
        assert!(out.find("<dialog id=\"stride-popup\"").unwrap() > out.find("</main>").unwrap());
        assert!(out.contains("F='days',DAYS=7,T='delay',DELAY=8,SCROLL=50,CORNER=false"));
        assert_eq!(render(out.clone(), "home", &config()), out);
    }

    #[test]
    fn campaign_key_changes_with_content() {
        let a = render(PAGE.into(), "home", &config());
        let b = render(PAGE.into(), "home", &Config { title: "Other".into(), ..config() });
        let key = |s: &str| s[s.find("K='").unwrap()..s.find("',F=").unwrap()].to_owned();
        assert_ne!(key(&a), key(&b));
    }

    #[test]
    fn unsafe_input_is_escaped_or_dropped() {
        let c = Config {
            title: "<img src=x onerror=alert(1)>".into(),
            button_url: "javascript:alert(1)".into(),
            image_url: "data:image/svg+xml,<svg onload=alert(1)>".into(),
            ..config()
        };
        let out = render(PAGE.into(), "home", &c);
        assert!(out.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(!out.contains("javascript:") && !out.contains("data:image"));
    }

    #[test]
    fn page_rules_and_empty_title() {
        assert_eq!(render(PAGE.into(), "home", &Config::default()), PAGE);
        let c = Config { pages: "except".into(), page_list: slug_list("checkout, cart"), ..config() };
        assert_eq!(render(PAGE.into(), "checkout", &c), PAGE);
        assert_ne!(render(PAGE.into(), "shop", &c), PAGE);
    }

    #[test]
    fn numbers_are_clamped() {
        let mut values = config().to_values();
        values.insert("delay".into(), 99999.into());
        values.insert("scroll".into(), (-5).into());
        let c = Config::from_values(&values);
        assert_eq!((c.delay, c.scroll), (300, 10));
    }
}
