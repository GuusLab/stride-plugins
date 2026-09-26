//! Cookie Consent: a GDPR/AVG cookie banner for every published page.
//!
//! One permission (`storage`, for the panel's settings), one hook and one
//! panel. Without `storage` the banner still works, with its defaults: a
//! site that installed a consent banner should never silently lose it.
//!
//! The visitor's choice lives in a first-party cookie, `stride_consent`, whose
//! value is `n` (necessary only) plus `a` (analytics) and/or `m` (marketing).
//! Other scripts read it from `<html data-cookie-consent="necessary analytics">`
//! or listen for the `stride:consent` event on `document`.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv};

/// All settings live under one key, so a render costs one host call.
const CONFIG: &str = "config";
/// Marks the injected block so a page is never given two banners.
const MARKER: &str = "id=\"scc\"";

const DEFAULT_BACKGROUND: &str = "#0F172A";
const DEFAULT_FOREGROUND: &str = "#F8FAFC";
const DEFAULT_ACCENT: &str = "#0EA5E9";

struct Config {
    enabled: bool,
    bar: bool,
    message_en: String,
    message_nl: String,
    policy_url: String,
    background: String,
    foreground: String,
    accent: String,
    reopen: bool,
}

impl Config {
    fn from_values(values: &Map<String, JsonValue>) -> Self {
        let text = |name: &str| {
            values
                .get(name)
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let flag = |name: &str| values.get(name).and_then(JsonValue::as_bool).unwrap_or(true);
        let colour = |name: &str, fallback: &str| {
            let value = text(name);
            if is_colour(&value) { value } else { fallback.to_owned() }
        };
        let policy = text("policy-url");
        Config {
            enabled: flag("enabled"),
            bar: text("layout") == "bar",
            message_en: text("message-en"),
            message_nl: text("message-nl"),
            policy_url: if is_safe_url(&policy) { policy } else { String::new() },
            background: colour("background", DEFAULT_BACKGROUND),
            foreground: colour("foreground", DEFAULT_FOREGROUND),
            accent: colour("accent", DEFAULT_ACCENT),
            reopen: flag("reopen"),
        }
    }

    fn defaults() -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), true.into());
        values.insert("layout".into(), "card".into());
        values.insert("message-en".into(), "".into());
        values.insert("message-nl".into(), "".into());
        values.insert("policy-url".into(), "".into());
        values.insert("background".into(), DEFAULT_BACKGROUND.into());
        values.insert("foreground".into(), DEFAULT_FOREGROUND.into());
        values.insert("accent".into(), DEFAULT_ACCENT.into());
        values.insert("reopen".into(), true.into());
        values
    }
}

/// The stored settings, or the defaults when there are none or `storage` was
/// refused. Never fails: a missing setting is not a reason to drop a banner.
fn load_values() -> Map<String, JsonValue> {
    let mut values = Config::defaults();
    match kv::get::<Map<String, JsonValue>>(CONFIG) {
        Ok(Some(stored)) => values.extend(stored),
        Ok(None) => {}
        Err(error) if error.is_permission_denied() => {}
        Err(error) => stride_pdk::log("warn", &format!("using default settings: {error}")),
    }
    values
}

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    Ok(Json(PageRendered { html: render(page.html, &Config::from_values(&load_values())) }))
}

fn render(html: String, config: &Config) -> String {
    if !config.enabled || html.contains(MARKER) {
        return html;
    }
    let block = banner(config, is_dutch(&html));
    match rfind_ci(&html, "</body") {
        Some(at) => {
            let mut html = html;
            html.insert_str(at, &block);
            html
        }
        None => html + &block,
    }
}

struct Words {
    title: &'static str,
    message: &'static str,
    policy: &'static str,
    settings: &'static str,
    reject: &'static str,
    accept: &'static str,
    save: &'static str,
    necessary: &'static str,
    necessary_hint: &'static str,
    analytics: &'static str,
    analytics_hint: &'static str,
    marketing: &'static str,
    marketing_hint: &'static str,
    reopen: &'static str,
}

