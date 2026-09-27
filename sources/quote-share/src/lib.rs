//! Quote Share: select a sentence in an article and a small bar appears to
//! share it as a quote — on X, Bluesky, WhatsApp, by email, or as a link
//! that opens the page with that very sentence highlighted.
//!
//! Plain share links and the browser's own text fragments (`#:~:text=`):
//! nothing is loaded from any network until a reader clicks, and nobody is
//! tracked. The bar is a few hundred bytes of inline script.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 10] = ["enabled", "x", "bluesky", "whatsapp", "email", "copy", "colour", "language", "skip-pages", "min-words"];
const MARKER: &str = "id=\"stride-quote\"";
const NETWORKS: [&str; 5] = ["x", "bluesky", "whatsapp", "email", "copy"];

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    networks: Vec<(String, bool)>,
    colour: String,
    language: String,
    skip_pages: Vec<String>,
    min_words: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            networks: NETWORKS.iter().map(|n| ((*n).to_owned(), true)).collect(),
            colour: "#111827".into(),
            language: "auto".into(),
            skip_pages: vec!["home".into()],
            min_words: 150,
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
            networks: NETWORKS.iter().map(|n| ((*n).to_owned(), flag(values, n, true))).collect(),
            colour: if is_colour(&colour) { colour } else { d.colour },
            language: if ["auto", "en", "nl", "de", "fr"].contains(&language.as_str()) { language } else { d.language },
            skip_pages: if values.contains_key("skip-pages") { slug_list(&text(values, "skip-pages")) } else { d.skip_pages },
            min_words: values.get("min-words").and_then(JsonValue::as_i64).map(|n| n.clamp(0, 5000) as usize).unwrap_or(d.min_words),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        for (n, on) in &self.networks {
            values.insert(n.clone(), (*on).into());
        }
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values.insert("min-words".into(), (self.min_words as i64).into());
        values
    }

    fn on(&self, network: &str) -> bool {
        self.networks.iter().any(|(n, on)| n == network && *on)
    }
}

struct Words {
    bar: &'static str,
    x: &'static str,
    bluesky: &'static str,
    whatsapp: &'static str,
    email: &'static str,
    copy: &'static str,
    copied: &'static str,
}

fn words(language: &str) -> Words {
    match language {
        "nl" => Words { bar: "Deel dit citaat", x: "Deel op X", bluesky: "Deel op Bluesky", whatsapp: "Deel via WhatsApp", email: "Deel via e-mail", copy: "Kopieer citaat met link", copied: "Gekopieerd" },
        "de" => Words { bar: "Zitat teilen", x: "Auf X teilen", bluesky: "Auf Bluesky teilen", whatsapp: "Über WhatsApp teilen", email: "Per E-Mail teilen", copy: "Zitat mit Link kopieren", copied: "Kopiert" },
        "fr" => Words { bar: "Partager la citation", x: "Partager sur X", bluesky: "Partager sur Bluesky", whatsapp: "Partager sur WhatsApp", email: "Partager par e-mail", copy: "Copier la citation avec le lien", copied: "Copié" },
        _ => Words { bar: "Share this quote", x: "Share on X", bluesky: "Share on Bluesky", whatsapp: "Share on WhatsApp", email: "Share by email", copy: "Copy quote with link", copied: "Copied" },
    }
}

