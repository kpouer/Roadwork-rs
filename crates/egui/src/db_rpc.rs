//! RPC client bridging the egui app (extension overlay iframe) to the wasm
//! worker that owns the SQLite store.
//!
//! The app iframe cannot reach the worker's iframe directly (they are
//! cross-origin siblings under the WME page), so it posts `ROADWORK_APP_RPC`
//! messages to `window.parent`. The extension page-world script relays them to
//! the worker through its own `rpcCall`, then posts
//! `ROADWORK_APP_RPC_RESULT` / `ROADWORK_APP_RPC_ERROR` back to this iframe,
//! which settles the pending JS promise.

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

struct Pending {
    resolve: js_sys::Function,
    reject: js_sys::Function,
}

thread_local! {
    static NEXT_ID: RefCell<u32> = const { RefCell::new(0) };
    #[allow(clippy::missing_const_for_thread_local)]
    static PENDING: RefCell<HashMap<u32, Pending>> = RefCell::new(HashMap::new());
    #[allow(clippy::missing_const_for_thread_local, clippy::type_complexity)]
    static LISTENER: RefCell<Option<Closure<dyn FnMut(web_sys::MessageEvent)>>> =
        RefCell::new(None);
}

/// Registers the single `message` listener that settles pending RPC calls.
fn ensure_listener() {
    if LISTENER.with(|cell| cell.borrow().is_some()) {
        return;
    }
    let window = web_sys::window().expect("No window");
    let closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let Ok(obj) = event.data().dyn_into::<js_sys::Object>() else {
            return;
        };
        let Some(msg_type) = js_sys::Reflect::get(&obj, &js_sys::JsString::from("type"))
            .ok()
            .and_then(|v| v.as_string())
        else {
            return;
        };
        let Some(id) = js_sys::Reflect::get(&obj, &js_sys::JsString::from("id"))
            .ok()
            .and_then(|v| v.as_f64())
            .map(|f| f as u32)
        else {
            return;
        };
        let Some(pending) = PENDING.with(|cell| cell.borrow_mut().remove(&id)) else {
            return;
        };
        match msg_type.as_str() {
            "ROADWORK_APP_RPC_RESULT" => {
                let result = js_sys::Reflect::get(&obj, &js_sys::JsString::from("result"))
                    .unwrap_or(JsValue::NULL);
                let _ = pending.resolve.call1(&JsValue::UNDEFINED, &result);
            }
            "ROADWORK_APP_RPC_ERROR" => {
                let error = js_sys::Reflect::get(&obj, &js_sys::JsString::from("error"))
                    .unwrap_or_else(|_| JsValue::from_str("DB RPC error"));
                let _ = pending.reject.call1(&JsValue::UNDEFINED, &error);
            }
            _ => {}
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    window
        .add_event_listener_with_callback("message", closure.as_ref().unchecked_ref())
        .expect("Failed to add DB RPC message listener");
    LISTENER.with(|cell| *cell.borrow_mut() = Some(closure));
}

/// Calls `method` on the wasm worker through the extension relay. The JS relay
/// enforces its own 30s timeout, surfaced here as an error.
pub(crate) async fn call(method: &str, args: Vec<JsValue>) -> Result<JsValue, JsValue> {
    ensure_listener();
    let id = NEXT_ID.with(|cell| {
        let mut next = cell.borrow_mut();
        *next += 1;
        *next
    });
    let (resolve, reject, promise) = {
        let mut resolver = None;
        let mut rejecter = None;
        let promise =
            js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
                resolver = Some(resolve);
                rejecter = Some(reject);
            });
        (
            resolver.expect("Promise resolver was not called"),
            rejecter.expect("Promise rejecter was not called"),
            promise,
        )
    };
    PENDING.with(|cell| {
        cell.borrow_mut().insert(id, Pending { resolve, reject });
    });

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &js_sys::JsString::from("type"),
        &js_sys::JsString::from("ROADWORK_APP_RPC"),
    )
    .ok();
    js_sys::Reflect::set(
        &obj,
        &js_sys::JsString::from("id"),
        &JsValue::from_f64(id as f64),
    )
    .ok();
    js_sys::Reflect::set(
        &obj,
        &js_sys::JsString::from("method"),
        &JsValue::from_str(method),
    )
    .ok();
    let args_array = js_sys::Array::new();
    for arg in args {
        args_array.push(&arg);
    }
    js_sys::Reflect::set(&obj, &js_sys::JsString::from("args"), &args_array).ok();

    let parent = web_sys::window()
        .and_then(|w| w.parent().ok().flatten())
        .ok_or_else(|| JsValue::from_str("No parent window"))?;
    parent.post_message(&obj, "*").ok();

    JsFuture::from(promise).await
}

/// Calls an RPC and deserializes the result through `JSON.stringify`.
pub(crate) async fn rpc_json<T>(method: &str, args: Vec<JsValue>) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value = call(method, args).await.map_err(|e| {
        e.as_string()
            .unwrap_or_else(|| format!("Erreur RPC: {e:?}"))
    })?;
    let json =
        js_sys::JSON::stringify(&value).map_err(|e| format!("Erreur de sérialisation: {e:?}"))?;
    let s = json.as_string().unwrap_or_default();
    serde_json::from_str(&s).map_err(|e| format!("Erreur de décodage: {e}"))
}