const EN: Words = Words {
    title: "Cookies on this site",
    message: "We use cookies to keep this site working. With your permission we also use them to see how the site is used and to show relevant content. You can change your choice at any time.",
    policy: "Privacy policy",
    settings: "Settings",
    reject: "Reject all",
    accept: "Accept all",
    save: "Save choices",
    necessary: "Necessary",
    necessary_hint: "Needed for the site to work. Always on.",
    analytics: "Analytics",
    analytics_hint: "Anonymous statistics about how the site is used.",
    marketing: "Marketing",
    marketing_hint: "Personalised content and ads, also on other sites.",
    reopen: "Cookie settings",
};

const NL: Words = Words {
    title: "Cookies op deze site",
    message: "We gebruiken cookies om deze site goed te laten werken. Met jouw toestemming gebruiken we ze ook om te zien hoe de site wordt gebruikt en om relevante inhoud te tonen. Je kunt je keuze altijd wijzigen.",
    policy: "Privacyverklaring",
    settings: "Instellingen",
    reject: "Alles weigeren",
    accept: "Alles accepteren",
    save: "Keuze opslaan",
    necessary: "Noodzakelijk",
    necessary_hint: "Nodig om de site te laten werken. Altijd aan.",
    analytics: "Statistieken",
    analytics_hint: "Anonieme statistieken over hoe de site wordt gebruikt.",
    marketing: "Marketing",
    marketing_hint: "Gepersonaliseerde inhoud en advertenties, ook op andere sites.",
    reopen: "Cookie-instellingen",
};

/// The whole script: reads the cookie, sets the attribute, fires the event,
/// wires the buttons. Kept small and free of anything site-specific, so the
/// only interpolation into it is none at all.
const SCRIPT: &str = r#"(function(){var d=document,r=d.documentElement,b=d.getElementById('scc'),o=d.getElementById('scc-r'),p=d.getElementById('scc-p'),g=d.getElementById('scc-g'),A=d.getElementById('scc-a'),M=d.getElementById('scc-m'),x=d.cookie.match(/(?:^|; )stride_consent=(n[am]*)/),v=x?x[1]:'';function c(s){return{necessary:true,analytics:s.indexOf('a')>0,marketing:s.indexOf('m')>0}}function apply(s){var k=c(s);r.setAttribute('data-cookie-consent',s?'necessary'+(k.analytics?' analytics':'')+(k.marketing?' marketing':''):'pending');if(s)d.dispatchEvent(new CustomEvent('stride:consent',{detail:k}))}function show(on){b.hidden=!on;if(o)o.hidden=on||!v}function save(a,m){v='n'+(a?'a':'')+(m?'m':'');d.cookie='stride_consent='+v+';max-age=15552000;path=/;SameSite=Lax'+(location.protocol=='https:'?';Secure':'');apply(v);show(false);if(o)o.focus()}function open(){var k=c(v);A.checked=k.analytics;M.checked=k.marketing;show(true);(b.querySelector('button')||b).focus()}apply(v);show(!v);b.addEventListener('click',function(e){var t=e.target.closest('[data-scc]');if(!t)return;var a=t.getAttribute('data-scc');if(a=='all')save(1,1);else if(a=='none')save(0,0);else if(a=='save')save(A.checked,M.checked);else{p.hidden=!p.hidden;g.hidden=p.hidden;t.setAttribute('aria-expanded',String(!p.hidden))}});d.addEventListener('click',function(e){if(e.target.closest('#scc-r,[data-cookie-settings]')){e.preventDefault();open()}});window.StrideConsent={get:function(){return v?c(v):null},open:open}})();"#;

