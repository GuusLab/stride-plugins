//! Site Search: a search box for the whole site, opened with a button or
//! ⌘K / Ctrl+K / "/", that finds pages by title and text as you type.
//!
//! The index travels with the page. Every published page is in it by its
//! title (from `documents.list`), always up to date. A page's text joins the
//! first time the page is viewed: the hook stores the title and the start of
//! its visible text, from `<main>` when there is one, without header, menu
//! and footer.
//!
//! Only text every visitor may see is stored. Stride renders members-only
//! blocks for signed-in members through this same hook, so a page that can
//! differ per visitor (an access rule in the page or in any component it
//! places) is searchable by its title only. The index is rebuilt only when
//! something changed, so a page view costs a handful of storage reads.

mod util;

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv, serde_json};
use util::*;

const FIELDS: [&str; 7] = ["enabled", "placement", "language", "colour", "text-length", "shortcut", "skip-pages"];
const MARKER: &str = "id=\"stride-search\"";
/// Each page is kept under its own storage key, so pages rendered at the same
/// time (publishing the whole site renders them in parallel) never overwrite
/// each other's entry. Storage holds 256 keys per site; a few are settings.
const PAGE_PREFIX: &str = "page:";
const GENERATION: &str = "generation";
const INDEX_CACHE: &str = "index-cache";
const MAX_PAGES: usize = 240;
/// What one page may contribute, and what the whole embedded index may weigh.
const MAX_TITLE: usize = 120;
/// Also what the ready-built index may weigh in storage (64 KiB a value).
const MAX_INLINE: usize = 56 * 1024;

#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    /// `header` (in the page header when there is one) or `floating`.
    placement: String,
    language: String,
    colour: String,
    /// Characters of each page's text kept for searching.
    text_length: usize,
    shortcut: bool,
    skip_pages: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            placement: "header".into(),
            language: "auto".into(),
            colour: "#2563eb".into(),
            text_length: 600,
            shortcut: true,
            skip_pages: vec!["404".into()],
        }
    }
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Config::default();
        let language = text(values, "language");
        let colour = text(values, "colour").to_ascii_lowercase();
        Config {
            enabled: flag(values, "enabled", d.enabled),
            placement: if text(values, "placement") == "floating" { "floating".into() } else { d.placement },
            language: if ["auto", "en", "nl", "de", "fr"].contains(&language.as_str()) { language } else { d.language },
            colour: if is_colour(&colour) { colour } else { d.colour },
            text_length: values
                .get("text-length")
                .and_then(JsonValue::as_i64)
                .map(|n| n.clamp(100, 2000) as usize)
                .unwrap_or(d.text_length),
            shortcut: flag(values, "shortcut", d.shortcut),
            skip_pages: if values.contains_key("skip-pages") { slug_list(&text(values, "skip-pages")) } else { d.skip_pages },
        }
    }

    fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("placement".into(), self.placement.clone().into());
        values.insert("language".into(), self.language.clone().into());
        values.insert("colour".into(), self.colour.clone().into());
        values.insert("text-length".into(), (self.text_length as i64).into());
        values.insert("shortcut".into(), self.shortcut.into());
        values.insert("skip-pages".into(), self.skip_pages.join(", ").into());
        values
    }
}

