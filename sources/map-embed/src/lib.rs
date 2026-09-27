//! Privacy Map: an OpenStreetMap of one place that loads only on request.
//!
//! Until a visitor clicks Show map, the page holds a placeholder drawn with
//! CSS: the place name, the address and a button. No iframe, image, script or
//! font comes from another site, so OpenStreetMap learns nothing about a
//! visitor who never asks for the map. The button is a real link to
//! openstreetmap.org, so without JavaScript it still leads to the map.
//!
//! The map goes where an editor types `[map]` (a paragraph holding only the
//! code is replaced whole), where an element carries `data-privacy-map`, or,
//! if a page is chosen in the settings, at the end of that page's content.
//!
//! Permissions: `storage`, for the settings. Without it nothing is shown:
//! there is no location to show.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};
use util::{escape, find_ci, insert_before_body_end, insert_in_head, is_colour, readable_on, rfind_ci};

const SETTINGS: &str = "settings";
const CODE: &str = "[map]";
const ATTRIBUTE: &str = "data-privacy-map";
const DONE: &str = "class=\"pm-style\"";
/// The width the bounding box is worked out for; the embed fits the box to
/// the frame, so a wider frame simply shows a little more around the place.
const FRAME_WIDTH: f64 = 720.0;

// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub name: String,
    pub address: String,
    pub location: String,
    pub zoom: i64,
    pub height: i64,
    pub page: String,
    pub remember: bool,
    pub colour: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            name: String::new(),
            address: String::new(),
            location: String::new(),
            zoom: 16,
            height: 360,
            page: String::new(),
            remember: false,
            colour: "#C2410C".to_owned(),
        }
    }
}

impl Settings {
    fn load() -> Result<Self, stride_pdk::HostError> {
        Ok(match kv::get::<Map<String, JsonValue>>(SETTINGS)? {
            Some(values) => Settings::from_values(&values),
            None => Settings::default(),
        })
    }

    /// Anything missing or malformed falls back to its default.
    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Settings::default();
        let text = |name: &str| values.get(name).and_then(JsonValue::as_str).map(|s| s.trim().to_owned());
        let flag = |name: &str, fallback: bool| values.get(name).and_then(JsonValue::as_bool).unwrap_or(fallback);
        let number = |name: &str, fallback: i64, min: i64, max: i64| {
            values
                .get(name)
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
                .map(|n| n.clamp(min, max))
                .unwrap_or(fallback)
        };
        let colour = text("colour").filter(|c| is_colour(c)).unwrap_or(d.colour);
        Settings {
            enabled: flag("enabled", d.enabled),
            name: text("name").unwrap_or(d.name),
            address: text("address").unwrap_or(d.address),
            location: text("location").unwrap_or(d.location),
            zoom: number("zoom", d.zoom, 3, 19),
            height: number("height", d.height, 200, 720),
            page: text("page").map(|p| p.trim_matches('/').to_ascii_lowercase()).unwrap_or(d.page),
            remember: flag("remember", d.remember),
            colour,
        }
    }

    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut v = Map::new();
        v.insert("enabled".into(), self.enabled.into());
        v.insert("name".into(), self.name.clone().into());
        v.insert("address".into(), self.address.clone().into());
        v.insert("location".into(), self.location.clone().into());
        v.insert("zoom".into(), self.zoom.into());
        v.insert("height".into(), self.height.into());
        v.insert("page".into(), self.page.clone().into());
        v.insert("remember".into(), self.remember.into());
        v.insert("colour".into(), self.colour.clone().into());
        v
    }
}

// ------------------------------------------------------------------ location

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub lat: f64,
    pub lon: f64,
}

fn number(text: &str) -> Option<f64> {
    let n: f64 = text.trim().parse().ok()?;
    n.is_finite().then_some(n)
}

