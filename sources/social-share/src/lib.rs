//! Social Share: a row of share buttons under every article.
//!
//! Every button is a plain `<a href>` to the network's own share address, with
//! an inline SVG icon. Nothing is loaded from another site and nothing is
//! tracked. The only script is a few hundred bytes of first-party JavaScript
//! for the copy-link button and, when the page's absolute address is not
//! known on the server, for filling that address into the links.
//!
//! Permissions: `storage`, for the panel's settings and for the site's origin
//! learned from `on_publish`. Without it the plugin still works with its
//! defaults and reads the page's address in the browser.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, Published,
    kv,
};

const SETTINGS: &str = "settings";
const ORIGIN: &str = "origin";
const MARKER: &str = "class=\"ss-share";

// ------------------------------------------------------------------ networks

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Network {
    X,
    LinkedIn,
    Facebook,
    WhatsApp,
    Email,
}

impl Network {
    const ALL: [Network; 5] = [
        Network::X,
        Network::LinkedIn,
        Network::Facebook,
        Network::WhatsApp,
        Network::Email,
    ];

    fn key(self) -> &'static str {
        match self {
            Network::X => "x",
            Network::LinkedIn => "linkedin",
            Network::Facebook => "facebook",
            Network::WhatsApp => "whatsapp",
            Network::Email => "email",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Network::X => "Post",
            Network::LinkedIn => "LinkedIn",
            Network::Facebook => "Facebook",
            Network::WhatsApp => "WhatsApp",
            Network::Email => "E-mail",
        }
    }

    fn aria(self) -> &'static str {
        match self {
            Network::X => "Post on X (opens in a new tab)",
            Network::LinkedIn => "Share on LinkedIn (opens in a new tab)",
            Network::Facebook => "Share on Facebook (opens in a new tab)",
            Network::WhatsApp => "Share on WhatsApp (opens in a new tab)",
            Network::Email => "Share by e-mail",
        }
    }

    /// The share address, with `{u}` for the encoded page address and `{t}`
    /// for the encoded title.
    fn template(self) -> &'static str {
        match self {
            Network::X => "https://x.com/intent/post?url={u}&text={t}",
            Network::LinkedIn => "https://www.linkedin.com/sharing/share-offsite/?url={u}",
            Network::Facebook => "https://www.facebook.com/sharer/sharer.php?u={u}",
            Network::WhatsApp => "https://wa.me/?text={t}%20{u}",
            Network::Email => "mailto:?subject={t}&body={u}",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Network::X => {
                r#"<path fill="currentColor" d="M18.9 1.15h3.68l-8.04 9.19L24 22.85h-7.41l-5.8-7.59-6.64 7.59H.47l8.6-9.83L0 1.15h7.59l5.24 6.93zm-1.29 19.5h2.04L6.49 3.24H4.3z"/>"#
            }
            Network::LinkedIn => {
                r#"<path fill="currentColor" d="M20.45 20.45h-3.56v-5.57c0-1.33-.02-3.04-1.85-3.04-1.85 0-2.14 1.45-2.14 2.94v5.67H9.35V9h3.41v1.56h.05c.48-.9 1.64-1.85 3.37-1.85 3.6 0 4.27 2.37 4.27 5.46zM5.34 7.43a2.06 2.06 0 1 1 0-4.13 2.06 2.06 0 0 1 0 4.13zM7.12 20.45H3.56V9h3.56zM22.22 0H1.77C.79 0 0 .77 0 1.73v20.54C0 23.23.79 24 1.77 24h20.45c.98 0 1.78-.77 1.78-1.73V1.73C24 .77 23.2 0 22.22 0z"/>"#
            }
            Network::Facebook => {
                r#"<path fill="currentColor" d="M24 12.07C24 5.4 18.63 0 12 0S0 5.4 0 12.07C0 18.1 4.39 23.1 10.13 24v-8.44H7.08v-3.49h3.05V9.41c0-3.02 1.79-4.7 4.53-4.7 1.31 0 2.69.24 2.69.24v2.97h-1.51c-1.5 0-1.96.93-1.96 1.89v2.26h3.33l-.53 3.49h-2.8V24C19.61 23.1 24 18.1 24 12.07z"/>"#
            }
            Network::WhatsApp => {
                r#"<path fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" d="M3.2 20.8l1.25-4.1a9 9 0 1 1 3.4 3.1z"/><path fill="currentColor" d="M9.3 7.2c.3-.1.6 0 .75.3l.8 1.85c.1.28.03.5-.17.7l-.62.62a6.4 6.4 0 0 0 2.97 2.97l.62-.62c.2-.2.43-.27.7-.17l1.85.8c.3.14.4.45.3.75-.35 1.02-1.25 1.62-2.3 1.5a7.3 7.3 0 0 1-6.4-6.4c-.12-1.05.48-1.95 1.5-2.3z"/>"#
            }
            Network::Email => {
                r#"<rect x="2.5" y="4.5" width="19" height="15" rx="2.5" fill="none" stroke="currentColor" stroke-width="2"/><path d="M3.5 6.5l8.5 6.5 8.5-6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"#
            }
        }
    }

    fn color(self) -> &'static str {
        match self {
            Network::X => "#000000",
            Network::LinkedIn => "#0A66C2",
            Network::Facebook => "#0866FF",
            Network::WhatsApp => "#0F7B3F",
            Network::Email => "#475569",
        }
    }
}

