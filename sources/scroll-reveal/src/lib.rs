//! Scroll Reveal: a Stride plugin.
//!
//! Page content fades and slides into place as the visitor scrolls to it.
//! Modern browsers do it with a CSS scroll-driven animation
//! (`animation-timeline: view()`), so no code runs on scroll. Browsers without
//! it get the same look from a few hundred bytes of inline JavaScript built on
//! `IntersectionObserver`. Everything sits behind
//! `prefers-reduced-motion: no-preference`, and content is only ever hidden
//! by a script that is known to reveal it again, so a visitor without
//! JavaScript, a printer or a search engine always sees the whole page.
//!
//! It asks for `storage` only, to keep its settings. Without it the plugin
//! still works, with the defaults.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

const SETTINGS_KEY: &str = "settings";
/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-sr";

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
pub fn panel_scroll_reveal(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
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
                     It still reveals content with the defaults shown here."
                        .to_owned()
                }
                _ => String::new(),
            },
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let submitted = Settings::from_values(&request.values);
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
                "Saved. The effect is off. Publish the site again to remove it from pages \
                 that are already live."
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
pub enum Effect {
    FadeUp,
    Fade,
    SlideLeft,
    SlideRight,
    Zoom,
}

impl Effect {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "fade-up" => Some(Effect::FadeUp),
            "fade" => Some(Effect::Fade),
            "slide-left" => Some(Effect::SlideLeft),
            "slide-right" => Some(Effect::SlideRight),
            "zoom" => Some(Effect::Zoom),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Effect::FadeUp => "fade-up",
            Effect::Fade => "fade",
            Effect::SlideLeft => "slide-left",
            Effect::SlideRight => "slide-right",
            Effect::Zoom => "zoom",
        }
    }
    /// The transform content starts from. `none` for a plain fade.
    fn transform(self, distance: Distance) -> String {
        let px = distance.px();
        match self {
            Effect::FadeUp => format!("translateY({px}px)"),
            Effect::Fade => "none".to_owned(),
            // "From the left" means it starts on the left and moves right.
            Effect::SlideLeft => format!("translateX(-{px}px)"),
            Effect::SlideRight => format!("translateX({px}px)"),
            Effect::Zoom => format!("scale({})", distance.scale()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distance {
    Small,
    Medium,
    Large,
}

impl Distance {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "small" => Some(Distance::Small),
            "medium" => Some(Distance::Medium),
            "large" => Some(Distance::Large),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Distance::Small => "small",
            Distance::Medium => "medium",
            Distance::Large => "large",
        }
    }
    fn px(self) -> u32 {
        match self {
            Distance::Small => 16,
            Distance::Medium => 40,
            Distance::Large => 80,
        }
    }
    fn scale(self) -> &'static str {
        match self {
            Distance::Small => ".97",
            Distance::Medium => ".92",
            Distance::Large => ".85",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Targets {
    Blocks,
    Sections,
}

/// Elements whose insides never move on their own: the container moves as
/// one piece, so nothing is animated twice over.
const WHOLE: &str = "a,article,figure,blockquote,li,table,form,picture,nav,p,h1,h2,h3,h4,h5,h6,dl,pre";

impl Targets {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "blocks" => Some(Targets::Blocks),
            "sections" => Some(Targets::Sections),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Targets::Blocks => "blocks",
            Targets::Sections => "sections",
        }
    }
    /// A fixed selector: no setting text ever reaches CSS or the script.
    fn selector(self) -> String {
        match self {
            Targets::Blocks => format!(
                ":is(main :is(h1,h2,h3,h4,h5,h6,p,ul,ol,dl,table,pre,blockquote,figure,img,video,\
picture,article,form),main :is(div,section)>a):not(:is({WHOLE}) *)"
            ),
            // The section's content moves, its background band stays put.
            Targets::Sections => {
                ":is(main>:is(section,article)>*,main>:not(section,article,script,style,template))"
                    .to_owned()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub effect: Effect,
    pub distance: Distance,
    pub targets: Targets,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            effect: Effect::FadeUp,
            distance: Distance::Medium,
            targets: Targets::Blocks,
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
        Settings {
            enabled: value.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            effect: s("effect").and_then(Effect::parse).unwrap_or(d.effect),
            distance: s("distance").and_then(Distance::parse).unwrap_or(d.distance),
            targets: s("targets").and_then(Targets::parse).unwrap_or(d.targets),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        JsonValue::Object(self.to_values())
    }

    /// The panel's field names are the manifest's, and the stored keys are
    /// the same.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("effect".into(), self.effect.name().into());
        values.insert("distance".into(), self.distance.name().into());
        values.insert("targets".into(), self.targets.name().into());
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody chose in the panel. Every field is a fixed choice, so
    /// anything unknown simply falls back to its default.
    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        Settings::from_json(&JsonValue::Object(values.clone()))
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

// ------------------------------------------------------------ the plain work

/// The page with the effect added, or `None` to leave it alone: disabled,
/// excluded, already done, or not a whole document.
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
    let head_close = find_ci(bytes, b"</head>").filter(|&at| at < body);

    let selector = settings.targets.selector();
    let style = style(settings, &selector);
    let script = format!(
        "<script>{}{}{}</script>",
        SCRIPT_HEAD,
        // The selector is one of two constants and holds no quote or `<`.
        selector,
        SCRIPT_TAIL
    );

    let mut out = String::with_capacity(html.len() + style.len() + script.len());
    let style_at = head_close.unwrap_or(body);
    out.push_str(&html[..style_at]);
    out.push_str(&style);
    out.push_str(&html[style_at..body_close]);
    out.push_str(&script);
    out.push_str(&html[body_close..]);
    Some(out)
}

/// The fallback. It does nothing where CSS already does the work, where the
/// visitor prefers reduced motion, or where there is no IntersectionObserver.
/// Content already on screen when the page opens is left where it is; the
/// rest is hidden only once the observer is watching it. Whenever anything
/// comes into view, everything above it is revealed too, so a jump to the
/// end or to an anchor never leaves content it skipped over invisible.
const SCRIPT_HEAD: &str = "(function(){var w=window,c=w.CSS;\
if(c&&c.supports&&c.supports('animation-timeline','view()'))return;\
if(!('IntersectionObserver'in w)||matchMedia('(prefers-reduced-motion: reduce)').matches)return;\
var h=innerHeight,e=[],o=new IntersectionObserver(function(){\
e=e.filter(function(x){if(x.getBoundingClientRect().top<innerHeight*.92){\
x.classList.add('stride-sr-in');o.unobserve(x);return 0}return 1})},\
{rootMargin:'0px 0px -8% 0px'});document.querySelectorAll('";
const SCRIPT_TAIL: &str = "').forEach(function(x){\
if(x.getBoundingClientRect().top>=h){x.classList.add('stride-sr-o');e.push(x);o.observe(x)}})})();";

fn style(settings: &Settings, selector: &str) -> String {
    let from = settings.effect.transform(settings.distance);
    let ease = "cubic-bezier(.2,.7,.2,1)";
    // Content waiting off to the side must not widen the page on a phone.
    // `clip`, unlike `hidden`, makes no scroll container, so sticky headers
    // inside keep working.
    let clip = match settings.effect {
        Effect::SlideLeft | Effect::SlideRight => "main{overflow-x:clip}",
        _ => "",
    };
    format!(
        "<style id=\"{MARKER}\">@media screen and (prefers-reduced-motion:no-preference){{{clip}\
@supports (animation-timeline:view()){{{selector}{{animation:stride-sr ease-out both;\
animation-timeline:view();animation-range:entry 0% entry 100%}}}}\
.stride-sr-o{{transition:opacity .7s {ease},transform .7s {ease}}}\
.stride-sr-o:not(.stride-sr-in){{opacity:0;transform:{from}}}}}\
@keyframes stride-sr{{from{{opacity:0;transform:{from}}}}}</style>"
    )
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
<body class=\"x\" data-a='>'><main><section><h1>Hi</h1></section></main></body></html>";

    #[test]
    fn adds_style_and_script() {
        let html = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(html.contains("</style></head>"));
        assert!(html.contains("</script></body></html>"));
        assert!(html.contains("translateY(40px)"));
        assert!(html.contains("prefers-reduced-motion:no-preference"));
        assert!(html.contains("animation-timeline:view()"));
    }

    #[test]
    fn is_idempotent_and_respects_settings() {
        let once = build(PAGE, "guide", &Settings::default()).unwrap();
        assert!(build(&once, "guide", &Settings::default()).is_none());
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(build(PAGE, "guide", &off).is_none());
        let skip = Settings { exclude: vec!["guide".into()], ..Settings::default() };
        assert!(build(PAGE, "guide", &skip).is_none());
        let zoom = Settings {
            effect: Effect::Zoom,
            distance: Distance::Large,
            targets: Targets::Sections,
            ..Settings::default()
        };
        let html = build(PAGE, "guide", &zoom).unwrap();
        assert!(html.contains("scale(.85)"));
        assert!(!html.contains("overflow-x:clip"));
        let slide = Settings { effect: Effect::SlideRight, ..Settings::default() };
        assert!(build(PAGE, "guide", &slide).unwrap().contains("main{overflow-x:clip}"));
        assert!(html.contains(":is(main>:is(section,article)>*"));
        let fade = Settings { effect: Effect::Fade, ..Settings::default() };
        assert!(build(PAGE, "guide", &fade).unwrap().contains("transform:none"));
    }

    #[test]
    fn refuses_fragments() {
        assert!(build("<p>no body</p>", "x", &Settings::default()).is_none());
        assert!(build("<bodyguard></bodyguard>", "x", &Settings::default()).is_none());
    }

    #[test]
    fn selectors_are_safe_in_a_script_string() {
        for t in [Targets::Blocks, Targets::Sections] {
            let s = t.selector();
            assert!(!s.contains('\'') && !s.contains('<') && !s.contains('\\'));
        }
    }

    #[test]
    fn settings_round_trip_and_bad_input() {
        let s = Settings {
            enabled: true,
            effect: Effect::SlideLeft,
            distance: Distance::Small,
            targets: Targets::Sections,
            exclude: vec!["home".into(), "contact".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()), s);
        let mut bad = Map::new();
        bad.insert("effect".into(), "}</style><script>".into());
        assert_eq!(Settings::from_values(&bad), Settings::default());
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
    }
}