/// The storage key for a page. Keys may be 128 bytes; a longer address is
/// hashed, and the entry itself carries the slug.
fn page_key(slug: &str) -> String {
    if slug.len() <= 100 { format!("{PAGE_PREFIX}{slug}") } else { format!("{PAGE_PREFIX}#{:08x}", fnv(slug)) }
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

/// This page's title and searchable text, from its public HTML.
fn entry_for(html: &str, text_length: usize) -> (String, String) {
    // The page's own title from <head> (an SVG in the body can carry a
    // <title> too), then its first heading.
    let title = inner_of(html, "head")
        .and_then(|head| inner_of(head, "title"))
        .map(text_of)
        .filter(|t| !t.is_empty())
        .or_else(|| inner_of(html, "h1").map(text_of))
        .unwrap_or_default();
    let body = inner_of(html, "main").or_else(|| inner_of(html, "body")).unwrap_or(html);
    let body = without_elements(
        body,
        &["script", "style", "noscript", "template", "svg", "nav", "header", "footer", "dialog", "form", "button", "h1"],
    );
    let text: String = text_of(&body).chars().take(text_length).collect();
    (title.chars().take(MAX_TITLE).collect(), text)
}

/// A published page or post, from `documents.list`.
struct Doc {
    id: String,
    slug: String,
    title: String,
    version: i64,
}

/// Published documents, when `read-pages` is granted.
fn published(site_id: &str) -> Option<Vec<Doc>> {
    let out: JsonValue = stride_pdk::action("documents.list", &serde_json::json!({ "siteId": site_id })).ok()?;
    Some(
        out.get("documents")?
            .as_array()?
            .iter()
            .filter(|d| d.get("status").and_then(JsonValue::as_str) == Some("published"))
            .filter_map(|d| {
                Some(Doc {
                    id: d.get("id")?.as_str()?.to_owned(),
                    slug: d.get("slug")?.as_str()?.to_owned(),
                    title: d.get("title").and_then(JsonValue::as_str).unwrap_or_default().to_owned(),
                    version: d.get("version").and_then(JsonValue::as_i64).unwrap_or(0),
                })
            })
            .collect(),
    )
}

/// Every node of a tree, as JSON.
fn walk<'a>(node: &'a JsonValue, out: &mut Vec<&'a JsonValue>) {
    out.push(node);
    if let Some(children) = node.get("children").and_then(JsonValue::as_array) {
        for child in children {
            walk(child, out);
        }
    }
}

/// Whether a tree shows everyone the same thing: no node carries an access
/// rule other than `public`. Component instances it places are returned so
/// the caller can check their masters too, which is where Stride itself
/// says a page's rules may live.
fn tree_is_public(root: &JsonValue, instances: &mut Vec<String>) -> bool {
    let mut nodes = Vec::new();
    walk(root, &mut nodes);
    let mut public = true;
    for node in nodes {
        if let Some(access) = node.get("access").filter(|a| !a.is_null()) {
            if access.get("type").and_then(JsonValue::as_str) != Some("public") {
                public = false;
            }
        }
        let kind = node.get("kind");
        if kind.and_then(|k| k.get("type")).and_then(JsonValue::as_str) == Some("instance") {
            match kind.and_then(|k| k.get("component")).and_then(JsonValue::as_str) {
                Some(id) => instances.push(id.to_owned()),
                None => public = false,
            }
        }
    }
    public
}