const LINK_ICON: &str = r#"<path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#;

// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub heading: String,
    pub style: String,
    pub accent: String,
    pub align: String,
    pub networks: [bool; 5],
    pub copy: bool,
    pub only: String,
    pub exclude: String,
    pub site_address: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            heading: "Share this article".to_owned(),
            style: "brand".to_owned(),
            accent: "#DB2777".to_owned(),
            align: "left".to_owned(),
            networks: [true; 5],
            copy: true,
            only: String::new(),
            exclude: "home".to_owned(),
            site_address: String::new(),
        }
    }
}

const STYLES: [&str; 4] = ["brand", "outline", "icons", "minimal"];
const ALIGNS: [&str; 2] = ["left", "center"];

impl Settings {
    /// Every read may fail: without `storage` the defaults apply.
    fn load() -> Self {
        match kv::get::<Map<String, JsonValue>>(SETTINGS) {
            Ok(Some(values)) => Settings::from_values(&values),
            _ => Settings::default(),
        }
    }

    /// Build settings from panel values, falling back to the default for
    /// anything missing or malformed.
    pub fn from_values(values: &Map<String, JsonValue>) -> Self {
        let d = Settings::default();
        let boolean = |name: &str, fallback: bool| {
            values
                .get(name)
                .and_then(JsonValue::as_bool)
                .unwrap_or(fallback)
        };
        let text = |name: &str, fallback: &str| {
            values
                .get(name)
                .and_then(JsonValue::as_str)
                .map(|s| s.trim().to_owned())
                .unwrap_or_else(|| fallback.to_owned())
        };
        let choice = |name: &str, allowed: &[&str], fallback: &str| {
            let value = text(name, fallback);
            if allowed.contains(&value.as_str()) {
                value
            } else {
                fallback.to_owned()
            }
        };
        let mut networks = d.networks;
        for (index, network) in Network::ALL.iter().enumerate() {
            networks[index] = boolean(network.key(), true);
        }
        let accent = text("accent", &d.accent);
        Settings {
            enabled: boolean("enabled", d.enabled),
            heading: text("heading", &d.heading),
            style: choice("style", &STYLES, &d.style),
            accent: if is_hex_color(&accent) { accent } else { d.accent },
            align: choice("align", &ALIGNS, &d.align),
            networks,
            copy: boolean("copy", d.copy),
            only: text("only", &d.only),
            exclude: text("exclude", &d.exclude),
            site_address: text("site-address", &d.site_address),
        }
    }

    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("heading".into(), self.heading.clone().into());
        values.insert("style".into(), self.style.clone().into());
        values.insert("accent".into(), self.accent.clone().into());
        values.insert("align".into(), self.align.clone().into());
        for (index, network) in Network::ALL.iter().enumerate() {
            values.insert(network.key().into(), self.networks[index].into());
        }
        values.insert("copy".into(), self.copy.into());
        values.insert("only".into(), self.only.clone().into());
        values.insert("exclude".into(), self.exclude.clone().into());
        values.insert("site-address".into(), self.site_address.clone().into());
        values
    }

    fn any_button(&self) -> bool {
        self.copy || self.networks.iter().any(|on| *on)
    }
}

