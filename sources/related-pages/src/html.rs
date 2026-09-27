//! Reading the rendered page and writing the block into it.

use crate::settings::{Settings, ShowOn, Style};
use crate::similar::Pick;

/// The marker that makes the hook idempotent.
const MARKER: &str = "stride-rel";

/// Whether this page should get a block at all, before any work is done.
pub fn wants_block(html: &str, slug: &str, settings: &Settings) -> bool {
    if !settings.enabled
        || slug == "home"
        || settings.exclude.iter().any(|s| s == slug)
        || !settings.in_folders(slug)
        || html.contains(MARKER)
    {
        return false;
    }
    let bytes = html.as_bytes();
    if find_ci(bytes, b"<body").is_none() {
        return false;
    }
    match settings.show_on {
        ShowOn::All => true,
        ShowOn::Articles => {
            slug.contains('/')
                || find_ci(bytes, b"<article").is_some()
                || prose_words(bytes, ARTICLE_WORDS) >= ARTICLE_WORDS
        }
    }
}

/// Paragraph words that make a page read as an article. An about page is
/// usually about a hundred; Stride's own article template is over two hundred.
const ARTICLE_WORDS: usize = 175;

/// Words inside `<p>` elements, counted up to `limit`.
fn prose_words(bytes: &[u8], limit: usize) -> usize {
    let mut words = 0;
    let mut from = 0;
    while let Some(rel) = find_ci(&bytes[from..], b"<p") {
        let open = from + rel;
        from = open + 2;
        if !matches!(bytes.get(open + 2), Some(b'>' | b' ' | b'\t' | b'\n')) {
            continue;
        }
        let Some(start) = bytes[from..].iter().position(|&c| c == b'>').map(|i| from + i + 1) else {
            break;
        };
        from = start;
        let Some(close) = find_ci(&bytes[from..], b"</p>").map(|i| from + i) else { break };
        let (mut in_tag, mut in_word) = (false, false);
        for &c in &bytes[from..close] {
            match c {
                b'<' => in_tag = true,
                b'>' => {
                    in_tag = false;
                    in_word = false;
                }
                _ if in_tag => {}
                c if c.is_ascii_whitespace() => in_word = false,
                _ => {
                    if !in_word {
                        words += 1;
                        in_word = true;
                    }
                }
            }
        }
        if words >= limit {
            return words;
        }
        from = close + 4;
    }
    words
}

/// The page's `<title>`, without a trailing " | Site name".
pub fn document_title(html: &str) -> String {
    let bytes = html.as_bytes();
    let Some(open) = find_ci(bytes, b"<title") else { return String::new() };
    let Some(start) = html[open..].find('>').map(|i| open + i + 1) else { return String::new() };
    let Some(end) = find_ci(&bytes[start..], b"</title>").map(|i| start + i) else {
        return String::new();
    };
    let title = html[start..end].replace("&amp;", "&");
    [" | ", " · ", " — ", " – ", " - "]
        .iter()
        .find_map(|sep| title.split_once(sep).map(|(first, _)| first.to_owned()))
        .unwrap_or(title)
        .trim()
        .to_owned()
}

/// The page with the block added, or `None` when there is nothing to add or
/// nowhere sensible to add it.
pub fn insert(html: &str, picks: &[Pick], settings: &Settings) -> Option<String> {
    if picks.is_empty() {
        return None;
    }
    let bytes = html.as_bytes();
    let body = find_ci(bytes, b"<body")?;
    let body_close = rfind_ci(bytes, b"</body>").filter(|&at| at > body)?;
    // At the end of the article when there is one, else of the main content,
    // else just above the site footer, else at the end of the page.
    let within = |at: usize| at > body && at < body_close;
    let at = rfind_ci(bytes, b"</article>")
        .filter(|&at| within(at))
        .or_else(|| rfind_ci(bytes, b"</main>").filter(|&at| within(at)))
        .or_else(|| rfind_ci(bytes, b"<footer").filter(|&at| within(at)))
        .unwrap_or(body_close);
    let head_close = find_ci(bytes, b"</head>").filter(|&h| h < body);

    let style = style(settings);
    let block = block(picks, settings);
    let mut out = String::with_capacity(html.len() + style.len() + block.len());
    match head_close {
        Some(h) => {
            out.push_str(&html[..h]);
            out.push_str(&style);
            out.push_str(&html[h..at]);
        }
        None => {
            out.push_str(&html[..at]);
            out.push_str(&style);
        }
    }
    out.push_str(&block);
    out.push_str(&html[at..]);
    Some(out)
}

