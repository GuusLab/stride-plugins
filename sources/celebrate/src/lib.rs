//! Celebrate: a burst of confetti on the pages that deserve one — the thank
//! you page after an order, a sign-up or a booking — and on any link that
//! points to `#celebrate`.
//!
//! A canvas drawn for three seconds and then removed, a little over a
//! kilobyte of inline script. Visitors who ask their device for less motion
//! get no animation at all.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 6] = ["enabled", "pages", "palette", "colour", "amount", "shapes"];
const MARKER: &str = "id=\"stride-celebrate\"";

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    pages: Vec<String>,
    /// `party`, `gold`, `pastel` or `brand`.
    palette: String,
    colour: String,
    /// `some` or `lots`.
    amount: String,
    /// `confetti`, `hearts` or `stars`.
    shapes: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            pages: vec!["thanks".into(), "thank-you".into(), "bedankt".into()],
            palette: "party".into(),
            colour: "#e11d48".into(),
            amount: "some".into(),
            shapes: "confetti".into(),
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
        let colour = text(values, "colour").to_ascii_lowercase();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            pages: if values.contains_key("pages") { slug_list(&text(values, "pages")) } else { d.pages },
            palette: pick("palette", &["party", "gold", "pastel", "brand"], &d.palette),
            colour: if is_colour(&colour) { colour } else { d.colour },
            amount: pick("amount", &["some", "lots"], &d.amount),
            shapes: pick("shapes", &["confetti", "hearts", "stars"], &d.shapes),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("pages".into(), self.pages.join(", ").into());
        values.insert("palette".into(), self.palette.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("amount".into(), self.amount.clone().into());
        values.insert("shapes".into(), self.shapes.clone().into());
        values
    }

    fn colours(&self) -> Vec<String> {
        let list: &[&str] = match self.palette.as_str() {
            "gold" => &["#f5c542", "#e0a526", "#fff1c1", "#c88a12", "#fde68a"],
            "pastel" => &["#fbcfe8", "#bfdbfe", "#bbf7d0", "#fde68a", "#ddd6fe"],
            "brand" => &[],
            _ => &["#ef4444", "#f59e0b", "#10b981", "#3b82f6", "#8b5cf6", "#ec4899"],
        };
        if list.is_empty() {
            vec![self.colour.clone(), "#ffffff".into(), self.colour.clone(), "#111827".into()]
        } else {
            list.iter().map(|c| (*c).to_owned()).collect()
        }
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
            stride_pdk::log("warn", &format!("confetti left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    let on_load = slug_matches(&config.pages, slug);
    let has_trigger = html.contains("#celebrate\"");
    if !config.enabled || html.contains(MARKER) || !(on_load || has_trigger) {
        return html;
    }
    let colours = config.colours().iter().map(|c| format!("'{c}'")).collect::<Vec<_>>().join(",");
    let script = format!(
        "<script {MARKER}>(function(){{if(matchMedia('(prefers-reduced-motion:reduce)').matches)return;\
         var C=[{colours}],N={n},SHAPE='{shape}';\
         function burst(x,y){{var cv=document.createElement('canvas'),dpr=Math.min(devicePixelRatio||1,2),W=innerWidth,H=innerHeight;\
         cv.width=W*dpr;cv.height=H*dpr;cv.setAttribute('aria-hidden','true');\
         cv.style.cssText='position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:2147483600';\
         document.body.appendChild(cv);var g=cv.getContext('2d');g.scale(dpr,dpr);var ps=[];\
         for(var i=0;i<N;i++){{var a=-Math.PI/2+(Math.random()-.5)*Math.PI*1.15,v=9+Math.random()*12;\
         ps.push({{x:x,y:y,vx:Math.cos(a)*v,vy:Math.sin(a)*v,r:Math.random()*6.3,vr:(Math.random()-.5)*.35,\
         s:8+Math.random()*9,c:C[i%C.length],w:Math.random()*10}})}}\
         var t0=performance.now();function draw(t){{var e=t-t0;g.clearRect(0,0,W,H);\
         ps.forEach(function(p){{p.vy+=.28;p.vx*=.985;p.vy*=.985;p.x+=p.vx+Math.sin((e/180)+p.w);p.y+=p.vy;p.r+=p.vr;\
         g.save();g.translate(p.x,p.y);g.rotate(p.r);g.globalAlpha=Math.max(0,1-e/3200);g.fillStyle=p.c;\
         if(SHAPE==='hearts'){{g.font=p.s*1.6+'px system-ui';g.fillText('\\u2665',0,0)}}\
         else if(SHAPE==='stars'){{g.font=p.s*1.6+'px system-ui';g.fillText('\\u2605',0,0)}}\
         else g.fillRect(-p.s/2,-p.s/4,p.s,p.s/2*Math.abs(Math.cos(e/120+p.w)));g.restore()}});\
         if(e<3300)requestAnimationFrame(draw);else cv.remove()}}requestAnimationFrame(draw)}}\
         window.strideCelebrate=burst;\
         document.addEventListener('click',function(e){{var a=e.target.closest&&e.target.closest('a[href$=\"#celebrate\"]');\
         if(!a)return;e.preventDefault();var r=a.getBoundingClientRect();burst(r.left+r.width/2,r.top+r.height/2)}});\
         if({on_load}){{var go=function(){{burst(innerWidth/2,innerHeight*.35)}};\
         if(document.readyState==='complete')setTimeout(go,250);else addEventListener('load',function(){{setTimeout(go,250)}})}}}})()</script>",
        n = if config.amount == "lots" { 260 } else { 140 },
        shape = config.shapes,
    );
    let mut html = html;
    insert_before_body_end(&mut html, &script);
    html
}

#[plugin_fn]
pub fn panel_celebrate(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                message: "Saved. Your pages show the change right away. Add a link to #celebrate anywhere for confetti on click.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head></head><body><main><h1>Thanks!</h1></main></body></html>";

    #[test]
    fn bursts_on_listed_pages_only() {
        let out = render(PAGE.into(), "thank-you", &Config::default());
        assert!(out.contains("<script id=\"stride-celebrate\">") && out.contains("if(true)"));
        assert_eq!(render(out.clone(), "thank-you", &Config::default()), out);
        assert_eq!(render(PAGE.into(), "about", &Config::default()), PAGE);
    }

    #[test]
    fn a_celebrate_link_brings_the_script_without_a_burst_on_load() {
        let html = "<html><body><a href=\"#celebrate\">Party</a></body></html>";
        let out = render(html.into(), "about", &Config::default());
        assert!(out.contains("if(false)"));
    }

    #[test]
    fn brand_palette_uses_the_colour() {
        let c = Config { palette: "brand".into(), colour: "#0f766e".into(), ..Config::default() };
        assert!(render(PAGE.into(), "thanks", &c).contains("var C=['#0f766e','#ffffff','#0f766e','#111827']"));
    }
}
