//! The wire types, one per hook, matching `stride_plugins`'s own byte for
//! byte. They are duplicated rather than shared because a guest must not
//! depend on the host crate: the host links wasmtime, and a plugin that
//! linked wasmtime would be a very odd plugin.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Input of `on_page_render`: a page that has been rendered and not yet
/// served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRender {
    /// Which site the page belongs to. Storage is per site, so a plugin
    /// serving several needs this to tell them apart.
    pub site_id: String,
    /// The slug, with no leading slash. The home page is `home`.
    pub slug: String,
    /// The HTML as it stands, including whatever earlier plugins did to it.
    pub html: String,
}

/// Output of `on_page_render`: the HTML Stride will serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRendered {
    pub html: String,
}

/// Input of `on_document_save`: the document about to be stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSave {
    pub site_id: String,
    pub document_id: String,
    pub slug: String,
    /// `page`, `template` or `partial`.
    pub kind: String,
    /// The document tree.
    pub document: Value,
}

/// Output of `on_document_save`. `None` — the default — means "I did not
/// change it", which is what a plugin that only wants to look returns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSaved {
    #[serde(default)]
    pub document: Option<Value>,
}

/// Input of `on_publish`: a page that has just gone live. Whatever this hook
/// returns is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    pub site_id: String,
    pub document_id: String,
    pub slug: String,
    pub url: String,
}

/// Why a panel export is being called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelEvent {
    /// The editor opened the panel and wants what to show.
    Load,
    /// Somebody pressed save.
    Submit,
}

/// Input of `panel_<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelRequest {
    pub panel_id: String,
    pub site_id: String,
    pub event: PanelEvent,
    /// Empty on `load`; what the person typed on `submit`.
    #[serde(default)]
    pub values: Map<String, Value>,
}

/// Output of `panel_<id>`.
///
/// Only the fields the manifest declares survive: the host drops anything
/// else before the editor sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelResponse {
    #[serde(default)]
    pub values: Map<String, Value>,
    /// A confirmation to show, or empty.
    #[serde(default)]
    pub message: String,
    /// A refusal to show. Non-empty means the submit did not take effect.
    #[serde(default)]
    pub error: String,
}
