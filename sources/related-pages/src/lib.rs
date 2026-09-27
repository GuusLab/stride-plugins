//! Related Pages: a Stride plugin.
//!
//! At the end of articles and posts it adds a "Related reading" block: the
//! published pages of the same site that share the most topics or title
//! words with the page being rendered, as cards or as a list.
//!
//! It asks for `read-pages`, to list the site's published pages (slug, title,
//! date), and `storage`, to keep its settings. Without `read-pages` there is
//! nothing to suggest and pages are left exactly as rendered; without
//! `storage` it runs with the defaults.
//!
//! Stride keeps a post's tags where no plugin permission reaches them, so
//! "topics" are the ones typed into this plugin's own settings.

use extism_pdk::{FnResult, Json, plugin_fn};
use stride_pdk::{
    JsonValue, Map, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse, kv,
};

mod html;
mod settings;
mod similar;

pub use settings::{Settings, ShowOn, Style};
pub use similar::{Candidate, Pick, pick};

const SETTINGS_KEY: &str = "settings";
/// Enough for any site this is useful on, and a bound on the work a render
/// may do inside its 50 ms.
const MAX_CANDIDATES: usize = 500;

// ---------------------------------------------------------------- the hook

#[plugin_fn]
pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
    let settings = load_settings();
    if !html::wants_block(&page.html, &page.slug, &settings) {
        return Ok(Json(PageRendered { html: page.html }));
    }
    let pages = match list_pages(&page.site_id) {
        Ok(pages) => pages,
        // Refused read-pages is an answer: with nothing to suggest, change nothing.
        Err(error) if error.is_permission_denied() => {
            return Ok(Json(PageRendered { html: page.html }));
        }
        Err(error) => {
            stride_pdk::log("info", &format!("pages unreadable, block skipped: {error}"));
            return Ok(Json(PageRendered { html: page.html }));
        }
    };
    let title = pages
        .iter()
        .find(|c| c.slug == page.slug)
        .map(|c| c.title.clone())
        .unwrap_or_else(|| html::document_title(&page.html));
    let picks = pick(&page.slug, &title, &pages, &settings);
    let out = html::insert(&page.html, &picks, &settings).unwrap_or(page.html);
    Ok(Json(PageRendered { html: out }))
}

fn load_settings() -> Settings {
    match kv::get::<JsonValue>(SETTINGS_KEY) {
        Ok(Some(value)) => Settings::from_json(&value),
        Ok(None) => Settings::default(),
        Err(error) if error.is_permission_denied() => Settings::default(),
        Err(error) => {
            stride_pdk::log("info", &format!("settings unreadable, using defaults: {error}"));
            Settings::default()
        }
    }
}

fn list_pages(site_id: &str) -> Result<Vec<Candidate>, stride_pdk::HostError> {
    let listed: JsonValue = stride_pdk::action(
        "documents.list",
        &stride_pdk::serde_json::json!({
            "siteId": site_id,
            "kind": "page",
            "status": "published",
        }),
    )?;
    Ok(listed
        .get("documents")
        .and_then(JsonValue::as_array)
        .map(|docs| docs.iter().filter_map(Candidate::from_json).take(MAX_CANDIDATES).collect())
        .unwrap_or_default())
}

// --------------------------------------------------------------- the panel

#[plugin_fn]
pub fn panel_related_pages(Json(request): Json<PanelRequest>) -> FnResult<Json<PanelResponse>> {
    let stored = kv::get::<JsonValue>(SETTINGS_KEY);
    let current = match &stored {
        Ok(Some(value)) => Settings::from_json(value),
        _ => Settings::default(),
    };
    let respond = |values: Map<String, JsonValue>, message: String, error: String| {
        Ok(Json(PanelResponse { values, message, error }))
    };
    match request.event {
        PanelEvent::Load => {
            let mut notes = Vec::new();
            if matches!(&stored, Err(e) if e.is_permission_denied()) {
                notes.push(
                    "This plugin was not granted storage, so these settings cannot be saved. \
                     It runs with the defaults shown here.",
                );
            }
            if let Err(error) = list_pages(&request.site_id) {
                if error.is_permission_denied() {
                    notes.push(
                        "This plugin was not granted permission to read pages, so it cannot \
                         see which pages to suggest and adds nothing. Grant it in Plugins to \
                         turn the block on.",
                    );
                }
            }
            respond(current.to_values(), notes.join(" "), String::new())
        }
        PanelEvent::Submit => {
            let submitted = match Settings::from_values(&request.values) {
                Ok(settings) => settings,
                Err(error) => return respond(current.to_values(), String::new(), error),
            };
            if let Err(error) = kv::set(SETTINGS_KEY, &submitted.to_json()) {
                let error = if error.is_permission_denied() {
                    "This plugin was not granted the storage permission, so there is nowhere \
                     to keep these settings. Nothing was saved; the defaults stay in effect."
                        .to_owned()
                } else {
                    error.to_string()
                };
                return respond(submitted.to_values(), String::new(), error);
            }
            let message = if submitted.enabled {
                "Saved. Publish the site again to update every page that is already live."
            } else {
                "Saved. Related pages are off. Publish the site again to remove the block \
                 from pages that are already live."
            };
            respond(submitted.to_values(), message.to_owned(), String::new())
        }
    }
}
