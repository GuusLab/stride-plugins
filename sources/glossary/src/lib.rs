//! Glossary: explain your jargon where it appears. Terms listed in the site
//! settings get a dotted underline in the page text; pointing at one, tapping
//! it or tabbing to it shows its explanation.
//!
//! Only running text is touched: never headings, links, buttons, code,
//! scripts, the menu, header or footer, nor anything inside a tag. By default
//! only the first mention of each term on a page is marked, so a page does
//! not turn into a sea of underlines.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};
use util::*;

const FIELDS: [&str; 5] = ["enabled", "terms", "every", "colour", "skip-pages"];
const STYLE_ID: &str = "id=\"stride-glossary\"";
const MAX_TERMS: usize = 200;

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    terms: String,
    every: bool,
    colour: String,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { enabled: true, terms: String::new(), every: false, colour: "#6d28d9".into(), skip_pages: Vec::new() }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let colour = text(values, "colour").to_ascii_lowercase();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            terms: values.get("terms").and_then(JsonValue::as_str).unwrap_or_default().trim().to_owned(),
            every: flag(values, "every", d.every),
            colour: if is_colour(&colour) { colour } else { d.colour },
            skip_pages: slug_list(&text(values, "skip-pages")),
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("terms".into(), self.terms.clone().into());
        values.insert("every".into(), self.every.into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Term {
    /// Other spellings that mean the same, like "SEO" and "search engine optimisation".
    names: Vec<String>,
    definition: String,
}

/// `Term: definition`, one per line. `SEO | search engine optimisation:
/// definition` gives one entry several spellings. Err names the line that
/// has no colon.
fn parse_terms(text: &str) -> Result<Vec<Term>, String> {
    let mut terms = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((names, definition)) = line.split_once(':') else { return Err(line.chars().take(40).collect()) };
        let names: Vec<String> = names.split('|').map(|n| n.trim().to_owned()).filter(|n| n.chars().count() >= 2).collect();
        let definition = definition.trim().to_owned();
        if names.is_empty() || definition.is_empty() {
            return Err(line.chars().take(40).collect());
        }
        terms.push(Term { names, definition });
    }
    terms.truncate(MAX_TERMS);
    Ok(terms)
}

/// Elements whose text is never marked.
const SKIP: [&str; 22] = [
    "a", "h1", "h2", "h3", "h4", "h5", "h6", "button", "code", "pre", "kbd", "script", "style", "textarea", "select",
    "option", "abbr", "dialog", "nav", "header", "footer", "svg",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Wraps terms in the text parts of `html`, outside SKIP elements and within
/// `[from, to)`. Returns the new HTML and how many terms were marked.
fn mark_terms(html: &str, terms: &[Term], every: bool, from: usize, to: usize) -> (String, usize) {
    // Longest spelling first, so "search engine optimisation" wins over "search".
    let mut names: Vec<(String, usize)> = terms
        .iter()
        .enumerate()
        .flat_map(|(i, t)| t.names.iter().map(move |n| (escape(n).to_lowercase(), i)))
        .collect();
    names.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));

    let mut used = vec![false; terms.len()];
    let mut out = String::with_capacity(html.len() + 1024);
    let mut depth = 0usize;
    let mut marked = 0;
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < html.len() {
        if bytes[i] == b'<' {
            let end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
            let tag = &html[i..end];
            let lower = tag.to_ascii_lowercase();
            let closing = lower.starts_with("</");
            let name: String = lower.trim_start_matches(['<', '/']).chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if SKIP.contains(&name.as_str()) && !lower.ends_with("/>") {
                if closing {
                    depth = depth.saturating_sub(1);
                } else {
                    depth += 1;
                }
            }
            out.push_str(tag);
            i = end;
            continue;
        }
        let next = html[i..].find('<').map(|e| i + e).unwrap_or(html.len());
        let segment = &html[i..next];
        if depth > 0 || i < from || i >= to {
            out.push_str(segment);
        } else {
            let (text, n) = mark_segment(segment, &names, terms, &mut used, every);
            out.push_str(&text);
            marked += n;
        }
        i = next;
    }
    (out, marked)
}