/// Whether every visitor sees the same page, so its text may be searched by
/// anyone. Members-only blocks are rendered for members through this same
/// hook, so a page that can differ per visitor gives its title only. The
/// answer is kept per page and worked out again when the page or any
/// component changes; anything that cannot be checked counts as "differs".
fn is_public_page(site_id: &str, doc: &Doc) -> bool {
    let components: Option<Vec<(String, i64)>> = stride_pdk::action::<_, JsonValue>(
        "components.list",
        &serde_json::json!({ "siteId": site_id }),
    )
    .ok()
    .and_then(|out| {
        Some(
            out.get("components")?
                .as_array()?
                .iter()
                .filter_map(|c| Some((c.get("id")?.as_str()?.to_owned(), c.get("version").and_then(JsonValue::as_i64).unwrap_or(0))))
                .collect(),
        )
    });
    let Some(components) = components else { return false };
    let fingerprint = format!(
        "{}:{:08x}",
        doc.version,
        fnv(&components.iter().map(|(id, v)| format!("{id}={v}")).collect::<Vec<_>>().join(","))
    );
    let cache_key = format!("public:{:08x}", fnv(&doc.slug));
    if let Ok(Some(cached)) = kv::get::<JsonValue>(&cache_key) {
        if cached.get(0).and_then(JsonValue::as_str) == Some(fingerprint.as_str()) {
            return cached.get(1).and_then(JsonValue::as_bool).unwrap_or(false);
        }
    }

    let answer = (|| {
        let out: JsonValue = stride_pdk::action("documents.get", &serde_json::json!({ "siteId": site_id, "documentId": doc.id })).ok()?;
        let mut pending = Vec::new();
        if !tree_is_public(out.get("document")?.get("root")?, &mut pending) {
            return Some(false);
        }
        let mut seen: Vec<String> = Vec::new();
        while let Some(id) = pending.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id.clone());
            if seen.len() > 64 {
                return Some(false);
            }
            let out: JsonValue = stride_pdk::action("components.get", &serde_json::json!({ "siteId": site_id, "componentId": id })).ok()?;
            if !tree_is_public(out.get("component")?.get("root")?, &mut pending) {
                return Some(false);
            }
        }
        Some(true)
    })()
    .unwrap_or(false);
    let _ = kv::set(&cache_key, &JsonValue::Array(vec![fingerprint.into(), answer.into()]));
    answer
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let config = match load_values() {
        Ok(values) => Config::from_values(&values),
        Err(error) => {
            // Without storage there is no index to share between pages.
            stride_pdk::log("warn", &format!("search left out: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    if !config.enabled || page.html.contains(MARKER) {
        return Ok(Json(PageRendered { html: page.html }));
    }

    let docs = published(&page.site_id);
    let this = docs.as_ref().and_then(|d| d.iter().find(|d| d.slug == page.slug));

    // 1. This page's own entry: its title always, its text only when every
    //    visitor sees the same page. Written only when it changed.
    let key = page_key(&page.slug);
    let entry = if slug_matches(&config.skip_pages, &page.slug) {
        None
    } else {
        let (title, text) = entry_for(&page.html, config.text_length);
        let public = this.is_some_and(|doc| is_public_page(&page.site_id, doc));
        let title = if title.is_empty() { this.map(|d| d.title.clone()).unwrap_or_default() } else { title };
        let text = if public { text } else { String::new() };
        (!title.is_empty() || !text.is_empty()).then(|| JsonValue::Array(vec![page.slug.clone().into(), title.into(), text.into()]))
    };
    let mut generation = kv::get::<String>(GENERATION).ok().flatten().unwrap_or_default();
    let current = kv::get::<JsonValue>(&key).ok().flatten();
    if entry != current {
        let written = match &entry {
            Some(value) => kv::set(&key, value),
            None => kv::delete(&key).map(|_| ()),
        };
        match written {
            Ok(()) => generation = bump(&generation, &key),
            Err(error) => stride_pdk::log("warn", &format!("could not store this page in the index: {error}")),
        }
    }

    // 2. The index, rebuilt only when an entry, the list of published pages
    //    or the settings changed since it was last built.
    let fingerprint = format!(
        "{generation}|{:08x}|{}|{}",
        fnv(&docs
            .as_ref()
            .map(|d| d.iter().map(|d| format!("{}={}={}", d.slug, d.version, d.title)).collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|| "-".into())),
        config.text_length,
        config.skip_pages.join(",")
    );
    let cached = kv::get::<JsonValue>(INDEX_CACHE).ok().flatten().and_then(|c| {
        (c.get(0)?.as_str()? == fingerprint).then(|| c.get(1)?.as_str().map(str::to_owned)).flatten()
    });
    let index = match cached {
        Some(index) => index,
        None => {
            let index = build_index(docs.as_deref(), &config, &mut generation);
            let fingerprint = fingerprint.replacen(fingerprint.split('|').next().unwrap_or_default(), &generation, 1);
            let _ = kv::set(INDEX_CACHE, &JsonValue::Array(vec![fingerprint.into(), index.clone().into()]));
            index
        }
    };
    Ok(Json(PageRendered { html: render(page.html, &config, &index) }))
}

/// A new value for the generation counter. Two pages writing at once may
/// both bump from the same value; either result differs from the old one,
/// which is all the cache needs to notice.
fn bump(generation: &str, key: &str) -> String {
    let next = format!("{:08x}", fnv(&format!("{generation}/{key}/{}", generation.len())));
    let _ = kv::set(GENERATION, &next);
    next
}

/// Every published page by its title, with the text of the pages that have
/// stored theirs. Without `read-pages`, only pages that have been visited,
/// by title. Drops entries of pages that are no longer published.
fn build_index(docs: Option<&[Doc]>, config: &Config, generation: &mut String) -> String {
    let mut entries: Vec<(String, String, String)> = Vec::new();
    for k in kv::list(PAGE_PREFIX).unwrap_or_default().iter().take(MAX_PAGES) {
        let Some(value) = kv::get::<JsonValue>(k).ok().flatten() else { continue };
        let Some(a) = value.as_array() else { continue };
        let Some(slug) = a.first().and_then(JsonValue::as_str) else { continue };
        if docs.is_some_and(|d| !d.iter().any(|d| d.slug == slug)) {
            if kv::delete(k).is_ok() {
                *generation = bump(generation, k);
            }
            continue;
        }
        let title = a.get(1).and_then(JsonValue::as_str).unwrap_or_default();
        let text = if docs.is_some() { a.get(2).and_then(JsonValue::as_str).unwrap_or_default() } else { "" };
        entries.push((slug.to_owned(), title.to_owned(), text.to_owned()));
    }
    for doc in docs.unwrap_or_default() {
        if !entries.iter().any(|e| e.0 == doc.slug) && !doc.title.is_empty() {
            entries.push((doc.slug.clone(), doc.title.clone(), String::new()));
        }
    }
    entries.retain(|e| !slug_matches(&config.skip_pages, &e.0));
    index_json(entries)

}

struct Words {
    search: &'static str,
    placeholder: &'static str,
    none: &'static str,
    hint: &'static str,
}

fn words(language: &str) -> Words {
    match language {
        "nl" => Words { search: "Zoeken", placeholder: "Zoek op deze site", none: "Niets gevonden voor", hint: "om te openen" },
        "de" => Words { search: "Suchen", placeholder: "Diese Website durchsuchen", none: "Nichts gefunden für", hint: "zum Öffnen" },
        "fr" => Words { search: "Rechercher", placeholder: "Rechercher sur ce site", none: "Aucun résultat pour", hint: "pour ouvrir" },
        _ => Words { search: "Search", placeholder: "Search this site", none: "Nothing found for", hint: "to open" },
    }
}

/// The index as a JSON array, sorted by slug, trimmed to the inline budget
/// by shortening texts before dropping anything.
fn index_json(mut entries: Vec<(String, String, String)>) -> String {
    entries.sort();
    let mut limit = usize::MAX;
    loop {
        let json = format!(
            "[{}]",
            entries
                .iter()
                .map(|(slug, title, text)| {
                    let text: String = text.chars().take(limit).collect();
                    format!("[{},{},{}]", json_string(slug), json_string(title), json_string(&text))
                })
                .collect::<Vec<_>>()
                .join(",")
        );
        if json.len() <= MAX_INLINE || limit == 0 {
            return json;
        }
        limit = if limit == usize::MAX { 400 } else { limit / 2 };
    }
}

const ICON: &str = "<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" \
stroke-width=\"2\" stroke-linecap=\"round\"><circle cx=\"11\" cy=\"11\" r=\"7\"/><path d=\"m20 20-3.5-3.5\"/></svg>";

fn render(html: String, config: &Config, index: &str) -> String {
    if index == "[]" || html.contains(MARKER) {
        return html;
    }
    let language = if config.language == "auto" { page_language(&html) } else { config.language.clone() };
    let w = words(&language);
    let mac_hint = if config.shortcut { "<kbd>⌘K</kbd>" } else { "" };
    let button = format!(
        "<button type=\"button\" class=\"sss-open\" aria-haspopup=\"dialog\" aria-controls=\"stride-search\">{ICON}<span>{}</span>{mac_hint}</button>",
        w.search
    );
    let dialog = format!(
        "<dialog {MARKER} aria-label=\"{search}\"><div class=\"sss-box\"><div class=\"sss-field\">{ICON}\
         <input type=\"search\" placeholder=\"{placeholder}\" aria-label=\"{placeholder}\" autocomplete=\"off\" spellcheck=\"false\" \
         role=\"combobox\" aria-expanded=\"true\" aria-controls=\"sss-results\" aria-autocomplete=\"list\">\
         <button type=\"button\" class=\"sss-close\" aria-label=\"Close\">Esc</button></div>\
         <ul id=\"sss-results\" role=\"listbox\" aria-label=\"{search}\"></ul>\
         <p class=\"sss-empty\" hidden></p><p class=\"sss-foot\"><kbd>↑</kbd><kbd>↓</kbd> <kbd>↵</kbd> {hint}</p></div></dialog>",
        search = w.search,
        placeholder = w.placeholder,
        hint = w.hint,
    );
    let fg = readable_on(&config.colour);
    let style = format!(
        "<style id=\"stride-search-style\">\
         .sss-open{{display:inline-flex;align-items:center;gap:8px;padding:7px 10px 7px 12px;border:1px solid rgba(127,127,127,.35);\
         border-radius:999px;background:transparent;color:inherit;font:500 14px/1 system-ui,-apple-system,\"Segoe UI\",sans-serif;cursor:pointer}}\
         .sss-open:hover{{border-color:currentColor}}.sss-open:focus-visible{{outline:2px solid {c};outline-offset:2px}}\
         .sss-open kbd{{font:600 11px/1 ui-monospace,Menlo,monospace;padding:3px 5px;border-radius:5px;background:rgba(127,127,127,.18)}}\
         .sss-float{{position:fixed;left:20px;bottom:20px;z-index:2147482000;background:{c};color:{fg};border:0;\
         box-shadow:0 10px 30px rgba(0,0,0,.2);padding:11px 16px 11px 14px}}\
         #stride-search{{border:0;padding:0;margin:10vh auto auto;width:min(640px,calc(100vw - 24px));max-height:76vh;border-radius:16px;\
         background:#fff;color:#111827;box-shadow:0 30px 90px rgba(0,0,0,.35);font:15px/1.45 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
         #stride-search::backdrop{{background:rgba(15,23,42,.5);backdrop-filter:blur(2px)}}\
         #stride-search .sss-box{{display:flex;flex-direction:column;max-height:76vh}}\
         #stride-search .sss-field{{display:flex;align-items:center;gap:10px;padding:14px 16px;border-bottom:1px solid #e5e7eb;color:#6b7280}}\
         #stride-search input{{flex:1;border:0;outline:0;font:500 17px/1.3 inherit;font-family:inherit;color:#111827;background:transparent;min-width:0}}\
         #stride-search input::-webkit-search-cancel-button{{display:none}}\
         #stride-search .sss-close{{border:1px solid #e5e7eb;background:#f9fafb;border-radius:6px;font:600 11px/1 ui-monospace,Menlo,monospace;\
         padding:5px 7px;color:#6b7280;cursor:pointer}}\
         #stride-search ul{{list-style:none;margin:0;padding:8px;overflow:auto}}\
         #stride-search li a{{display:block;padding:10px 12px;border-radius:10px;color:inherit;text-decoration:none}}\
         #stride-search li a b{{display:block;font-weight:650;color:#111827}}\
         #stride-search li a span{{display:block;font-size:13.5px;color:#4b5563;margin-top:2px;overflow:hidden;\
         display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical}}\
         #stride-search li a small{{display:block;font-size:12px;color:#9ca3af;margin-top:3px}}\
         #stride-search li[aria-selected=true] a{{background:{c};color:{fg}}}\
         #stride-search li[aria-selected=true] a b,#stride-search li[aria-selected=true] a span,#stride-search li[aria-selected=true] a small{{color:inherit}}\
         #stride-search mark{{background:rgba(250,204,21,.45);color:inherit;border-radius:3px;padding:0 1px}}\
         #stride-search li[aria-selected=true] mark{{background:rgba(255,255,255,.3)}}\
         #stride-search .sss-empty{{margin:0;padding:22px 20px;color:#6b7280}}\
         #stride-search .sss-foot{{margin:0;padding:10px 16px;border-top:1px solid #e5e7eb;font-size:12px;color:#9ca3af}}\
         #stride-search .sss-foot kbd{{font:600 11px/1 ui-monospace,Menlo,monospace;padding:3px 5px;border-radius:5px;background:#f3f4f6;margin-right:2px}}\
         @media (max-width:640px){{#stride-search{{margin-top:12px}}.sss-open span,.sss-open kbd{{display:none}}}}</style>",
        c = config.colour,
    );
    let script = format!(
        "<script>(function(){{var I={index},NONE='{none}',KEYS={keys};\
         var d=document.getElementById('stride-search');if(!d||!d.showModal)return;\
         var q=d.querySelector('input'),ul=d.querySelector('ul'),empty=d.querySelector('.sss-empty'),sel=0,hits=[];\
         function norm(s){{return s.toLowerCase().normalize('NFD').replace(/[\\u0300-\\u036f]/g,'')}}\
         var N=I.map(function(e){{return [norm(e[1]),norm(e[2])]}});\
         function esc(s){{return s.replace(/[&<>\"]/g,function(c){{return{{'&':'&amp;','<':'&lt;','>':'&gt;','\"':'&quot;'}}[c]}})}}\
         function mark(s,terms){{var n=norm(s),out='',i=0;while(i<s.length){{var hit=0;\
         for(var t=0;t<terms.length;t++){{if(terms[t]&&n.substr(i,terms[t].length)===terms[t]){{hit=terms[t].length;break}}}}\
         if(hit){{out+='<mark>'+esc(s.substr(i,hit))+'</mark>';i+=hit}}else{{out+=esc(s[i]);i++}}}}return out}}\
         function snippet(text,terms){{var n=norm(text),at=-1;for(var t=0;t<terms.length;t++){{var p=n.indexOf(terms[t]);if(p>=0&&(at<0||p<at))at=p}}\
         var start=Math.max(0,at-40);return (start?'…':'')+text.substr(start,160)}}\
         function search(){{var terms=norm(q.value).split(/\\s+/).filter(Boolean);hits=[];\
         if(terms.length){{I.forEach(function(e,i){{var s=0;for(var t=0;t<terms.length;t++){{var term=terms[t],a=N[i][0].indexOf(term),b=N[i][1].indexOf(term);\
         if(a<0&&b<0)return;s+=(a===0?12:a>0?8:0)+(b>=0?2:0)}}hits.push([s,i])}});\
         hits.sort(function(a,b){{return b[0]-a[0]}});hits=hits.slice(0,12)}}sel=0;draw(terms)}}\
         function draw(terms){{ul.innerHTML=hits.map(function(h,k){{var e=I[h[1]];\
         return '<li role=\"option\" id=\"sss-'+k+'\" aria-selected=\"'+(k===sel)+'\"><a href=\"/'+(e[0]==='home'?'':e[0])+'\" tabindex=\"-1\"><b>'+mark(e[1]||e[0],terms)+\
         '</b><span>'+mark(snippet(e[2],terms),terms)+'</span><small>/'+(e[0]==='home'?'':esc(e[0]))+'</small></a></li>'}}).join('');\
         q.setAttribute('aria-activedescendant',hits.length?'sss-'+sel:'');\
         empty.hidden=!(q.value.trim()&&!hits.length);empty.textContent=NONE+' \\u201c'+q.value.trim()+'\\u201d'}}\
         function move(k){{if(!hits.length)return;sel=(sel+k+hits.length)%hits.length;[].forEach.call(ul.children,function(li,i){{li.setAttribute('aria-selected',i===sel)}});\
         q.setAttribute('aria-activedescendant','sss-'+sel);ul.children[sel].scrollIntoView({{block:'nearest'}})}}\
         function open(){{if(!d.open){{d.showModal();q.select()}}}}\
         q.addEventListener('input',search);\
         q.addEventListener('keydown',function(e){{if(e.key==='ArrowDown'){{e.preventDefault();move(1)}}else if(e.key==='ArrowUp'){{e.preventDefault();move(-1)}}\
         else if(e.key==='Enter'&&hits.length){{e.preventDefault();location.href=ul.children[sel].querySelector('a').href}}}});\
         d.querySelector('.sss-close').onclick=function(){{d.close()}};\
         d.addEventListener('click',function(e){{if(e.target===d)d.close()}});\
         [].forEach.call(document.querySelectorAll('.sss-open'),function(b){{b.addEventListener('click',open)}});\
         if(KEYS)document.addEventListener('keydown',function(e){{var t=e.target,typing=t&&(t.isContentEditable||/^(input|textarea|select)$/i.test(t.tagName));\
         if((e.key==='k'||e.key==='K')&&(e.metaKey||e.ctrlKey)){{e.preventDefault();open()}}else if(e.key==='/'&&!typing&&!d.open){{e.preventDefault();open()}}}});\
         if(!/Mac|iPhone|iPad/.test(navigator.platform||''))[].forEach.call(document.querySelectorAll('.sss-open kbd'),function(k){{k.textContent='Ctrl K'}});\
         }})()</script>",
        index = index,
        none = js_string(w.none),
        keys = config.shortcut,
    );

    let mut html = html;
    insert_in_head(&mut html, &style);
    // The button goes at the end of the page header, beside the menu, when
    // there is one; otherwise it floats in a corner.
    let lower = html.to_ascii_lowercase();
    let header_end = lower.find("<header").and_then(|h| lower[h..].find("</header>").map(|e| h + e));
    match header_end {
        Some(end) if config.placement == "header" => {
            // Inside the header's last child when it is a wrapper, so the
            // button sits in the same row as the menu.
            let inner_close = lower[..end].rfind("</div>").filter(|&c| c > lower[..end].rfind("<header").unwrap_or(0));
            html.insert_str(inner_close.unwrap_or(end), &button);
        }
        _ => insert_before_body_end(&mut html, &button.replacen("class=\"sss-open\"", "class=\"sss-open sss-float\"", 1)),
    }
    insert_before_body_end(&mut html, &format!("{dialog}{script}"));
    html
}

#[plugin_fn]
pub fn panel_site_search(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => {
            let values = Config::from_values(&load_values().unwrap_or_default()).to_values();
            let indexed = kv::list(PAGE_PREFIX).map(|k| k.len()).unwrap_or(0);
            Ok(Json(PanelResponse {
                values,
                message: format!("{indexed} pages have their text in the search. Every published page can be found by its title; its text is added the first time someone opens it."),
                ..PanelResponse::default()
            }))
        }
        PanelEvent::Submit => {
            let config = Config::from_values(&request.values);
            let values = config.to_values();
            for (key, value) in &values {
                if let Err(error) = kv::set(key, value) {
                    return Ok(Json(PanelResponse {
                        error: if error.is_permission_denied() {
                            "This plugin was not granted the storage permission, so it has nowhere to keep the search index.".to_owned()
                        } else {
                            error.to_string()
                        },
                        ..PanelResponse::default()
                    }));
                }
            }
            Ok(Json(PanelResponse {
                values,
                message: "Saved. The search box shows on your pages right away.".to_owned(),
                error: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html lang=\"nl\"><head><title>Over ons — Atelier</title></head><body><div><header class=\"h\"><div class=\"w\"><a href=\"/\">Logo</a>\
                        <nav><a href=\"/menu\">Menu</a></nav></div></header><main><h1>Over &amp; ons</h1><p>Wij maken   stoelen.</p>\
                        <script>var secret=1</script><footer>voet</footer></main><footer>Contact</footer></body></html>";

    #[test]
    fn entry_is_the_public_main_text() {
        let (title, text) = entry_for(PAGE, 600);
        assert_eq!(title, "Over ons — Atelier");
        let (title, _) = entry_for("<body><h1>Kop</h1><svg><title>Pin</title></svg></body>", 600);
        assert_eq!(title, "Kop");
        assert_eq!(text, "Wij maken stoelen.");
    }

    #[test]
    fn renders_button_in_header_dialog_and_index_once() {
        let entries = vec![
            ("home".into(), "Home".into(), "Welkom".into()),
            ("over".into(), "Over & ons".into(), "</script><b>x".into()),
        ];
        let out = render(PAGE.into(), &Config::default(), &index_json(entries.clone()));
        let button = out.find("<button type=\"button\" class=\"sss-open\"").unwrap();
        assert!(button > out.find("</nav>").unwrap() && button < out.find("</header>").unwrap());
        assert!(out.contains("<span>Zoeken</span>"), "language from <html lang>");
        assert!(out.contains("[\"over\",\"Over \\u0026 ons\",\"\\u003c/script\\u003e\\u003cb\\u003ex\"]"));
        assert!(!out.contains("</script><b>x"));
        assert!(out.find("<dialog id=\"stride-search\"").unwrap() > out.find("</main>").unwrap());
        assert_eq!(render(out.clone(), &Config::default(), &index_json(entries)).matches(MARKER).count(), 1);
    }

    #[test]
    fn floats_without_a_header_and_hides_without_an_index() {
        let bare = "<html><body><main><p>x</p></main></body></html>";
        assert_eq!(render(bare.into(), &Config::default(), "[]"), bare);
        let out = render(bare.into(), &Config::default(), &index_json(vec![("home".into(), "Home".into(), "x".into())]));
        assert!(out.contains("class=\"sss-open sss-float\""));
    }

    #[test]
    fn access_rules_and_instances_are_found() {
        let tree: JsonValue = serde_json::json!({"id":"r","kind":{"type":"frame"},"children":[
            {"id":"a","kind":{"type":"text","tag":"p","content":"x"},"access":{"type":"public"}},
            {"id":"b","kind":{"type":"instance","component":"c1"}}]});
        let mut inst = Vec::new();
        assert!(tree_is_public(&tree, &mut inst));
        assert_eq!(inst, vec!["c1".to_owned()]);
        let secret: JsonValue = serde_json::json!({"id":"r","kind":{"type":"frame"},"children":[
            {"id":"a","kind":{"type":"frame"},"children":[{"id":"s","kind":{"type":"text","tag":"p","content":"x"},"access":{"type":"signedIn"}}]}]});
        assert!(!tree_is_public(&secret, &mut Vec::new()));
    }

    #[test]
    fn index_is_trimmed_to_budget() {
        let big: Vec<_> = (0..400).map(|i| (format!("p{i}"), format!("Page {i}"), "word ".repeat(400))).collect();
        let json = index_json(big);
        assert!(json.len() <= MAX_INLINE);
        assert!(json.contains("\"p399\""), "pages are kept, texts shortened");
    }

    #[test]
    fn defaults_skip_404_and_settings_round_trip() {
        let c = Config::default();
        assert!(slug_matches(&c.skip_pages, "404"));
        assert_eq!(Config::from_values(&c.to_values()), c);
    }
}
