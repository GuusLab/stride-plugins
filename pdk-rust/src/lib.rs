//! The Rust PDK: what a Stride plugin is written against.
//!
//! A plugin is a WebAssembly module. This crate is the thin layer between
//! that and Rust: the types a hook receives and returns, the six host
//! functions, and one error type that tells a plugin *why* the host said no.
//!
//! ```no_run
//! use stride_pdk::{FnResult, Json, PageRender, PageRendered, plugin_fn};
//!
//! #[plugin_fn]
//! pub fn on_page_render(Json(page): Json<PageRender>) -> FnResult<Json<PageRendered>> {
//!     Ok(Json(PageRendered {
//!         html: format!("{}\n<!-- {} -->", page.html, page.slug),
//!     }))
//! }
//! ```
//!
//! # Permissions are answers, not crashes
//!
//! Every host call returns a [`Result`]. A plugin that was not granted
//! `storage` gets `Err(HostError { code: "permission-denied", .. })` from
//! [`kv::get`] — it is not killed, and it does not have to be. Write the
//! degraded path and your plugin stays installable by someone who will not
//! grant everything:
//!
//! ```no_run
//! # use stride_pdk::kv;
//! # fn example() -> String {
//! match kv::get::<String>("message") {
//!     Ok(Some(message)) => message,
//!     Ok(None) => "Welcome".to_owned(),
//!     Err(error) if error.is_permission_denied() => "Welcome".to_owned(),
//!     Err(error) => {
//!         // Say so and carry on. Returning is how a hook fails safely.
//!         stride_pdk::log("warn", &error.to_string());
//!         "Welcome".to_owned()
//!     }
//! }
//! # }
//! ```
//!
//! # Do not panic
//!
//! A panic in a guest is a WebAssembly trap. The host contains it — the page
//! is still served — but it costs your plugin a strike against the circuit
//! breaker, and five in a row disables it. `unwrap()` is how that happens by
//! accident.

use extism_pdk::host_fn;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use extism_pdk::{FnResult, Json, WithReturnCode, plugin_fn};
/// Re-exported so a plugin needs one dependency rather than three.
pub use {serde, serde_json};
/// The two `serde_json` types the hook signatures use.
pub use serde_json::{Map, Value as JsonValue};

/// A JSON object: `Map<String, JsonValue>`, written once.
///
/// `serde_json::Map` is generic in both parameters even though only one pair
/// is ever instantiated, so `Map::new()` compiles when the surrounding code
/// pins the types and fails with a type-annotation error when it does not —
/// which is the first thing a plugin author hits, in a line copied verbatim
/// out of the documentation. `Values::new()` never needs the annotation.
pub type Values = Map<String, JsonValue>;

mod types;
pub use types::{
    DocumentSave, DocumentSaved, PageRender, PageRendered, PanelEvent, PanelRequest, PanelResponse,
    Published,
};

#[host_fn]
unsafe extern "ExtismHost" {
    fn stride_action(request: String) -> String;
    fn stride_kv_get(request: String) -> String;
    fn stride_kv_set(request: String) -> String;
    fn stride_kv_delete(request: String) -> String;
    fn stride_kv_list(request: String) -> String;
    fn stride_log(request: String) -> String;
}

/// Why the host would not do what was asked.
///
/// `code` is stable and worth branching on; `message` is written for a person
/// reading a log.
///
/// | code | what happened |
/// |---|---|
/// | `permission-denied` | this plugin was not granted what the call needs |
/// | `refused` | granted, but not for this: an action no plugin may call, or no site |
/// | `action-failed` | the action ran and failed; `status` is the HTTP status a person would have got |
/// | `quota-exceeded` | a storage or logging limit |
/// | `bad-request` | the host could not read what the guest sent |
/// | `host-failed` | something broke on the host side |
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostError {
    pub code: String,
    #[serde(default)]
    pub message: String,
    /// The permission that would have unlocked it, when there is one.
    #[serde(default)]
    pub permission: Option<String>,
    /// For `action-failed`: the HTTP status the same failure gives a person.
    #[serde(default)]
    pub status: Option<u16>,
}

impl HostError {
    /// Whether asking for a permission in the manifest — and being granted it
    /// — would have made this call work.
    pub fn is_permission_denied(&self) -> bool {
        self.code == "permission-denied"
    }

