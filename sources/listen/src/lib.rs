//! Listen: a "Listen · 4 min" button under the title of long pages that
//! reads the page aloud with the browser's own voices, highlighting the
//! paragraph being read.
//!
//! Nothing is sent anywhere and nothing is downloaded: speech comes from the
//! visitor's device (the Web Speech API). The page is spoken a sentence at a
//! time, because some browsers stop a long utterance after about fifteen
//! seconds, and the button stays hidden when the device has no voice for the
//! page's language.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 7] = ["enabled", "min-words", "highlight", "colour", "language", "pages", "page-list"];
const MARKER: &str = "id=\"stride-listen\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    min_words: usize,
    highlight: bool,
    colour: String,
    language: String,
    pages: String,
    page_list: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            min_words: 250,
            highlight: true,
            colour: "#1d4ed8".into(),
            language: "auto".into(),
            pages: "except".into(),
            page_list: vec!["home".into(), "contact".into()],
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let colour = text(values, "colour").to_ascii_lowercase();
        let language = text(values, "language");
        let pages = text(values, "pages");
        Config {
            enabled: flag(values, "enabled", d.enabled),
            min_words: values.get("min-words").and_then(JsonValue::as_i64).map(|n| n.clamp(0, 5000) as usize).unwrap_or(d.min_words),
            highlight: flag(values, "highlight", d.highlight),
            colour: if is_colour(&colour) { colour } else { d.colour },
            language: if ["auto", "en", "nl", "de", "fr"].contains(&language.as_str()) { language } else { d.language },
            pages: if ["all", "only", "except"].contains(&pages.as_str()) { pages } else { d.pages },
            page_list: if values.contains_key("page-list") { slug_list(&text(values, "page-list")) } else { d.page_list },
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("min-words".into(), (self.min_words as i64).into());
        values.insert("highlight".into(), self.highlight.into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("language".into(), self.language.clone().into());
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

struct Words {
    listen: &'static str,
    pause: &'static str,
    resume: &'static str,
    min: &'static str,
    speed: &'static str,
    stop: &'static str,
}

fn words(language: &str) -> Words {
    match language {
        "nl" => Words { listen: "Luister", pause: "Pauze", resume: "Verder", min: "min", speed: "Snelheid", stop: "Stoppen" },
        "de" => Words { listen: "Anhören", pause: "Pause", resume: "Weiter", min: "Min.", speed: "Tempo", stop: "Stopp" },
        "fr" => Words { listen: "Écouter", pause: "Pause", resume: "Reprendre", min: "min", speed: "Vitesse", stop: "Arrêter" },
        _ => Words { listen: "Listen", pause: "Pause", resume: "Resume", min: "min", speed: "Speed", stop: "Stop" },
    }
}

/// Words of readable text in the page's main content.
fn word_count(html: &str) -> usize {
    let body = inner_of(html, "main").or_else(|| inner_of(html, "body")).unwrap_or(html);
    let body = without_elements(body, &["script", "style", "noscript", "template", "svg", "nav", "header", "footer", "dialog", "form", "button"]);
    text_of(&body).split_whitespace().count()
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
            stride_pdk::log("warn", &format!("listen button left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

const PLAY: &str = "<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\" fill=\"currentColor\"><path d=\"M8 5.5v13l11-6.5z\"/></svg>";

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || !config.applies_to(slug) || html.contains(MARKER) {
        return html;
    }
    let words_on_page = word_count(&html);
    if words_on_page < config.min_words.max(1) {
        return html;
    }
    let language = if config.language == "auto" { page_language(&html) } else { config.language.clone() };
    let w = words(&language);
    let minutes = (words_on_page as f64 / 170.0).ceil().max(1.0) as usize;
    let fg = readable_on(&config.colour);
    let bar = format!(
        "<div {MARKER} hidden><button type=\"button\" class=\"sl-play\" aria-pressed=\"false\">{PLAY}<span class=\"sl-label\">{listen}</span>\
         <span class=\"sl-time\">· {minutes} {min}</span></button>\
         <button type=\"button\" class=\"sl-speed\" aria-label=\"{speed}\" title=\"{speed}\">1×</button>\
         <button type=\"button\" class=\"sl-stop\" aria-label=\"{stop}\" title=\"{stop}\" hidden>■</button>\
         <span class=\"sl-bar\" aria-hidden=\"true\"><i></i></span></div>",
        listen = w.listen,
        min = w.min,
        speed = w.speed,
        stop = w.stop,
    );
    let style = format!(
        "<style id=\"stride-listen-style\">#stride-listen{{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin:14px 0 22px;\
         font:600 14px/1 system-ui,-apple-system,\"Segoe UI\",sans-serif}}#stride-listen[hidden]{{display:none}}\
         #stride-listen button{{display:inline-flex;align-items:center;gap:7px;border:0;border-radius:999px;cursor:pointer;font:inherit;padding:9px 14px}}\
         #stride-listen .sl-play{{background:{c};color:{fg};padding-left:11px}}\
         #stride-listen .sl-time{{font-weight:500;opacity:.85}}\
         #stride-listen .sl-speed,#stride-listen .sl-stop{{background:rgba(127,127,127,.14);color:inherit;min-width:44px;justify-content:center}}\
         #stride-listen button:focus-visible{{outline:2px solid {c};outline-offset:2px}}\
         #stride-listen .sl-bar{{flex:1 1 120px;max-width:220px;height:4px;border-radius:4px;background:rgba(127,127,127,.2);overflow:hidden}}\
         #stride-listen .sl-bar i{{display:block;height:100%;width:0;background:{c};transition:width .3s}}\
         .sl-reading{{background:color-mix(in srgb,{c} 12%,transparent);box-shadow:0 0 0 6px color-mix(in srgb,{c} 12%,transparent);border-radius:4px}}\
         @media (prefers-reduced-motion:reduce){{#stride-listen .sl-bar i{{transition:none}}}}</style>",
        c = config.colour,
    );
    let script = format!(
        "<script>(function(){{var box=document.getElementById('stride-listen'),S=window.speechSynthesis;\
         if(!box||!S||!window.SpeechSynthesisUtterance)return;\
         var LANG=(document.documentElement.lang||'{lang}').toLowerCase(),HL={hl},W={{listen:'{listen}',pause:'{pause}',resume:'{resume}'}};\
         var play=box.querySelector('.sl-play'),label=box.querySelector('.sl-label'),speedB=box.querySelector('.sl-speed'),\
         stopB=box.querySelector('.sl-stop'),bar=box.querySelector('.sl-bar i'),rates=[1,1.25,1.5,.85],ri=0;\
         var root=document.querySelector('main')||document.body,voice=null,parts=[],at=0,state='idle',current=null;\
         function pickVoice(){{var vs=S.getVoices(),base=LANG.split('-')[0];\
         voice=vs.filter(function(v){{return v.lang.toLowerCase()===LANG}})[0]||vs.filter(function(v){{return v.lang.toLowerCase().split(/[-_]/)[0]===base}})[0]||null;\
         box.hidden=!voice}}\
         pickVoice();if(!voice&&S.onvoiceschanged!==undefined)S.onvoiceschanged=pickVoice;\
         function collect(){{parts=[];[].forEach.call(root.querySelectorAll('h1,h2,h3,h4,p,li,blockquote,figcaption'),function(el){{\
         if(el.closest('nav,header,footer,dialog,form,button,[aria-hidden=true],#stride-listen'))return;\
         if(el.querySelector('p,li,h2,h3'))return;var t=(el.innerText||'').replace(/\\s+/g,' ').trim();if(t.length<2)return;\
         (t.match(/[^.!?…]+[.!?…]*[\"'”’)]*\\s*/g)||[t]).forEach(function(s){{s=s.trim();\
         while(s.length>220){{var cut=s.lastIndexOf(' ',220);if(cut<60)cut=220;parts.push([el,s.slice(0,cut)]);s=s.slice(cut).trim()}}\
         if(s)parts.push([el,s])}})}})}}\
         function mark(el){{if(!HL)return;if(current&&current!==el)current.classList.remove('sl-reading');current=el;\
         if(el){{el.classList.add('sl-reading');var r=el.getBoundingClientRect();if(r.top<0||r.bottom>innerHeight)el.scrollIntoView({{block:'center',behavior:matchMedia('(prefers-reduced-motion:reduce)').matches?'auto':'smooth'}})}}}}\
         function speak(){{if(at>=parts.length){{done();return}}var u=new SpeechSynthesisUtterance(parts[at][1]);u.voice=voice;u.lang=voice.lang;u.rate=rates[ri];\
         mark(parts[at][0]);bar.style.width=(at/parts.length*100)+'%';\
         u.onend=function(){{if(state!=='playing')return;at++;speak()}};u.onerror=function(e){{if(e.error!=='interrupted'&&e.error!=='canceled'){{at++;speak()}}}};S.speak(u)}}\
         function set(s){{state=s;play.setAttribute('aria-pressed',s==='playing');\
         play.querySelector('path').setAttribute('d',s==='playing'?'M7 5h4v14H7zM13 5h4v14h-4z':'M8 5.5v13l11-6.5z');label.textContent=s==='playing'?W.pause:s==='paused'?W.resume:W.listen;stopB.hidden=s==='idle'}}\
         function done(){{S.cancel();set('idle');at=0;mark(null);bar.style.width='0'}}\
         play.addEventListener('click',function(){{if(state==='playing'){{set('paused');S.cancel()}}\
         else{{if(state==='idle'){{collect();at=0}}set('playing');S.cancel();speak()}}}});\
         stopB.addEventListener('click',done);\
         speedB.addEventListener('click',function(){{ri=(ri+1)%rates.length;speedB.textContent=rates[ri]+'×';\
         if(state==='playing'){{S.cancel();speak()}}}});\
         addEventListener('pagehide',function(){{S.cancel()}})}})()</script>",
        lang = js_string(&language),
        hl = config.highlight,
        listen = js_string(w.listen),
        pause = js_string(w.pause),
        resume = js_string(w.resume),
    );

    let mut html = html;
    insert_in_head(&mut html, &style);
    // Right after the page title, so it reads as part of the article.
    let lower = html.to_ascii_lowercase();
    match lower.find("<h1").and_then(|h| lower[h..].find("</h1>").map(|e| h + e + 5)) {
        Some(after_h1) => html.insert_str(after_h1, &bar),
        None => match lower.find("<main").and_then(|m| lower[m..].find('>').map(|g| m + g + 1)) {
            Some(in_main) => html.insert_str(in_main, &bar),
            None => insert_after_body_start(&mut html, &bar),
        },
    }
    insert_before_body_end(&mut html, &script);
    html
}

#[plugin_fn]
pub fn panel_listen(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            if config.pages == "only" && config.page_list.is_empty() {
                return Ok(Json(PanelResponse {
                    error: "List at least one page, or choose another option under Pages.".to_owned(),
                    ..PanelResponse::default()
                }));
            }
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
            Ok(Json(PanelResponse { values, message: "Saved. Your pages show the change right away.".to_owned(), error: String::new() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(words: usize) -> String {
        format!(
            "<html lang=\"nl\"><head></head><body><header><nav>Menu</nav></header><main><h1 class=\"t\">Titel</h1><p>{}</p></main></body></html>",
            "woord ".repeat(words)
        )
    }

    #[test]
    fn button_after_the_title_on_long_pages_only() {
        let out = render(page(400), "blog/lang", &Config::default());
        let bar = out.find("<div id=\"stride-listen\" hidden>").unwrap();
        assert!(bar > out.find("</h1>").unwrap() && bar < out.find("<p>").unwrap());
        assert!(out.contains("<span class=\"sl-label\">Luister</span><span class=\"sl-time\">· 3 min</span>"));
        assert_eq!(render(out.clone(), "blog/lang", &Config::default()), out);
        let short = page(50);
        assert_eq!(render(short.clone(), "blog/kort", &Config::default()), short);
    }

    #[test]
    fn page_rules() {
        let long = page(400);
        assert_eq!(render(long.clone(), "home", &Config::default()), long, "home is skipped by default");
        let only = Config { pages: "only".into(), page_list: slug_list("blog/*"), ..Config::default() };
        assert_eq!(render(long.clone(), "about", &only), long);
        assert_ne!(render(long.clone(), "blog/x", &only), long);
    }

    #[test]
    fn counts_main_text_only() {
        let html = "<body><header>a b c d e</header><main><p>one two three</p><script>x y z</script></main><footer>f g</footer></body>";
        assert_eq!(word_count(html), 3);
    }
}