fn icon(network: &str) -> &'static str {
    match network {
        "x" => "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M17.8 3h3.1l-6.8 7.8L22 21h-6.2l-4.9-6.4L5.3 21H2.2l7.3-8.3L2 3h6.4l4.4 5.8zm-1.1 16.2h1.7L7.4 4.7H5.6z\"/></svg>",
        "bluesky" => "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M6.3 4.2C8.6 5.9 11 9.4 12 11.2c1-1.8 3.4-5.3 5.7-7 1.7-1.2 4.3-2.1 4.3.8 0 .6-.3 4.9-.5 5.6-.7 2.4-3.2 3-5.4 2.6 3.9.7 4.9 2.9 2.7 5.1-4.1 4.2-5.9-1-6.4-2.4l-.4-1.1-.4 1.1c-.5 1.4-2.3 6.6-6.4 2.4-2.2-2.2-1.2-4.4 2.7-5.1-2.2.4-4.7-.3-5.4-2.6C2.3 9.9 2 5.6 2 5c0-2.9 2.6-2 4.3-.8z\"/></svg>",
        "whatsapp" => "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M12 2a10 10 0 0 0-8.6 15.1L2 22l5-1.3A10 10 0 1 0 12 2zm5.3 14.2c-.2.6-1.3 1.2-1.8 1.2-.5.1-1 .2-3.3-.7-2.8-1.1-4.5-4-4.7-4.2-.1-.2-1.1-1.5-1.1-2.8s.7-2 1-2.3c.2-.3.5-.3.7-.3h.5c.2 0 .4 0 .6.5l.8 2c.1.2.1.4 0 .5l-.4.6-.3.4c-.1.1-.3.3-.1.6.2.3.8 1.3 1.7 2.1 1.2 1 2.1 1.4 2.4 1.5.3.1.5.1.6-.1l.9-1c.2-.3.4-.2.6-.1l1.9.9c.3.1.5.2.5.3.1.2.1.8-.1 1.3z\"/></svg>",
        "email" => "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" d=\"M3 5h18v14H3zM3 7l9 6 9-6\"/></svg>",
        _ => "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" d=\"M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1\"/></svg>",
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
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let config = match load_values() {
        Ok(values) => Config::from_values(&values),
        Err(error) if error.is_permission_denied() => Config::default(),
        Err(error) => {
            stride_pdk::log("warn", &format!("quote sharing left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || slug_matches(&config.skip_pages, slug) || html.contains(MARKER) || !NETWORKS.iter().any(|n| config.on(n)) {
        return html;
    }
    let body = inner_of(&html, "main").or_else(|| inner_of(&html, "body")).unwrap_or(&html);
    let body = without_elements(body, &["script", "style", "nav", "header", "footer", "svg", "dialog"]);
    if text_of(&body).split_whitespace().count() < config.min_words.max(1) {
        return html;
    }
    let language = if config.language == "auto" { page_language(&html) } else { config.language.clone() };
    let w = words(&language);
    let label = |n: &str| match n {
        "x" => w.x,
        "bluesky" => w.bluesky,
        "whatsapp" => w.whatsapp,
        "email" => w.email,
        _ => w.copy,
    };
    let buttons: String = NETWORKS
        .iter()
        .filter(|n| config.on(n))
        .map(|n| format!("<button type=\"button\" data-sq=\"{n}\" aria-label=\"{l}\" title=\"{l}\">{}</button>", icon(n), l = label(n)))
        .collect();
    let bar = format!("<div {MARKER} role=\"toolbar\" aria-label=\"{}\" hidden>{buttons}<span class=\"sq-done\" role=\"status\"></span></div>", w.bar);
    let fg = readable_on(&config.colour);
    let style = format!(
        "<style id=\"stride-quote-style\">#stride-quote{{position:absolute;z-index:2147480500;display:flex;gap:2px;padding:4px;border-radius:12px;\
         background:{c};color:{fg};box-shadow:0 10px 30px rgba(0,0,0,.25);transform:translate(-50%,-100%);margin-top:-10px}}\
         #stride-quote[hidden]{{display:none}}#stride-quote::after{{content:\"\";position:absolute;left:50%;top:100%;margin-left:-6px;\
         border:6px solid transparent;border-top-color:{c}}}\
         #stride-quote button{{display:grid;place-items:center;width:36px;height:36px;border:0;border-radius:9px;background:transparent;\
         color:inherit;cursor:pointer}}#stride-quote button:hover{{background:rgba(127,127,127,.28)}}\
         #stride-quote button:focus-visible{{outline:2px solid currentColor;outline-offset:-2px}}\
         #stride-quote svg{{width:18px;height:18px}}#stride-quote .sq-done{{position:absolute;left:50%;top:-30px;transform:translateX(-50%);\
         white-space:nowrap;font:600 12px/1 system-ui,sans-serif;background:{c};color:{fg};padding:6px 9px;border-radius:7px}}\
         #stride-quote .sq-done:empty{{display:none}}</style>",
        c = config.colour,
    );
    let script = format!(
        "<script>(function(){{var bar=document.getElementById('stride-quote');if(!bar||!window.getSelection)return;\
         var root=document.querySelector('main')||document.body,done=bar.querySelector('.sq-done'),quote='',timer;\
         function clean(s){{return s.replace(/\\s+/g,' ').trim()}}\
         function inRoot(n){{var e=n&&(n.nodeType===1?n:n.parentElement);return e&&root.contains(e)&&!e.closest('nav,header,footer,input,textarea,button,a,#stride-quote,[contenteditable]')}}\
         function hide(){{bar.hidden=true;done.textContent=''}}\
         function show(){{var sel=getSelection();if(!sel.rangeCount||sel.isCollapsed){{hide();return}}\
         var t=clean(sel.toString());if(t.length<12||t.length>600||!inRoot(sel.anchorNode)||!inRoot(sel.focusNode)){{hide();return}}\
         quote=t.length>240?t.slice(0,237).replace(/\\s+\\S*$/,'')+'…':t;var r=sel.getRangeAt(0).getBoundingClientRect();\
         bar.hidden=false;bar.style.left=Math.min(Math.max(r.left+r.width/2+scrollX,bar.offsetWidth/2+8),scrollX+innerWidth-bar.offsetWidth/2-8)+'px';\
         bar.style.top=(r.top+scrollY)+'px'}}\
         document.addEventListener('selectionchange',function(){{clearTimeout(timer);timer=setTimeout(show,180)}});\
         addEventListener('resize',hide);\
         function link(){{var u=location.href.split('#')[0],words=quote.replace(/…$/,'').split(' '),frag;\
         frag=words.length>8?encodeURIComponent(words.slice(0,4).join(' '))+','+encodeURIComponent(words.slice(-4).join(' ')):encodeURIComponent(quote.replace(/…$/,''));\
         return u+'#:~:text='+frag.replace(/-/g,'%2D')}}\
         function go(url){{window.open(url,'_blank','noopener')}}\
         bar.addEventListener('mousedown',function(e){{e.preventDefault()}});\
         bar.addEventListener('click',function(e){{var b=e.target.closest('button');if(!b)return;var q='\\u201c'+quote+'\\u201d',u=link(),t=document.title;\
         switch(b.dataset.sq){{case 'x':go('https://x.com/intent/post?text='+encodeURIComponent(q)+'&url='+encodeURIComponent(u));break;\
         case 'bluesky':go('https://bsky.app/intent/compose?text='+encodeURIComponent(q+' '+u));break;\
         case 'whatsapp':go('https://wa.me/?text='+encodeURIComponent(q+' '+u));break;\
         case 'email':location.href='mailto:?subject='+encodeURIComponent(t)+'&body='+encodeURIComponent(q+'\\n\\n'+u);break;\
         default:var s=q+'\\n'+u;(navigator.clipboard&&isSecureContext?navigator.clipboard.writeText(s):Promise.reject()).catch(function(){{\
         var a=document.createElement('textarea');a.value=s;a.style.position='fixed';a.style.opacity='0';document.body.appendChild(a);a.select();\
         try{{document.execCommand('copy')}}catch(x){{}}document.body.removeChild(a)}}).then(function(){{done.textContent='{copied}';\
         setTimeout(hide,1400)}},function(){{done.textContent='{copied}';setTimeout(hide,1400)}})}}}});\
         document.addEventListener('keydown',function(e){{if(e.key==='Escape')hide()}})}})()</script>",
        copied = js_string(w.copied),
    );
    let mut html = html;
    insert_in_head(&mut html, &style);
    insert_before_body_end(&mut html, &format!("{bar}{script}"));
    html
}

#[plugin_fn]
pub fn panel_quote_share(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return Ok(Json(PanelResponse {
                        error: if error.is_permission_denied() {
                            "This plugin was not granted the storage permission, so it keeps the default settings.".to_owned()
                        } else {
                            error.to_string()
                        },
                        ..PanelResponse::default()
                    }));
                }
            }
            let message = if NETWORKS.iter().any(|n| config.on(n)) {
                "Saved. Your pages show the change right away."
            } else {
                "Saved. Every button is off, so no share bar is shown."
            };
            Ok(Json(PanelResponse { values, message: message.to_owned(), error: String::new() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(words: usize) -> String {
        format!("<html lang=\"de\"><head></head><body><main><h1>T</h1><p>{}</p></main></body></html>", "Wort ".repeat(words))
    }

    #[test]
    fn adds_the_bar_to_long_pages_with_chosen_buttons() {
        let mut c = Config::default();
        c.networks.retain(|(n, _)| n != "bluesky");
        let out = render(page(200), "blog/x", &c);
        assert!(out.contains("aria-label=\"Zitat teilen\""));
        assert!(out.contains("data-sq=\"x\"") && !out.contains("data-sq=\"bluesky\""));
        assert!(out.find("<div id=\"stride-quote\"").unwrap() > out.find("</main>").unwrap());
        assert_eq!(render(out.clone(), "blog/x", &c), out);
    }

    #[test]
    fn short_pages_skipped_pages_and_no_buttons_change_nothing() {
        let short = page(20);
        assert_eq!(render(short.clone(), "blog/x", &Config::default()), short);
        let long = page(200);
        assert_eq!(render(long.clone(), "home", &Config::default()), long);
        let none = Config { networks: NETWORKS.iter().map(|n| ((*n).to_owned(), false)).collect(), ..Config::default() };
        assert_eq!(render(long.clone(), "blog/x", &none), long);
    }

    #[test]
    fn settings_round_trip() {
        let c = Config { colour: "#0f766e".into(), min_words: 10, ..Config::default() };
        assert_eq!(Config::from_values(&c.to_values()), c);
    }
}