    fn host_failed(message: impl Into<String>) -> Self {
        Self {
            code: "host-failed".into(),
            message: message.into(),
            permission: None,
            status: None,
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for HostError {}

/// The envelope every host function answers with.
#[derive(serde::Deserialize)]
enum Reply {
    #[serde(rename = "ok")]
    Ok(Value),
    #[serde(rename = "error")]
    Err(HostError),
}

fn call(host: impl Fn(String) -> Result<String, extism_pdk::Error>, request: Value) -> Result<Value, HostError> {
    let encoded = serde_json::to_string(&request)
        .map_err(|error| HostError::host_failed(format!("the request would not encode: {error}")))?;
    let raw = host(encoded)
        .map_err(|error| HostError::host_failed(format!("the host call failed: {error}")))?;
    match serde_json::from_str::<Reply>(&raw) {
        Ok(Reply::Ok(value)) => Ok(value),
        Ok(Reply::Err(error)) => Err(error),
        Err(error) => Err(HostError::host_failed(format!(
            "the host answered with something this PDK cannot read: {error}"
        ))),
    }
}

/// Perform an action from Stride's action registry, as this plugin.
///
/// The plugin's own permissions decide which actions it may call, and the
/// registry applies the same check it applies to a person. See
/// `docs/plugins.md` for the mapping from permission to action.
///
/// ```no_run
/// # use serde::Deserialize;
/// #[derive(Deserialize)]
/// struct Documents { documents: Vec<serde_json::Value> }
///
/// let listed: Documents = stride_pdk::action("documents.list", &serde_json::json!({
///     "siteId": "site-1"
/// }))?;
/// # Ok::<(), stride_pdk::HostError>(())
/// ```
pub fn action<I: Serialize, O: DeserializeOwned>(name: &str, input: &I) -> Result<O, HostError> {
    let input = serde_json::to_value(input)
        .map_err(|error| HostError::host_failed(format!("the input would not encode: {error}")))?;
    let value = call(
        |request| unsafe { stride_action(request) },
        serde_json::json!({"action": name, "input": input}),
    )?;
    serde_json::from_value(value).map_err(|error| {
        HostError::host_failed(format!("the action's output would not decode: {error}"))
    })
}

/// The plugin's own storage. Needs the `storage` permission.
///
/// Data is per site: a plugin serving three sites has three bags, and a hook
/// reads the one belonging to the site it is running for.
pub mod kv {
    use super::{DeserializeOwned, HostError, Serialize, Value, call};

    /// Read one value. `Ok(None)` when there is nothing under that key.
    pub fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>, HostError> {
        let value = call(
            |request| unsafe { super::stride_kv_get(request) },
            serde_json::json!({"key": key}),
        )?;
        if value.is_null() {
            return Ok(None);
        }
        serde_json::from_value(value)
            .map(Some)
            .map_err(|error| HostError::host_failed(format!("the stored value would not decode: {error}")))
    }

    /// Write one value. At most 64 KiB of JSON, and 256 keys per site.
    pub fn set<T: Serialize>(key: &str, value: &T) -> Result<(), HostError> {
        let value: Value = serde_json::to_value(value).map_err(|error| {
            HostError {
                code: "bad-request".into(),
                message: format!("the value would not encode: {error}"),
                permission: None,
                status: None,
            }
        })?;
        call(
            |request| unsafe { super::stride_kv_set(request) },
            serde_json::json!({"key": key, "value": value}),
        )?;
        Ok(())
    }

    /// Delete one. `Ok(false)` when there was nothing there.
    pub fn delete(key: &str) -> Result<bool, HostError> {
        let value = call(
            |request| unsafe { super::stride_kv_delete(request) },
            serde_json::json!({"key": key}),
        )?;
        Ok(value.as_bool().unwrap_or(false))
    }

    /// Every key under `prefix`, sorted.
    pub fn list(prefix: &str) -> Result<Vec<String>, HostError> {
        let value = call(
            |request| unsafe { super::stride_kv_list(request) },
            serde_json::json!({"prefix": prefix}),
        )?;
        Ok(value
            .as_array()
            .map(|keys| {
                keys.iter()
                    .filter_map(|key| key.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// Write one line to the server log. Needs no permission.
///
/// At most 1024 characters and 20 lines per call; the host truncates and then
/// refuses. A guest cannot read the log back, and the level it asks for is a
/// hint: `warn` and `error` are logged as warnings, everything else as info.
pub fn log(level: &str, message: &str) {
    let _ = call(
        |request| unsafe { stride_log(request) },
        serde_json::json!({"level": level, "message": message}),
    );
}