const ARROW: &str = "<svg class=\"stride-rel-a\" viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" \
aria-hidden=\"true\" focusable=\"false\"><path d=\"M5 12h14M13 6l6 6-6 6\" fill=\"none\" \
stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

fn block(picks: &[Pick], settings: &Settings) -> String {
    let style = match settings.style {
        Style::Cards => "stride-rel-cards",
        Style::List => "stride-rel-list",
    };
    let mut out = format!(
        "<aside class=\"stride-rel {style}\" aria-labelledby=\"stride-rel-h\">\
<h2 class=\"stride-rel-h\" id=\"stride-rel-h\">{}</h2><ul class=\"stride-rel-u\" role=\"list\">",
        escape(settings.effective_heading())
    );
    for pick in picks {
        out.push_str("<li><a class=\"stride-rel-link\" href=\"/");
        out.push_str(&escape(&pick.slug));
        out.push_str("\">");
        if !pick.label.is_empty() {
            out.push_str("<span class=\"stride-rel-k\">");
            out.push_str(&escape(&pick.label));
            out.push_str("</span>");
        }
        out.push_str("<span class=\"stride-rel-t\">");
        out.push_str(&escape(&pick.title));
        out.push_str("</span>");
        out.push_str(ARROW);
        out.push_str("</a></li>");
    }
    out.push_str("</ul></aside>");
    out
}

fn style(settings: &Settings) -> String {
    let c = settings.effective_color();
    format!(
        "<style>.stride-rel{{--stride-rel:{c};box-sizing:border-box;clear:both;max-width:712px;\
margin:56px auto;padding:0 16px;color:inherit;font:inherit}}\
.stride-rel *{{box-sizing:border-box}}\
.stride-rel-h{{margin:0 0 16px;font-size:1.25rem;line-height:1.3;font-weight:700;letter-spacing:-.01em}}\
.stride-rel-u{{list-style:none;margin:0;padding:0}}\
.stride-rel-link{{color:inherit;text-decoration:none}}\
.stride-rel-link:focus-visible{{outline:2px solid var(--stride-rel);outline-offset:3px}}\
.stride-rel-k{{display:block;font-size:.75rem;line-height:1.4;font-weight:600;letter-spacing:.06em;\
text-transform:uppercase;color:var(--stride-rel)}}\
.stride-rel-t{{font-weight:600;line-height:1.35}}\
.stride-rel-a{{flex:none;color:var(--stride-rel);transition:transform .15s}}\
.stride-rel-link:hover .stride-rel-a{{transform:translateX(3px)}}\
.stride-rel-cards .stride-rel-u{{display:grid;gap:16px;\
grid-template-columns:repeat(auto-fill,minmax(min(100%,200px),1fr))}}\
.stride-rel-cards li{{display:flex}}\
.stride-rel-cards .stride-rel-link{{display:flex;flex-direction:column;gap:8px;width:100%;\
padding:20px;border:1px solid rgba(127,127,127,.28);border-radius:12px;\
transition:border-color .15s,box-shadow .15s,transform .15s}}\
.stride-rel-cards .stride-rel-link:hover{{border-color:var(--stride-rel);\
box-shadow:0 8px 24px rgba(0,0,0,.08);transform:translateY(-2px)}}\
.stride-rel-cards .stride-rel-t{{font-size:1.0625rem}}\
.stride-rel-cards .stride-rel-a{{margin-top:auto}}\
.stride-rel-list .stride-rel-u{{border-top:1px solid rgba(127,127,127,.28)}}\
.stride-rel-list li{{border-bottom:1px solid rgba(127,127,127,.28)}}\
.stride-rel-list .stride-rel-link{{display:flex;align-items:center;gap:16px;padding:14px 4px}}\
.stride-rel-list .stride-rel-k{{order:2;margin-left:auto;text-align:right}}\
.stride-rel-list .stride-rel-a{{order:3}}\
.stride-rel-list .stride-rel-link:hover .stride-rel-t{{text-decoration:underline;\
text-decoration-color:var(--stride-rel);text-underline-offset:3px}}\
@media (max-width:540px){{.stride-rel-list .stride-rel-k{{display:none}}}}\
@media (prefers-reduced-motion:reduce){{.stride-rel-link,.stride-rel-a{{transition:none}}\
.stride-rel-cards .stride-rel-link:hover{{transform:none}}}}\
@media print{{.stride-rel{{display:none}}}}</style>"
    )
}