fn banner(config: &Config, dutch: bool) -> String {
    let w = if dutch { &NL } else { &EN };
    let custom = if dutch { &config.message_nl } else { &config.message_en };
    let message = if custom.is_empty() { w.message.to_owned() } else { escape(custom) };
    let policy = if config.policy_url.is_empty() {
        String::new()
    } else {
        format!(" <a href=\"{}\">{}</a>", escape(&config.policy_url), w.policy)
    };
    let (bg, fg, accent) = (&config.background, &config.foreground, &config.accent);
    let on_accent = readable_on(accent);
    let layout = if config.bar {
        "left:0;right:0;bottom:0;border-radius:0;max-width:none"
    } else {
        "left:16px;right:16px;bottom:16px;max-width:680px;border-radius:16px"
    };
    let reopen = if config.reopen {
        format!("<button type=\"button\" id=\"scc-r\" hidden>{}</button>", w.reopen)
    } else {
        String::new()
    };
    format!(
        "<style>#scc{{position:fixed;z-index:2147483000;{layout};margin:0 auto;box-sizing:border-box;\
background:{bg};color:{fg};padding:20px 24px;box-shadow:0 16px 48px rgba(0,0,0,.28);\
font:15px/1.55 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif;text-align:left}}\
#scc[hidden],#scc [hidden],#scc-r[hidden]{{display:none}}\
#scc h2{{margin:0 0 6px;font:inherit;font-size:17px;font-weight:700;color:inherit}}\
#scc p{{margin:0}}#scc a{{color:inherit;text-decoration:underline;text-underline-offset:2px}}\
#scc .scc-b{{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px;justify-content:flex-end}}\
#scc button{{font:inherit;font-weight:600;line-height:1.2;border-radius:10px;padding:10px 16px;cursor:pointer;\
border:1px solid {fg}55;background:transparent;color:inherit;margin:0}}\
#scc .scc-y{{background:{accent};border-color:{accent};color:{on_accent}}}\
#scc button:focus-visible,#scc input:focus-visible,#scc a:focus-visible,#scc-r:focus-visible{{outline:3px solid {accent};outline-offset:2px}}\
#scc fieldset{{border:0;margin:16px 0 0;padding:0;display:grid;gap:10px}}\
#scc label{{display:flex;gap:10px;align-items:flex-start;cursor:pointer}}#scc label span span{{opacity:.8;display:block;font-size:14px}}\
#scc input{{accent-color:{accent};width:18px;height:18px;margin:3px 0 0;flex:none}}\
#scc-r{{position:fixed;z-index:2147483000;left:16px;bottom:16px;font:600 13px/1 system-ui,-apple-system,sans-serif;\
padding:9px 13px;border-radius:999px;border:0;background:{bg};color:{fg};box-shadow:0 4px 16px rgba(0,0,0,.2);cursor:pointer}}\
@media (max-width:520px){{#scc .scc-b button{{flex:1 1 auto}}}}</style>\
<section id=\"scc\" role=\"dialog\" aria-modal=\"false\" aria-labelledby=\"scc-t\" aria-describedby=\"scc-d\" hidden>\
<h2 id=\"scc-t\">{title}</h2><p id=\"scc-d\">{message}{policy}</p>\
<fieldset id=\"scc-p\" aria-label=\"{settings}\" hidden>\
<label><input type=\"checkbox\" checked disabled><span><strong>{necessary}</strong><span>{necessary_hint}</span></span></label>\
<label><input type=\"checkbox\" id=\"scc-a\"><span><strong>{analytics}</strong><span>{analytics_hint}</span></span></label>\
<label><input type=\"checkbox\" id=\"scc-m\"><span><strong>{marketing}</strong><span>{marketing_hint}</span></span></label>\
</fieldset><div class=\"scc-b\">\
<button type=\"button\" data-scc=\"prefs\" aria-expanded=\"false\" aria-controls=\"scc-p\">{settings}</button>\
<button type=\"button\" data-scc=\"save\" id=\"scc-g\" hidden>{save}</button>\
<button type=\"button\" class=\"scc-y\" data-scc=\"none\">{reject}</button>\
<button type=\"button\" class=\"scc-y\" data-scc=\"all\">{accept}</button>\
</div></section>{reopen}<script>{SCRIPT}</script>",
        title = w.title,
        settings = w.settings,
        necessary = w.necessary,
        necessary_hint = w.necessary_hint,
        analytics = w.analytics,
        analytics_hint = w.analytics_hint,
        marketing = w.marketing,
        marketing_hint = w.marketing_hint,
        save = w.save,
        reject = w.reject,
        accept = w.accept,
    )
}