fn mark_segment(segment: &str, names: &[(String, usize)], terms: &[Term], used: &mut [bool], every: bool) -> (String, usize) {
    let lower = segment.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; then leave this text alone.
    if lower.len() != segment.len() {
        return (segment.to_owned(), 0);
    }
    let mut out = String::with_capacity(segment.len());
    let mut marked = 0;
    let mut at = 0;
    'scan: while at < segment.len() {
        for (name, index) in names {
            if (!every && used[*index]) || !lower[at..].starts_with(name.as_str()) {
                continue;
            }
            let before_ok = segment[..at].chars().next_back().is_none_or(|c| !is_word_char(c));
            let end = at + name.len();
            let after_ok = segment[end..].chars().next().is_none_or(|c| !is_word_char(c));
            if !(before_ok && after_ok) {
                continue;
            }
            used[*index] = true;
            marked += 1;
            out.push_str(&format!(
                "<span class=\"sgl\"><button type=\"button\" class=\"sgl-term\" aria-expanded=\"false\">{}</button>\
                 <span class=\"sgl-tip\" role=\"tooltip\">{}</span></span>",
                &segment[at..end],
                escape(&terms[*index].definition)
            ));
            at = end;
            continue 'scan;
        }
        let c = segment[at..].chars().next().unwrap_or(' ');
        out.push(c);
        at += c.len_utf8();
    }
    (out, marked)
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
        Err(error) => {
            stride_pdk::log("warn", &format!("glossary left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &page.slug, &config) }))
}

fn render(html: String, slug: &str, config: &Config) -> String {
    if !config.enabled || slug_matches(&config.skip_pages, slug) || html.contains(STYLE_ID) {
        return html;
    }
    let Ok(terms) = parse_terms(&config.terms) else { return html };
    if terms.is_empty() {
        return html;
    }
    // The main content only; without a <main>, everything after <body>.
    let lower = html.to_ascii_lowercase();
    let (from, to) = match (lower.find("<main"), lower.rfind("</main>")) {
        (Some(a), Some(b)) if a < b => (a, b),
        _ => (lower.find("<body").unwrap_or(0), lower.rfind("</body>").unwrap_or(lower.len())),
    };
    let (mut out, marked) = mark_terms(&html, &terms, config.every, from, to);
    if marked == 0 {
        return html;
    }
    let style = format!(
        "<style {STYLE_ID}>.sgl{{position:relative;display:inline}}\
         .sgl-term{{all:unset;cursor:help;border-bottom:1.5px dotted {c};color:inherit;font:inherit}}\
         .sgl-term:focus-visible{{outline:2px solid {c};outline-offset:2px;border-radius:2px}}\
         .sgl-tip{{position:absolute;left:50%;bottom:calc(100% + 10px);transform:translateX(-50%);z-index:2147480000;\
         width:max-content;max-width:min(300px,80vw);padding:10px 12px;border-radius:10px;background:#111827;color:#f9fafb;\
         font:400 14px/1.45 system-ui,-apple-system,\"Segoe UI\",sans-serif;text-align:left;box-shadow:0 12px 30px rgba(0,0,0,.25);\
         visibility:hidden;opacity:0;transition:opacity .12s;pointer-events:none;white-space:normal}}\
         .sgl-tip::after{{content:\"\";position:absolute;left:50%;top:100%;margin-left:-6px;border:6px solid transparent;border-top-color:#111827}}\
         .sgl:hover .sgl-tip,.sgl-term:focus-visible+.sgl-tip,.sgl-term[aria-expanded=true]+.sgl-tip{{visibility:visible;opacity:1}}\
         .sgl-tip.sgl-below{{bottom:auto;top:calc(100% + 10px)}}.sgl-tip.sgl-below::after{{top:auto;bottom:100%;border-top-color:transparent;border-bottom-color:#111827}}\
         @media (prefers-reduced-motion:reduce){{.sgl-tip{{transition:none}}}}</style>",
        c = config.colour,
    );
    // Tap to open and close, Escape to close, and flip below when there is no room above.
    let script = "<script>(function(){var open=null;function set(b,v){b.setAttribute('aria-expanded',v);\
        var t=b.nextElementSibling;if(v){t.classList.toggle('sgl-below',b.getBoundingClientRect().top<t.offsetHeight+24)}open=v?b:null}\
        document.addEventListener('click',function(e){var b=e.target.closest&&e.target.closest('.sgl-term');\
        if(open&&open!==b)set(open,false);if(b)set(b,b.getAttribute('aria-expanded')!=='true')});\
        document.addEventListener('keydown',function(e){if(e.key==='Escape'&&open){var b=open;set(b,false);b.focus()}});\
        [].forEach.call(document.querySelectorAll('.sgl'),function(s){s.addEventListener('mouseenter',function(){var b=s.firstChild,t=b.nextElementSibling;\
        t.classList.toggle('sgl-below',b.getBoundingClientRect().top<t.offsetHeight+24)})})})()</script>";
    insert_in_head(&mut out, &style);
    insert_before_body_end(&mut out, script);
    out
}

#[plugin_fn]
pub fn panel_glossary(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            Ok(Json(PanelResponse { values, ..PanelResponse::default() }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let terms = match parse_terms(&config.terms) {
                Ok(terms) => terms,
                Err(line) => {
                    return Ok(Json(PanelResponse {
                        error: format!("\"{line}\" needs a term, a colon and an explanation, like: SEO: making your site easy to find in search engines."),
                        ..PanelResponse::default()
                    }));
                }
            };
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return Ok(Json(PanelResponse {
                        error: if error.is_permission_denied() {
                            "This plugin was not granted the storage permission, so it cannot keep your terms.".to_owned()
                        } else {
                            error.to_string()
                        },
                        ..PanelResponse::default()
                    }));
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: format!("Saved {} terms. Your pages show them right away.", terms.len()),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(terms: &str) -> Config {
        Config { terms: terms.into(), ..Config::default() }
    }

    const PAGE: &str = "<html><head></head><body><header>SEO agency</header><main><h2>What SEO is</h2>\
        <p>Good SEO starts with content. More SEO later. See <a href=\"/seo\">SEO guide</a>.</p>\
        <p class=\"seo\">Search engine optimisation and R&amp;D and seoul.</p><code>SEO</code></main><footer>SEO</footer></body></html>";

    #[test]
    fn marks_first_mention_in_running_text_only() {
        let out = render(PAGE.into(), "blog", &config("SEO | search engine optimisation: Making your site easy to find.\nR&D: Research."));
        assert_eq!(out.matches("class=\"sgl-term\"").count(), 2, "{out}");
        assert!(out.contains("<p>Good <span class=\"sgl\"><button type=\"button\" class=\"sgl-term\" aria-expanded=\"false\">SEO</button>"));
        assert!(out.contains("<h2>What SEO is</h2>") && out.contains("<a href=\"/seo\">SEO guide</a>") && out.contains("<code>SEO</code>"));
        assert!(out.contains("<header>SEO agency</header>") && out.contains("<footer>SEO</footer>"));
        assert!(out.contains(">R&amp;D</button>"), "escaped terms match escaped text");
        assert!(out.contains("seoul."), "whole words only");
        assert!(out.contains("<p class=\"seo\">"), "attributes are never touched");
        assert_eq!(render(out.clone(), "blog", &config("SEO: x")), out);
    }

    #[test]
    fn every_mention_when_asked_and_definitions_escaped() {
        let c = Config { every: true, ..config("SEO: <b>bold</b> & more") };
        let out = render(PAGE.into(), "blog", &c);
        assert_eq!(out.matches("class=\"sgl-term\"").count(), 2, "both mentions in running text, not the link");
        assert!(out.contains("&lt;b&gt;bold&lt;/b&gt; &amp; more"));
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_terms("# comment\n\nCMS: A system.").unwrap().len(), 1);
        assert!(parse_terms("no colon here").is_err());
        assert!(parse_terms("X: a one-letter term is refused").is_err());
    }

    #[test]
    fn nothing_to_do_changes_nothing() {
        assert_eq!(render(PAGE.into(), "blog", &config("")), PAGE);
        assert_eq!(render(PAGE.into(), "blog", &config("Kubernetes: containers")), PAGE);
    }
}