fn place(lat: Option<f64>, lon: Option<f64>) -> Option<Place> {
    let (lat, lon) = (lat?, lon?);
    ((-85.0..=85.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some(Place { lat, lon })
}

/// The value of `key=` in a query string or fragment.
fn param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split(['?', '&', '#'])
        .find_map(|pair| pair.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
}

/// A place from coordinates ("52.3731, 4.8922" or "52.3731 4.8922"), a
/// `geo:` URI, or an openstreetmap.org link (a marker's `mlat`/`mlon`, or the
/// view in `#map=zoom/lat/lon`).
pub fn parse_location(text: &str) -> Option<Place> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("openstreetmap.org") || lower.starts_with("http") {
        if let Some(found) = place(param(text, "mlat").and_then(number), param(text, "mlon").and_then(number)) {
            return Some(found);
        }
        let view = param(text, "map")?;
        let mut parts = view.split('/').skip(1);
        let lat = parts.next().and_then(number);
        let lon = parts.next().and_then(|s| number(s.split(['&', '?']).next().unwrap_or(s)));
        return place(lat, lon);
    }
    let text = lower.strip_prefix("geo:").unwrap_or(&lower);
    let text = text.split([';', '?']).next().unwrap_or(text);
    let mut parts = text.split([',', ' ']).filter(|s| !s.is_empty());
    let lat = parts.next().and_then(number);
    let lon = parts.next().and_then(number);
    if parts.next().is_some() {
        return None;
    }
    place(lat, lon)
}

fn fixed(n: f64) -> String {
    format!("{n:.5}")
}

/// The embed address, with a bounding box that shows `zoom` in a frame of
/// `FRAME_WIDTH` by `height` pixels (256-pixel Web Mercator tiles).
pub fn embed_url(at: Place, zoom: i64, height: i64) -> String {
    let degrees_per_pixel = 360.0 / (256.0 * 2f64.powi(zoom as i32));
    let half_lon = degrees_per_pixel * FRAME_WIDTH / 2.0;
    let half_lat = degrees_per_pixel * at.lat.to_radians().cos() * height as f64 / 2.0;
    format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={}%2C{}%2C{}%2C{}&amp;layer=mapnik&amp;marker={}%2C{}",
        fixed(at.lon - half_lon),
        fixed(at.lat - half_lat),
        fixed(at.lon + half_lon),
        fixed(at.lat + half_lat),
        fixed(at.lat),
        fixed(at.lon),
    )
}

pub fn view_url(at: Place, zoom: i64) -> String {
    let (lat, lon) = (fixed(at.lat), fixed(at.lon));
    format!("https://www.openstreetmap.org/?mlat={lat}&amp;mlon={lon}#map={zoom}/{lat}/{lon}")
}

// ------------------------------------------------------------------ markup

const PIN: &str = "<svg class=\"pm-pin\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"M12 2a7 7 0 0 0-7 7c0 5.2 7 13 7 13s7-7.8 7-13a7 7 0 0 0-7-7zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5z\"/></svg>";

fn card(s: &Settings, at: Place) -> String {
    let title = if s.name.is_empty() { "Map".to_owned() } else { format!("Map of {}", s.name) };
    let name = if s.name.is_empty() {
        String::new()
    } else {
        format!("<p class=\"pm-name\">{}</p>", escape(&s.name))
    };
    let lines: Vec<String> = s.address.lines().map(str::trim).filter(|l| !l.is_empty()).map(escape).collect();
    let address = if lines.is_empty() {
        String::new()
    } else {
        format!("<address class=\"pm-addr\">{}</address>", lines.join("<br>"))
    };
    let view = view_url(at, s.zoom);
    format!(
        "<section class=\"pm\" aria-label=\"{label}\" data-pm-src=\"{src}\" data-pm-title=\"{label}\">\
<div class=\"pm-box\"><div class=\"pm-ph\">{PIN}<div class=\"pm-card\">{name}{address}\
<a class=\"pm-show\" href=\"{view}\" target=\"_blank\" rel=\"noopener noreferrer\">Show map</a>\
<p class=\"pm-note\">The map loads from OpenStreetMap, which then sees your IP address.</p></div></div></div>\
<p class=\"pm-foot\"><a href=\"{view}\" target=\"_blank\" rel=\"noopener noreferrer\">Open in OpenStreetMap</a></p></section>",
        label = escape(&title),
        src = embed_url(at, s.zoom, s.height),
    )
}