pub fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

/// `https://example.com` from anything that starts like an http(s) URL.
pub fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .map(|r| ("https://", r))
        .or_else(|| url.strip_prefix("http://").map(|r| ("http://", r)))?;
    let host = rest.1.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b))
    {
        return None;
    }
    Some(format!("{}{}", rest.0, host.to_ascii_lowercase()))
}

// ----------------------------------------------------------------- the hooks

#[plugin_fn]
pub fn on_publish(Json(event): Json<Published>) -> FnResult<Json<JsonValue>> {
    // Remember the site's origin, once, when the host tells us an absolute
    // address. Refused storage just means the browser fills it in instead.
    if let Some(origin) = origin_of(&event.url) {
        let known = kv::get::<String>(ORIGIN).ok().flatten();
        if known.as_deref() != Some(origin.as_str()) {
            let _ = kv::set(ORIGIN, &origin);
        }
    }
    Ok(Json(JsonValue::Object(Map::new())))
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = Settings::load();
    let origin = origin_of(&settings.site_address)
        .or_else(|| kv::get::<String>(ORIGIN).ok().flatten());
    let html = render(&page.html, &page.slug, &settings, origin.as_deref())
        .unwrap_or(page.html);
    Ok(Json(PageRendered { html }))
}

/// The page with the share row in it, or `None` to leave it alone.
pub fn render(html: &str, slug: &str, settings: &Settings, origin: Option<&str>) -> Option<String> {
    if !settings.enabled || !settings.any_button() || html.contains(MARKER) {
        return None;
    }
    if !wanted(slug, &settings.only, &settings.exclude) {
        return None;
    }
    let title = page_title(html);
    let url = origin.map(|origin| {
        let slug = slug.trim_matches('/');
        if slug == "home" || slug.is_empty() {
            format!("{origin}/")
        } else {
            format!("{origin}/{slug}")
        }
    });
    let block = share_block(settings, &title, url.as_deref());
    let style = format!("<style>{CSS}</style>");
    // The stylesheet belongs in <head>; only a page without one gets it inline.
    match find_ci(html.as_bytes(), b"</head>") {
        Some(head) => {
            let body = inject(html, &block)?;
            let mut out = String::with_capacity(body.len() + style.len());
            out.push_str(&body[..head]);
            out.push_str(&style);
            out.push_str(&body[head..]);
            Some(out)
        }
        None => inject(html, &format!("{style}{block}")),
    }
}

