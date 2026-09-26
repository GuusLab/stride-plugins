//! Image Lightbox: click an image on a published page to see it large, in a
//! native `<dialog>`, with the alt text as its caption and arrows to step
//! through the other images on the page.
//!
//! One permission (`storage`, for the settings), one hook, one panel. With
//! storage refused the defaults apply, because a lightbox that works out of
//! the box is the plugin's whole promise.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 6] = ["enabled", "which", "captions", "arrows", "backdrop", "skip-pages"];
const MARKER: &str = "id=\"stride-lightbox\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    /// `large` (only images shown smaller than they are) or `all`.
    which: String,
    captions: bool,
    arrows: bool,
    /// `dark` or `light`.
    backdrop: String,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            which: "large".into(),
            captions: true,
            arrows: true,
            backdrop: "dark".into(),
            skip_pages: Vec::new(),
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            which: if text(values, "which") == "all" { "all".into() } else { d.which },
            captions: flag(values, "captions", d.captions),
            arrows: flag(values, "arrows", d.arrows),
            backdrop: if text(values, "backdrop") == "light" { "light".into() } else { d.backdrop },
            skip_pages: slug_list(&text(values, "skip-pages")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("which".into(), self.which.clone().into());
        values.insert("captions".into(), self.captions.into());
        values.insert("arrows".into(), self.arrows.into());
        values.insert("backdrop".into(), self.backdrop.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
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
            stride_pdk::log("warn", &format!("lightbox left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled
        || slug_matches(&config.skip_pages, slug)
        || html.contains(MARKER)
        || find_ci(&html, "<img").is_none()
    {
        return html;
    }
    let (backdrop, ink, button) = match config.backdrop.as_str() {
        "light" => ("rgba(250,250,249,.96)", "#1c1917", "rgba(28,25,23,.08)"),
        _ => ("rgba(12,12,14,.95)", "#fafaf9", "rgba(255,255,255,.12)"),
    };
    let style = format!(
        "<style>.slb-zoom{{cursor:zoom-in}}\
         .slb-zoom:focus-visible{{outline:3px solid #2563eb;outline-offset:3px}}\
         #stride-lightbox{{border:0;padding:0;margin:0;width:100vw;height:100vh;max-width:none;max-height:none;\
         background:{backdrop};color:{ink};font:500 15px/1.5 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         #stride-lightbox::backdrop{{background:transparent}}\
         #stride-lightbox[open]{{display:grid;grid-template:1fr auto/1fr;place-items:center}}\
         #stride-lightbox figure{{margin:0;display:grid;gap:14px;justify-items:center;padding:64px 72px 24px;\
         max-width:100%;max-height:100%;box-sizing:border-box;min-height:0}}\
         #stride-lightbox img{{max-width:100%;max-height:calc(100vh - 150px);object-fit:contain;\
         border-radius:4px;box-shadow:0 20px 60px rgba(0,0,0,.35);cursor:zoom-out}}\
         #stride-lightbox figcaption{{max-width:60ch;text-align:center}}\
         #stride-lightbox figcaption:empty{{display:none}}\
         #stride-lightbox .slb-count{{font-size:13px;opacity:.7;padding-bottom:18px;font-variant-numeric:tabular-nums}}\
         #stride-lightbox button{{position:absolute;display:grid;place-items:center;width:48px;height:48px;border:0;\
         border-radius:999px;background:{button};color:inherit;cursor:pointer;font:400 26px/1 system-ui,sans-serif}}\
         #stride-lightbox button:hover{{filter:brightness(1.4)}}\
         #stride-lightbox button:focus-visible{{outline:2px solid currentColor;outline-offset:2px}}\
         #stride-lightbox .slb-close{{top:16px;right:16px}}\
         #stride-lightbox .slb-prev{{left:16px;top:50%;transform:translateY(-50%)}}\
         #stride-lightbox .slb-next{{right:16px;top:50%;transform:translateY(-50%)}}\
         #stride-lightbox button[hidden]{{display:none}}\
         @media (max-width:640px){{#stride-lightbox figure{{padding:64px 12px 16px}}\
         #stride-lightbox .slb-prev,#stride-lightbox .slb-next{{top:auto;bottom:10px;transform:none}}}}\
         @media (prefers-reduced-motion:no-preference){{#stride-lightbox[open] img{{animation:slb-in .18s ease-out}}}}\
         @keyframes slb-in{{from{{opacity:0;transform:scale(.97)}}}}</style>"
    );
    let dialog = format!(
        "<dialog {MARKER} aria-label=\"Image viewer\"><figure><img alt=\"\"><figcaption></figcaption></figure>\
         <div class=\"slb-count\" aria-live=\"polite\"></div>\
         <button type=\"button\" class=\"slb-close\" aria-label=\"Close\">&times;</button>\
         <button type=\"button\" class=\"slb-prev\" aria-label=\"Previous image\" hidden>&#8249;</button>\
         <button type=\"button\" class=\"slb-next\" aria-label=\"Next image\" hidden>&#8250;</button></dialog>{}",
        script(config)
    );
    let mut html = html;
    insert_in_head(&mut html, &style);
    insert_before_body_end(&mut html, &dialog);
    html
}

/// Runs once the page has loaded, because "is this image shown smaller than
/// it is" can only be answered once it has a size.
fn script(config: &Config) -> String {
    format!(
        "<script>(function(){{var d=document.getElementById('stride-lightbox');\
         if(!d||!d.showModal)return;var ALL={all},CAP={cap},ARROWS={arrows};\
         var big=d.querySelector('img'),cap=d.querySelector('figcaption'),count=d.querySelector('.slb-count'),\
         prev=d.querySelector('.slb-prev'),next=d.querySelector('.slb-next'),list=[],at=0,back=null;\
         function worth(i){{if(i.closest('a,button,dialog,header,nav,footer'))return false;\
         if(i.naturalWidth<80||i.naturalHeight<80)return false;\
         return ALL||i.naturalWidth>i.clientWidth*1.15||i.naturalHeight>i.clientHeight*1.15}}\
         function show(n){{at=(n+list.length)%list.length;var i=list[at];\
         big.src=i.currentSrc||i.src;big.alt=i.alt||'';cap.textContent=CAP?(i.alt||''):'';\
         var many=ARROWS&&list.length>1;prev.hidden=next.hidden=!many;\
         count.textContent=list.length>1?(at+1)+' / '+list.length:''}}\
         function open(i){{back=i;show(list.indexOf(i));d.showModal()}}\
         function setup(){{list=[].filter.call(document.querySelectorAll('main img,article img,body img'),worth);\
         list.forEach(function(i){{if(i.dataset.slb)return;i.dataset.slb='1';i.classList.add('slb-zoom');\
         i.tabIndex=0;i.setAttribute('role','button');\
         i.setAttribute('aria-label','Enlarge image'+(i.alt?': '+i.alt:''));\
         i.addEventListener('click',function(){{open(i)}});\
         i.addEventListener('keydown',function(e){{if(e.key==='Enter'||e.key===' '){{e.preventDefault();open(i)}}}})}})}}\
         prev.onclick=function(){{show(at-1)}};next.onclick=function(){{show(at+1)}};\
         d.querySelector('.slb-close').onclick=function(){{d.close()}};\
         big.onclick=function(){{d.close()}};\
         d.addEventListener('click',function(e){{if(e.target===d||e.target.tagName==='FIGURE')d.close()}});\
         d.addEventListener('keydown',function(e){{if(list.length<2)return;\
         if(e.key==='ArrowLeft')show(at-1);if(e.key==='ArrowRight')show(at+1)}});\
         d.addEventListener('close',function(){{big.removeAttribute('src');if(back)back.focus()}});\
         var x0=null;d.addEventListener('touchstart',function(e){{x0=e.touches[0].clientX}},{{passive:true}});\
         d.addEventListener('touchend',function(e){{if(x0===null||list.length<2)return;\
         var dx=e.changedTouches[0].clientX-x0;x0=null;if(Math.abs(dx)>50)show(at+(dx<0?1:-1))}});\
         if(document.readyState==='complete')setup();else addEventListener('load',setup)}})()</script>",
        all = config.which == "all",
        cap = config.captions,
        arrows = config.arrows,
    )
}

#[plugin_fn]
pub fn panel_image_lightbox(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
            Ok(Json(PanelResponse {
                values,
                message: if config.enabled {
                    "Saved. Publish your pages again to update them.".to_owned()
                } else {
                    "Saved. Images open as before once your pages are published again.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head><title>x</title></head><body><main><img src=\"a.jpg\" alt=\"A\"></main></body></html>";

    #[test]
    fn adds_style_in_head_and_dialog_before_body_end() {
        let out = render(PAGE.into(), "home", &Config::default());
        assert!(out.find("<style>.slb-zoom").unwrap() < out.find("</head>").unwrap());
        let dialog = out.find("<dialog id=\"stride-lightbox\"").unwrap();
        assert!(dialog > out.find("</main>").unwrap() && dialog < out.find("</body>").unwrap());
        assert!(out.contains("var ALL=false,CAP=true,ARROWS=true"));
        assert_eq!(render(out.clone(), "home", &Config::default()), out);
    }

    #[test]
    fn leaves_pages_without_images_and_skipped_pages_alone() {
        let plain = "<html><body><p>No pictures</p></body></html>";
        assert_eq!(render(plain.into(), "home", &Config::default()), plain);
        let config = Config { skip_pages: slug_list("gallery"), ..Config::default() };
        assert_eq!(render(PAGE.into(), "gallery", &config), PAGE);
        let off = Config { enabled: false, ..Config::default() };
        assert_eq!(render(PAGE.into(), "home", &off), PAGE);
    }

    #[test]
    fn settings_round_trip_and_unknown_values_fall_back() {
        let mut values = Map::new();
        values.insert("which".into(), "everything".into());
        values.insert("backdrop".into(), "light".into());
        let config = Config::from_values(&values);
        assert_eq!(config.which, "large");
        assert_eq!(config.backdrop, "light");
        assert_eq!(Config::from_values(&config.to_values()), config);
    }
}