fn style(s: &Settings) -> String {
    let fg = readable_on(&s.colour);
    format!(
        "<style {DONE}>\
.pm{{--pm:{c};--pm-fg:{fg};margin:2rem 0;font:inherit}}\
.pm-wrap{{box-sizing:border-box;max-width:70rem;margin:0 auto;padding:1rem 1.25rem}}\
.pm-box{{position:relative;height:{h}px;border-radius:14px;overflow:hidden;border:1px solid rgba(0,0,0,.12);background:#e9ede3}}\
.pm-ph{{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;padding:1rem;\
background:radial-gradient(circle at 50% 42%,transparent 0 30%,rgba(233,237,227,.85) 75%),\
linear-gradient(0deg,transparent calc(58% - 5px),#fff calc(58% - 5px) calc(58% + 5px),transparent calc(58% + 5px)),\
linear-gradient(90deg,transparent calc(36% - 5px),#fff calc(36% - 5px) calc(36% + 5px),transparent calc(36% + 5px)),\
repeating-linear-gradient(28deg,transparent 0 52px,#fbfbf8 52px 56px),\
repeating-linear-gradient(-62deg,transparent 0 70px,#fbfbf8 70px 74px),\
linear-gradient(135deg,#dfe8d5,#eceee6 55%,#dbe5ee)}}\
.pm-pin{{position:absolute;left:50%;top:18%;width:44px;height:44px;margin-left:-22px;fill:var(--pm);filter:drop-shadow(0 3px 3px rgba(0,0,0,.25))}}\
.pm-card{{position:relative;margin-top:3.5rem;max-width:22rem;padding:1.1rem 1.25rem;border-radius:12px;background:#fff;color:#1f2937;text-align:center;box-shadow:0 8px 28px rgba(0,0,0,.14)}}\
.pm-name{{margin:0 0 .25rem;font-weight:700;font-size:1.05rem}}\
.pm-addr{{font-style:normal;line-height:1.45;color:#4b5563}}\
.pm-show{{display:inline-block;margin-top:.85rem;padding:.6rem 1.3rem;border-radius:999px;background:var(--pm);color:var(--pm-fg);font-weight:600;text-decoration:none;line-height:1.2}}\
.pm-show:hover{{filter:brightness(.92)}}\
.pm-show:focus-visible,.pm-foot a:focus-visible{{outline:3px solid var(--pm);outline-offset:3px}}\
.pm-note{{margin:.7rem 0 0;font-size:.78rem;line-height:1.4;color:#6b7280}}\
.pm-frame{{display:block;width:100%;height:100%;border:0}}\
.pm-foot{{margin:.45rem 0 0;font-size:.85rem;text-align:right}}.pm-foot a{{color:inherit;opacity:.8}}\
@media print{{.pm-show,.pm-note{{display:none}}}}\
</style>",
        c = s.colour,
        h = s.height,
    )
}

fn script(remember: bool) -> String {
    format!(
        "<script>(function(){{var K='stride-privacy-map',r={remember},ok=0;\
if(r)try{{ok=localStorage.getItem(K)==='1'}}catch(e){{}}\
function show(c,f){{var s=c.getAttribute('data-pm-src'),p=c.querySelector('.pm-ph');if(!s||!p)return;\
var i=document.createElement('iframe');i.className='pm-frame';i.src=s.replace(/&amp;/g,'&');\
i.title=c.getAttribute('data-pm-title')||'Map';i.setAttribute('referrerpolicy','no-referrer');\
c.removeAttribute('data-pm-src');p.replaceWith(i);if(f)i.focus()}}\
document.querySelectorAll('.pm[data-pm-src]').forEach(function(c){{if(ok)return show(c);\
var b=c.querySelector('.pm-show');if(!b)return;b.setAttribute('role','button');\
b.addEventListener('click',function(e){{e.preventDefault();if(r)try{{localStorage.setItem(K,'1')}}catch(x){{}}show(c,1)}});\
b.addEventListener('keydown',function(e){{if(e.key===' '){{e.preventDefault();b.click()}}}})}})}})();</script>"
    )
}

// ------------------------------------------------------------------ placing