/// Whether a page with this slug gets buttons.
pub fn wanted(slug: &str, only: &str, exclude: &str) -> bool {
    let slug = slug.trim_matches('/');
    let list = |text: &str| -> Vec<String> {
        text.split([',', ' ', '\n'])
            .map(|s| s.trim().trim_matches('/').to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    };
    for pattern in list(exclude) {
        let hit = match pattern.strip_suffix('*') {
            Some(prefix) => slug.starts_with(prefix.trim_end_matches('/')),
            None => slug == pattern,
        };
        if hit {
            return false;
        }
    }
    let only = list(only);
    if only.is_empty() {
        return true;
    }
    only.iter().any(|prefix| {
        let prefix = prefix.trim_end_matches('*').trim_end_matches('/');
        slug == prefix || slug.starts_with(&format!("{prefix}/")) || slug.starts_with(prefix)
    })
}

// ------------------------------------------------------------------ the HTML

const CSS: &str = ".ss-share{--ss-a:#DB2777;box-sizing:border-box;width:calc(100% - 2.5rem);max-width:46rem;margin:2.5rem auto;padding:1.5rem 0 0;border-top:1px solid rgba(127,127,127,.25);font:500 14px/1.2 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif}\
.ss-center{text-align:center}.ss-center .ss-list{justify-content:center}\
.ss-share .ss-title{margin:0 0 .9rem;font-size:13px;font-weight:600;letter-spacing:.08em;text-transform:uppercase;opacity:.7}\
.ss-list{display:flex;flex-wrap:wrap;gap:.5rem;list-style:none;margin:0;padding:0}\
.ss-btn{display:inline-flex;align-items:center;gap:.45rem;min-height:40px;padding:.5rem .9rem;border-radius:999px;border:1px solid transparent;background:none;color:inherit;font:inherit;text-decoration:none;cursor:pointer;transition:transform .15s,filter .15s,background-color .15s}\
.ss-btn svg{width:18px;height:18px;flex:none}\
.ss-btn:hover{transform:translateY(-1px)}\
.ss-btn:focus-visible{outline:3px solid var(--ss-a);outline-offset:2px}\
.ss-brand .ss-btn{background:var(--c);color:#fff}.ss-brand .ss-btn:hover{filter:brightness(1.12)}\
.ss-outline .ss-btn{border-color:rgba(127,127,127,.35)}.ss-outline .ss-btn svg{color:var(--c)}.ss-outline .ss-btn:hover{border-color:var(--c)}\
.ss-icons .ss-btn{width:44px;height:44px;padding:0;justify-content:center;background:var(--c);color:#fff}\
.ss-minimal .ss-btn{width:40px;height:40px;padding:0;justify-content:center;color:var(--ss-a)}.ss-minimal .ss-btn:hover{background:rgba(127,127,127,.12)}\
.ss-copy{--c:var(--ss-a)}.ss-done svg{animation:ss-pop .3s}\
@keyframes ss-pop{50%{transform:scale(1.25)}}\
.ss-sr{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}\
@media (prefers-reduced-motion:reduce){.ss-btn{transition:none}.ss-btn:hover{transform:none}.ss-done svg{animation:none}}";

/// Fills in `data-ss-t` links from the browser when the server did not know
/// the address, and wires up the copy button, revealing it only when it can
/// work.
const JS: &str = "(function(){var r=document.querySelector('.ss-share');if(!r)return;var u=location.href.split('#')[0],e=encodeURIComponent;r.querySelectorAll('a[data-ss-t]').forEach(function(a){a.href=a.getAttribute('data-ss-t').split('{u}').join(e(u))});var b=r.querySelector('.ss-copy'),s=r.querySelector('.ss-status');if(!b||!navigator.clipboard)return;var l=r.getAttribute('data-ss-url')||u;b.parentNode.hidden=false;b.addEventListener('click',function(){navigator.clipboard.writeText(l).then(function(){s.textContent='Link copied';b.classList.add('ss-done');setTimeout(function(){s.textContent='';b.classList.remove('ss-done')},2000)})})})();";

fn svg(inner: &str) -> String {
    format!(
        "<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\" \
         focusable=\"false\">{inner}</svg>"
    )
}

pub fn share_block(settings: &Settings, title: &str, url: Option<&str>) -> String {
    let labels = matches!(settings.style.as_str(), "brand" | "outline");
    let encoded_title = percent_encode(title);
    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "<aside class=\"ss-share ss-{} ss-{}\" style=\"--ss-a:{}\" aria-label=\"Share this page\"",
        settings.style, settings.align, settings.accent
    ));
    if let Some(url) = url {
        out.push_str(&format!(" data-ss-url=\"{}\"", escape(url)));
    }
    out.push('>');
    if !settings.heading.is_empty() {
        out.push_str(&format!(
            "<h2 class=\"ss-title\">{}</h2>",
            escape(&settings.heading)
        ));
    }
    out.push_str("<ul class=\"ss-list\">");
    for (index, network) in Network::ALL.iter().enumerate() {
        if !settings.networks[index] {
            continue;
        }
        let template = network.template().replace("{t}", &encoded_title);
        let external = *network != Network::Email;
        let href = match url {
            Some(url) => template.replace("{u}", &percent_encode(url)),
            None => template.replace("{u}", ""),
        };
        out.push_str("<li><a class=\"ss-btn\" href=\"");
        out.push_str(&escape(&href));
        out.push('"');
        if url.is_none() {
            out.push_str(" data-ss-t=\"");
            out.push_str(&escape(&template));
            out.push('"');
        }
        if external {
            out.push_str(" target=\"_blank\" rel=\"noopener noreferrer\"");
        }
        out.push_str(&format!(
            " style=\"--c:{}\" aria-label=\"{}\"",
            network.color(),
            network.aria()
        ));
        if !labels {
            out.push_str(&format!(" title=\"{}\"", network.aria()));
        }
        out.push('>');
        out.push_str(&svg(network.icon()));
        if labels {
            out.push_str(&format!("<span>{}</span>", network.label()));
        }
        out.push_str("</a></li>");
    }
    if settings.copy {
        out.push_str(
            "<li hidden><button type=\"button\" class=\"ss-btn ss-copy\" aria-label=\"Copy link\"",
        );
        if !labels {
            out.push_str(" title=\"Copy link\"");
        }
        out.push('>');
        out.push_str(&svg(LINK_ICON));
        if labels {
            out.push_str("<span>Copy link</span>");
        }
        out.push_str("</button></li>");
    }
    out.push_str("</ul><span class=\"ss-status ss-sr\" role=\"status\" aria-live=\"polite\"></span></aside>");
    if settings.copy || url.is_none() {
        out.push_str("<script>");
        out.push_str(JS);
        out.push_str("</script>");
    }
    out
}