/// Text and attribute values alike.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
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

    const PAGE: &str = "<!doctype html><html><head><title>Sourdough starter | Crumb</title></head>\
<body><main><article><h1>Hi</h1></article></main><footer>f</footer></body></html>";

    fn picks() -> Vec<Pick> {
        vec![Pick { slug: "blog/b\"x".into(), title: "<b>Rye</b> & bread".into(), label: "Blog".into() }]
    }

    #[test]
    fn inserts_escaped_block_at_the_end_of_the_article() {
        let s = Settings::default();
        let html = insert(PAGE, &picks(), &s).unwrap();
        assert!(html.contains("</style></head>"));
        assert!(html.contains("</ul></aside></article></main>"));
        assert!(html.contains("href=\"/blog/b&quot;x\""));
        assert!(html.contains("&lt;b&gt;Rye&lt;/b&gt; &amp; bread"));
        assert!(html.contains(">Related reading</h2>"));
        assert!(!wants_block(&html, "blog/a", &s), "idempotent");
        assert!(insert(PAGE, &[], &s).is_none());
    }

    #[test]
    fn falls_back_to_main_footer_then_body() {
        let s = Settings::default();
        let p = "<html><body><main>m</main><footer>f</footer></body></html>";
        assert!(insert(p, &picks(), &s).unwrap().contains("</aside></main>"));
        let p = "<html><body><div>m</div><footer>f</footer></body></html>";
        assert!(insert(p, &picks(), &s).unwrap().contains("</aside><footer>"));
        let p = "<html><body><div>m</div></body></html>";
        assert!(insert(p, &picks(), &s).unwrap().contains("</aside></body>"));
    }

    #[test]
    fn decides_which_pages_get_a_block() {
        let s = Settings::default();
        assert!(wants_block(PAGE, "anything", &s));
        let plain = "<html><body><main>m</main></body></html>";
        assert!(wants_block(plain, "blog/a", &s));
        assert!(!wants_block(plain, "about", &s));
        let long = format!("<html><body><p>{}</p></body></html>", "word ".repeat(200));
        assert!(wants_block(&long, "essay", &s));
        assert_eq!(prose_words(b"<p>a <b>b</b>c</p><pre>x y</pre><p class=x>d</p>", 99), 4);
        assert!(!wants_block(PAGE, "home", &s));
        assert!(!wants_block("<p>fragment</p>", "blog/a", &s));
        let all = Settings { show_on: ShowOn::All, ..Settings::default() };
        assert!(wants_block(plain, "about", &all));
        let off = Settings { enabled: false, ..Settings::default() };
        assert!(!wants_block(PAGE, "blog/a", &off));
    }

    #[test]
    fn reads_the_title() {
        assert_eq!(document_title(PAGE), "Sourdough starter");
        assert_eq!(document_title("<title>A &amp; B</title>"), "A & B");
        assert_eq!(document_title("<p>none</p>"), "");
    }
}