/// Replace `[map]`: a paragraph holding only the code is replaced whole,
/// because a section inside `<p>` is not valid HTML.
fn replace_code(html: &str, block: &str) -> (String, bool) {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut found = false;
    while let Some(at) = rest.find(CODE) {
        found = true;
        let before = &rest[..at];
        let after = &rest[at + CODE.len()..];
        let whole = before
            .rfind("<p")
            .filter(|&p| matches!(before.as_bytes().get(p + 2), Some(b'>' | b' ')))
            .filter(|&p| before[p..].find('>').is_some_and(|g| before[p + g + 1..].trim().is_empty()))
            .filter(|_| after.trim_start().starts_with("</p>"));
        match whole {
            Some(p) => {
                out.push_str(&before[..p]);
                out.push_str(block);
                let close = after.find("</p>").map_or(0, |c| c + 4);
                rest = &after[close..];
            }
            None => {
                out.push_str(before);
                out.push_str(block);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, found)
}

/// Replace every element carrying `data-privacy-map` (a marker, expected
/// empty) with the map.
fn replace_markers(html: &str, block: &str) -> (String, bool) {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut found = false;
    while let Some(attr) = rest.find(ATTRIBUTE) {
        let Some(open) = rest[..attr].rfind('<') else { break };
        let Some(gt) = rest[attr..].find('>').map(|g| attr + g) else { break };
        let tag: String = rest[open + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if tag.is_empty() || rest[open..attr].contains('>') {
            out.push_str(&rest[..attr + ATTRIBUTE.len()]);
            rest = &rest[attr + ATTRIBUTE.len()..];
            continue;
        }
        let close = format!("</{}>", tag.to_ascii_lowercase());
        let end = if rest[..gt].ends_with('/') {
            gt + 1
        } else {
            match find_ci(&rest[gt..], &close) {
                Some(c) => gt + c + close.len(),
                None => gt + 1,
            }
        };
        found = true;
        out.push_str(&rest[..open]);
        out.push_str(block);
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out, found)
}

/// The end of the page's main content: before `</main>`, else before the
/// last `<footer`, else before `</body>`.
fn append(html: &mut String, block: &str) {
    if let Some(at) = rfind_ci(html, "</main>") {
        html.insert_str(at, block);
    } else if let Some(at) = rfind_ci(html, "<footer") {
        html.insert_str(at, block);
    } else {
        insert_before_body_end(html, block);
    }
}

/// Split at the opening `<body …>` so the head is never touched.
fn body_start(html: &str) -> usize {
    find_ci(html, "<body")
        .and_then(|at| html[at..].find('>').map(|g| at + g + 1))
        .unwrap_or(0)
}

pub fn render(html: &str, slug: &str, s: &Settings) -> Option<String> {
    if !s.enabled || html.contains(DONE) {
        return None;
    }
    let chosen = !s.page.is_empty() && s.page == slug.trim_matches('/').to_ascii_lowercase();
    let start = body_start(html);
    let (head, body) = html.split_at(start);
    let has_marker = body.contains(CODE) || body.contains(ATTRIBUTE);
    if !has_marker && !chosen {
        return None;
    }
    // Without a location there is nothing to show; remove the codes so
    // visitors do not see them, and leave everything else alone.
    let block = match parse_location(&s.location) {
        Some(at) => card(s, at),
        None if has_marker => String::new(),
        None => return None,
    };
    let (body, a) = replace_code(body, &block);
    let (mut body, b) = replace_markers(&body, &block);
    if chosen && !a && !b {
        append(&mut body, &format!("<div class=\"pm-wrap\">{block}</div>"));
    }
    let mut out = format!("{head}{body}");
    if !block.is_empty() {
        insert_in_head(&mut out, &style(s));
        insert_before_body_end(&mut out, &script(s.remember));
    }
    Some(out)
}

// ------------------------------------------------------------------ exports

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(error) => {
            stride_pdk::log("warn", &format!("no map shown: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    let html = render(&page.html, &page.slug, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

#[plugin_fn]
pub fn panel_map_embed(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let refuse = |values: Map<String, JsonValue>, error: &str| {
        Ok(Json(PanelResponse { values, message: String::new(), error: error.to_owned() }))
    };
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: Settings::load().unwrap_or_default().to_values(),
            ..PanelResponse::default()
        })),
        PanelEvent::Submit => {
            let values = request.values.clone();
            let location = util::text(&values, "location");
            if !location.is_empty() && parse_location(&location).is_none() {
                return refuse(
                    values,
                    "The location is not a place on the map. Paste a link from openstreetmap.org, or coordinates like 52.3731, 4.8922. Nothing was saved.",
                );
            }
            let colour = util::text(&values, "colour");
            if !colour.is_empty() && !is_colour(&colour) {
                return refuse(values, "The button colour has to be a hex colour like #C2410C. Nothing was saved.");
            }
            let settings = Settings::from_values(&values);
            if let Err(error) = kv::set(SETTINGS, &settings.to_values()) {
                let message = if error.is_permission_denied() {
                    "This plugin was not granted the storage permission, so there is nowhere to keep the map settings.".to_owned()
                } else {
                    format!("The settings could not be saved: {error}")
                };
                return refuse(values, &message);
            }
            let message = if settings.location.is_empty() {
                "Saved. Add a location to show the map."
            } else if settings.page.is_empty() {
                "Saved. Type [map] on a page, then publish it again."
            } else {
                "Saved. Publish the page again to see the map."
            };
            Ok(Json(PanelResponse { values: settings.to_values(), message: message.to_owned(), error: String::new() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> Settings {
        Settings {
            name: "Bakery <Oven>".into(),
            address: "Prinsengracht 1\n1015 DK Amsterdam".into(),
            location: "52.3731, 4.8922".into(),
            ..Settings::default()
        }
    }

    #[test]
    fn locations() {
        let p = parse_location("52.3731, 4.8922").unwrap();
        assert!((p.lat - 52.3731).abs() < 1e-9 && (p.lon - 4.8922).abs() < 1e-9);
        assert!(parse_location("52.3731 4.8922").is_some());
        assert!(parse_location("geo:52.3731,4.8922").is_some());
        let p = parse_location("https://www.openstreetmap.org/#map=17/52.37310/4.89220").unwrap();
        assert!((p.lon - 4.8922).abs() < 1e-9);
        let p = parse_location("https://www.openstreetmap.org/?mlat=51.5&mlon=-0.12#map=15/51.4/-0.1").unwrap();
        assert!((p.lat - 51.5).abs() < 1e-9);
        assert!(parse_location("Amsterdam").is_none());
        assert!(parse_location("95, 4").is_none());
        assert!(parse_location("1, 2, 3").is_none());
    }

    #[test]
    fn paragraph_code_is_replaced_whole_and_escaped() {
        let html = "<html><head></head><body><main><p>[map]</p></main></body></html>";
        let out = render(html, "contact", &configured()).unwrap();
        assert!(!out.contains("[map]"));
        assert!(!out.contains("<p><section"));
        assert!(out.contains("Bakery &lt;Oven&gt;"));
        assert!(out.contains("Prinsengracht 1<br>1015 DK Amsterdam"));
        assert!(!out.contains("<iframe"), "no third-party frame before a click");
        assert!(out.contains(DONE));
        assert!(render(&out, "contact", &configured()).is_none());
    }

    #[test]
    fn marker_element_and_chosen_page() {
        let html = "<body><main><h1>Visit</h1><div data-privacy-map></div><p>After</p></main></body>";
        let out = render(html, "about", &configured()).unwrap();
        assert!(!out.contains(ATTRIBUTE) && out.contains("<p>After</p>"));
        let s = Settings { page: "contact".into(), ..configured() };
        let out = render("<body><main><h1>Hi</h1></main><footer>f</footer></body>", "contact", &s).unwrap();
        assert!(out.find("class=\"pm\"").unwrap() < out.find("</main>").unwrap());
        assert!(render("<body><main></main></body>", "other", &s).is_none());
    }

    #[test]
    fn without_location_codes_are_removed() {
        let s = Settings { location: String::new(), ..configured() };
        let out = render("<body><p>[map]</p></body>", "x", &s).unwrap();
        assert_eq!(out, "<body></body>");
        assert!(render("<body><p>hi</p></body>", "x", &s).is_none());
    }

    #[test]
    fn head_is_left_alone() {
        let html = "<head><meta name=\"description\" content=\"[map]\"></head><body><p>[map]</p></body>";
        let out = render(html, "x", &configured()).unwrap();
        assert!(out.contains("content=\"[map]\""));
    }
}