/// After the first `</article>` when the page has one, else at the end of the
/// outermost wrapper before `</body>`. `None` without a body (a fragment, a feed).
pub fn inject(html: &str, block: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let at = match find_ci(bytes, b"</article>") {
        Some(at) => at + "</article>".len(),
        None => {
            // Inside the page's outermost wrapper when there is one, so the
            // row picks up the page's own background, colour and width.
            let end = rfind_ci(bytes, b"</body>")?;
            let before = html[..end].trim_end();
            ["</div>", "</main>"]
                .iter()
                .find(|close| {
                    before.len() >= close.len()
                        && before.as_bytes()[before.len() - close.len()..]
                            .eq_ignore_ascii_case(close.as_bytes())
                })
                .map_or(end, |close| before.len() - close.len())
        }
    };
    let mut out = String::with_capacity(html.len() + block.len());
    out.push_str(&html[..at]);
    out.push_str(block);
    out.push_str(&html[at..]);
    Some(out)
}

/// The text of `<title>`, else of the first `<h1>`, with tags stripped and the
/// common entities decoded, so it can be percent-encoded as the reader sees it.
pub fn page_title(html: &str) -> String {
    let bytes = html.as_bytes();
    for (open, close) in [(&b"<title"[..], &b"</title>"[..]), (b"<h1", b"</h1>")] {
        if let Some(start) = find_ci(bytes, open)
            && let Some(gt) = html[start..].find('>')
        {
            let from = start + gt + 1;
            if let Some(len) = find_ci(&bytes[from..], close) {
                let text = decode_entities(&strip_tags(&html[from..from + len]));
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }
    String::new()
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "hellip" => Some('\u{2026}'),
            "rsquo" => Some('\u{2019}'),
            "lsquo" => Some('\u{2018}'),
            "ldquo" => Some('\u{201C}'),
            "rdquo" => Some('\u{201D}'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// RFC 3986 percent-encoding of UTF-8: only unreserved characters stay.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

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

// ------------------------------------------------------------------ the panel

#[plugin_fn]
pub fn panel_social_share(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse {
            values: Settings::load().to_values(),
            message: String::new(),
            error: String::new(),
        })),
        PanelEvent::Submit => {
            let site_address = request
                .values
                .get("site-address")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim();
            if !site_address.is_empty() && origin_of(site_address).is_none() {
                return Ok(Json(PanelResponse {
                    values: request.values.clone(),
                    message: String::new(),
                    error: "The site address has to start with https:// (or http://) followed \
                            by a domain, like https://example.com. Nothing was saved."
                        .to_owned(),
                }));
            }
            if let Some(accent) = request.values.get("accent").and_then(JsonValue::as_str)
                && !accent.trim().is_empty()
                && !is_hex_color(accent.trim())
            {
                return Ok(Json(PanelResponse {
                    values: request.values.clone(),
                    message: String::new(),
                    error: "The accent colour has to be a hex colour like #DB2777. Nothing was \
                            saved."
                        .to_owned(),
                }));
            }
            let settings = Settings::from_values(&request.values);
            if let Err(error) = kv::set(SETTINGS, &settings.to_values()) {
                return Ok(Json(PanelResponse {
                    values: settings.to_values(),
                    message: String::new(),
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so there is \
                         nowhere to keep these settings. It still shows the default share \
                         buttons on every page but the home page."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                }));
            }
            let message = if !settings.enabled {
                "Saved. Share buttons are off on this site.".to_owned()
            } else if !settings.any_button() {
                "Saved, but every button is switched off, so nothing is shown.".to_owned()
            } else {
                "Saved. Your articles show the new share buttons.".to_owned()
            };
            Ok(Json(PanelResponse {
                values: settings.to_values(),
                message,
                error: String::new(),
            }))
        }
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html><html><head><title>Tom &amp; Jerry &mdash; a story</title></head>\
<body><div><h1>Tom</h1><p>Text.</p></div></body></html>";

    #[test]
    fn encodes_per_rfc3986() {
        assert_eq!(percent_encode("a b&c/é~"), "a%20b%26c%2F%C3%A9~");
    }

    #[test]
    fn title_is_decoded() {
        assert_eq!(page_title(PAGE), "Tom & Jerry \u{2014} a story");
        assert_eq!(page_title("<body><h1>Hi <em>there</em></h1></body>"), "Hi there");
    }

    #[test]
    fn injects_before_body_end_with_absolute_links() {
        let out = render(PAGE, "tom", &Settings::default(), Some("https://ex.com")).unwrap();
        assert!(out.contains("</aside><script>"));
        assert!(out.contains("<p>Text.</p><aside"));
        assert!(out.contains("</style></head>"));
        assert_eq!(out.matches("<style>").count(), 1);
        assert!(out.contains("</script></div></body>"));
        assert!(out.contains("https://www.facebook.com/sharer/sharer.php?u=https%3A%2F%2Fex.com%2Ftom\""));
        assert!(out.contains("x.com/intent/post?url=https%3A%2F%2Fex.com%2Ftom&amp;text=Tom%20%26%20Jerry"));
        assert!(!out.contains("data-ss-t="));
        assert!(!out.contains("<script src"));
    }

    #[test]
    fn falls_back_to_the_browser_address() {
        let out = render(PAGE, "tom", &Settings::default(), None).unwrap();
        assert!(out.contains("data-ss-t=\"https://www.linkedin.com/sharing/share-offsite/?url={u}\""));
        assert!(out.contains("<script>"));
    }

    #[test]
    fn style_goes_inline_without_a_head() {
        let out = render("<body><div><p>x</p></div></body>", "a", &Settings::default(), None).unwrap();
        assert!(out.contains("<p>x</p><style>"));
    }

    #[test]
    fn after_article() {
        let html = "<html><body><article>A</article><footer>F</footer></body></html>";
        let out = inject(html, "B").unwrap();
        assert_eq!(out, "<html><body><article>A</article>B<footer>F</footer></body></html>");
    }

    #[test]
    fn leaves_fragments_home_and_repeats_alone() {
        let s = Settings::default();
        assert!(render("<p>x</p>", "a", &s, None).is_none());
        assert!(render(PAGE, "home", &s, None).is_none());
        let once = render(PAGE, "a", &s, None).unwrap();
        assert!(render(&once, "a", &s, None).is_none());
    }

    #[test]
    fn page_filters() {
        assert!(wanted("blog/post", "blog", "home"));
        assert!(!wanted("about", "blog, news", "home"));
        assert!(!wanted("legal/terms", "", "home, legal/*"));
        assert!(wanted("anything", "", "home"));
    }

    #[test]
    fn escapes_admin_text() {
        let mut s = Settings::default();
        s.heading = "<script>alert(1)</script>".into();
        let out = share_block(&s, "t", None);
        assert!(out.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn bad_values_fall_back() {
        let mut v = Map::new();
        v.insert("style".into(), "evil\"".into());
        v.insert("accent".into(), "red;}".into());
        v.insert("x".into(), false.into());
        let s = Settings::from_values(&v);
        assert_eq!(s.style, "brand");
        assert_eq!(s.accent, "#DB2777");
        assert!(!s.networks[0]);
        assert!(s.networks[1]);
    }

    #[test]
    fn origins() {
        assert_eq!(origin_of("https://Ex.com/about").as_deref(), Some("https://ex.com"));
        assert_eq!(origin_of("/about"), None);
        assert_eq!(origin_of("https://ex.com\"><x").as_deref(), None);
    }

    #[test]
    fn icon_styles_have_no_visible_labels_but_have_aria() {
        let mut s = Settings::default();
        s.style = "icons".into();
        let out = share_block(&s, "t", None);
        assert!(!out.contains("<span>LinkedIn</span>"));
        assert!(out.contains("aria-label=\"Share on LinkedIn (opens in a new tab)\""));
    }
}