#[plugin_fn]
pub fn panel_cookie_consent(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    match request.event {
        PanelEvent::Load => Ok(Json(PanelResponse { values: load_values(), ..PanelResponse::default() })),
        PanelEvent::Submit => {
            let mut values = Config::defaults();
            for (key, value) in request.values {
                if values.contains_key(&key) {
                    values.insert(key, value);
                }
            }
            let error = |text: &str| {
                Ok(Json(PanelResponse { error: text.to_owned(), ..PanelResponse::default() }))
            };
            for name in ["background", "foreground", "accent"] {
                let value = values.get(name).and_then(JsonValue::as_str).unwrap_or_default();
                if !value.is_empty() && !is_colour(value) {
                    return error("Colours must be hex values such as #0EA5E9.");
                }
            }
            let policy = values.get("policy-url").and_then(JsonValue::as_str).unwrap_or_default().trim();
            if !is_safe_url(policy) {
                return error("The privacy policy link must be a path such as /privacy or an http(s):// address.");
            }
            if let Err(error) = kv::set(CONFIG, &values) {
                return Ok(Json(PanelResponse {
                    error: if error.is_permission_denied() {
                        "This plugin was not granted the storage permission, so it cannot keep \
                         settings. The banner still shows, with its default text and colours."
                            .to_owned()
                    } else {
                        error.to_string()
                    },
                    ..PanelResponse::default()
                }));
            }
            let on = values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(true);
            Ok(Json(PanelResponse {
                values,
                message: if on {
                    "Saved. Republish or reload a page to see the banner.".to_owned()
                } else {
                    "Saved. The cookie banner is off.".to_owned()
                },
                error: String::new(),
            }))
        }
    }
}

/// `lang` on the `<html>` element starts with `nl`.
fn is_dutch(html: &str) -> bool {
    let Some(start) = find_ci(html, "<html") else { return false };
    let tag = &html[start..];
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    let lower = tag.to_ascii_lowercase();
    let Some(at) = lower.find("lang=") else { return false };
    lower[at + 5..].trim_start_matches(['"', '\'']).starts_with("nl")
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.as_bytes().windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn rfind_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.as_bytes().windows(needle.len()).rposition(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn is_colour(value: &str) -> bool {
    value.len() > 1
        && value.starts_with('#')
        && matches!(value.len() - 1, 3 | 6 | 8)
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Relative paths and http(s) only: never `javascript:` or `data:`.
fn is_safe_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    url.is_empty()
        || (url.starts_with('/') && !url.starts_with("//"))
        || lower.starts_with("https://")
        || lower.starts_with("http://")
}

/// Dark or light text, whichever reads better on `colour` (WCAG luminance).
fn readable_on(colour: &str) -> &'static str {
    let hex = &colour[1..];
    let channel = |i: usize| -> f64 {
        let v = if hex.len() == 3 {
            u8::from_str_radix(&hex[i..i + 1].repeat(2), 16)
        } else {
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
        }
        .unwrap_or(0) as f64
            / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
    // Contrast against white vs. against #0F172A (luminance ~0.0086).
    if 1.05 / (l + 0.05) >= (l + 0.05) / 0.0586 { "#FFFFFF" } else { "#0F172A" }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::from_values(&Config::defaults())
    }

    #[test]
    fn injects_before_body_close_once() {
        let html = render("<html lang=\"en\"><body><p>x</p></body></html>".into(), &config());
        assert!(html.contains("Accept all"));
        assert!(html.ends_with("</script></body></html>"));
        assert_eq!(render(html.clone(), &config()), html);
    }

    #[test]
    fn dutch_by_lang() {
        let html = render("<html lang='nl-NL'><body></body></html>".into(), &config());
        assert!(html.contains("Alles accepteren"));
    }

    #[test]
    fn escapes_and_refuses_bad_values() {
        let mut values = Config::defaults();
        values.insert("message-en".into(), "<b>hi</b>".into());
        values.insert("policy-url".into(), "javascript:alert(1)".into());
        values.insert("accent".into(), "red;}".into());
        let html = render("<body></body>".into(), &Config::from_values(&values));
        assert!(html.contains("&lt;b&gt;hi"));
        assert!(!html.contains("javascript:"));
        assert!(!html.contains("red;}"));
    }

    #[test]
    fn contrast() {
        assert_eq!(readable_on("#0EA5E9"), "#0F172A");
        assert_eq!(readable_on("#1D4ED8"), "#FFFFFF");
        assert_eq!(readable_on("#fff"), "#0F172A");
    }
}
