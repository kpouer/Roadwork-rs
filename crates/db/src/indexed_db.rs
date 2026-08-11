//! Minimal IndexedDB persistence for the wasm32 build, driving `web-sys`
//! directly. Records are plain objects `{ fetched_at?, payload: Uint8Array }`
//! keyed by service name.

use js_sys::{Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, IdbDatabase, IdbFactory, IdbOpenDbRequest, IdbRequest, IdbTransaction,
    IdbTransactionMode, IdbVersionChangeEvent, Window,
};

const DB_NAME: &str = "roadwork";
const DB_VERSION: u32 = 2;

pub async fn open() -> Result<IdbDatabase, JsValue> {
    let window: Window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let factory: IdbFactory = window
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("indexed db unavailable"))?;
    let request: IdbOpenDbRequest = factory.open_with_u32(DB_NAME, DB_VERSION)?;

    let upgrade = Closure::once(move |event: IdbVersionChangeEvent| {
        let Some(target) = event.target() else {
            return;
        };
        let Ok(request) = target.dyn_into::<IdbRequest>() else {
            return;
        };
        let Ok(result) = request.result() else {
            return;
        };
        let Ok(db) = result.dyn_into::<IdbDatabase>() else {
            return;
        };
        let names = db.object_store_names();
        if !names.contains(super::ROADWORK_STORE) {
            let _ = db.create_object_store(super::ROADWORK_STORE);
        }
        if !names.contains(super::OPENDATA_STORE) {
            let _ = db.create_object_store(super::OPENDATA_STORE);
        }
        if !names.contains(super::KV_STORE) {
            let _ = db.create_object_store(super::KV_STORE);
        }
    });
    request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    upgrade.forget();

    let request: IdbRequest = request.unchecked_into();
    let value = JsFuture::from(request_promise(request)).await?;
    value.dyn_into()
}
pub async fn put(
    db: &IdbDatabase,
    store_name: &str,
    key: &str,
    value: &JsValue,
) -> Result<(), JsValue> {
    let tx = readwrite_tx(db, store_name)?;
    let complete = tx_complete_promise(&tx)?;
    let store = tx.object_store(store_name)?;
    let request = store.put_with_key(value, &JsValue::from_str(key))?;
    JsFuture::from(request_promise(request)).await?;
    JsFuture::from(complete).await?;
    Ok(())
}

pub async fn get(
    db: &IdbDatabase,
    store_name: &str,
    key: &str,
) -> Result<Option<JsValue>, JsValue> {
    let tx = readonly_tx(db, store_name)?;
    let store = tx.object_store(store_name)?;
    let request = store.get(&JsValue::from_str(key))?;
    let value = JsFuture::from(request_promise(request)).await?;
    if value.is_undefined() || value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub async fn delete(db: &IdbDatabase, store_name: &str, key: &str) -> Result<(), JsValue> {
    let tx = readwrite_tx(db, store_name)?;
    let complete = tx_complete_promise(&tx)?;
    let store = tx.object_store(store_name)?;
    let request = store.delete(&JsValue::from_str(key))?;
    JsFuture::from(request_promise(request)).await?;
    JsFuture::from(complete).await?;
    Ok(())
}

pub async fn clear(db: &IdbDatabase, store_name: &str) -> Result<(), JsValue> {
    let tx = readwrite_tx(db, store_name)?;
    let complete = tx_complete_promise(&tx)?;
    let store = tx.object_store(store_name)?;
    let request = store.clear()?;
    JsFuture::from(request_promise(request)).await?;
    JsFuture::from(complete).await?;
    Ok(())
}

fn readonly_tx(db: &IdbDatabase, store_name: &str) -> Result<IdbTransaction, JsValue> {
    tx_with_mode(db, store_name, IdbTransactionMode::Readonly)
}

fn readwrite_tx(db: &IdbDatabase, store_name: &str) -> Result<IdbTransaction, JsValue> {
    tx_with_mode(db, store_name, IdbTransactionMode::Readwrite)
}

fn tx_with_mode(
    db: &IdbDatabase,
    store_name: &str,
    mode: IdbTransactionMode,
) -> Result<IdbTransaction, JsValue> {
    let stores = js_sys::Array::of1(&JsValue::from_str(store_name));
    db.transaction_with_str_sequence_and_mode(stores.as_ref(), mode)
}

/// Builds the stored record: `{ fetched_at?, payload }`.
pub fn record(fetched_at: Option<i64>, payload: &[u8]) -> JsValue {
    let obj = js_sys::Object::new();
    if let Some(fetched_at) = fetched_at {
        let _ = Reflect::set(
            &obj,
            &"fetched_at".into(),
            &JsValue::from_f64(fetched_at as f64),
        );
    }
    let _ = Reflect::set(&obj, &"payload".into(), &Uint8Array::from(payload));
    obj.into()
}

/// Reads `{ fetched_at?, payload }` back into Rust.
pub fn decode_record(value: &JsValue) -> Result<(Option<i64>, Vec<u8>), JsValue> {
    let fetched_at = Reflect::get(value, &"fetched_at".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.as_f64())
        .map(|v| v as i64);
    let payload = Reflect::get(value, &"payload".into())?;
    let bytes = Uint8Array::new(&payload).to_vec();
    Ok((fetched_at, bytes))
}

/// Wraps an `IDBRequest` in a JS promise resolving with its result.
fn request_promise(request: IdbRequest) -> Promise {
    Promise::new(&mut move |resolve: Function, reject: Function| {
        let ok_request = request.clone();
        let ok_resolve = resolve.clone();
        let ok_reject = reject.clone();
        let ok = Closure::once(move |_event: Event| match ok_request.result() {
            Ok(value) => {
                let _ = ok_resolve.call1(&JsValue::UNDEFINED, &value);
            }
            Err(e) => {
                let _ = ok_reject.call1(&JsValue::UNDEFINED, &e);
            }
        });
        request.set_onsuccess(Some(ok.as_ref().unchecked_ref()));
        ok.forget();

        let err_request = request.clone();
        let err_reject = reject.clone();
        let err = Closure::once(move |_event: Event| {
            let error = request_error(&err_request);
            let _ = err_reject.call1(&JsValue::UNDEFINED, &error);
        });
        request.set_onerror(Some(err.as_ref().unchecked_ref()));
        err.forget();
    })
}

/// Wraps an `IDBTransaction` in a JS promise resolving on `complete`.
fn tx_complete_promise(tx: &IdbTransaction) -> Result<Promise, JsValue> {
    let tx_ok = tx.clone();
    Ok(Promise::new(
        &mut move |resolve: Function, reject: Function| {
            let resolve = resolve.clone();
            let ok = Closure::once(move |_event: Event| {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            });
            tx_ok.set_oncomplete(Some(ok.as_ref().unchecked_ref()));
            ok.forget();

            let tx_err = tx_ok.clone();
            let err_reject = reject.clone();
            let err_tx = tx_err.clone();
            let err = Closure::once(move |_event: Event| {
                let error = err_tx
                    .error()
                    .map(|e| e.name().into())
                    .unwrap_or_else(|| JsValue::from_str("indexeddb transaction failed"));
                let _ = err_reject.call1(&JsValue::UNDEFINED, &error);
            });
            tx_err.set_onerror(Some(err.as_ref().unchecked_ref()));
            err.forget();
        },
    ))
}

fn request_error(request: &IdbRequest) -> JsValue {
    request
        .error()
        .ok()
        .flatten()
        .map(|e| e.name().into())
        .unwrap_or_else(|| JsValue::from_str("indexeddb request failed"))
}
