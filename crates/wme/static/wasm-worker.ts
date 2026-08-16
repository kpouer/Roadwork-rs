// Dedicated worker hosting the Roadwork wasm module.
//
// The OPFS `sahpool` VFS used by SQLite on wasm32 requires a dedicated worker,
// so the wasm module runs here instead of on the extension iframe's main
// thread. The iframe (`wasm-init.js`) is only a thin relay forwarding
// postMessage traffic between this worker and the WME page.

const scope = self as any;

scope.importScripts('wasm_bytes.js', 'wasm_bindgen.js');

const postReady = (error?: string) => {
    const payload: any = { type: 'ROADWORK_WASM_READY' };
    if (error) payload.error = error;
    scope.postMessage(payload);
};

let acked = false;
let readyError: string | undefined;

scope.onmessage = async (e: MessageEvent) => {
    if (e.data?.type === 'ROADWORK_WASM_ACK') {
        acked = true;
        return;
    }
    if (e.data?.type !== 'ROADWORK_RPC') return;
    const { id, method, args } = e.data;
    try {
        let result;
        if (method === 'get_services') {
            result = wasm_bindgen.get_services();
        } else if (method === 'get_roadworks') {
            result = await wasm_bindgen.get_roadworks(args[0], args[1]);
        } else if (method === 'clear_all_cache') {
            await wasm_bindgen.clear_all_cache();
            result = true;
        } else if (method === 'set_log_level') {
            wasm_bindgen.set_log_level(args[0]);
            result = true;
        } else if (method === 'set_custom_descriptors') {
            wasm_bindgen.set_custom_descriptors(args[0]);
            result = true;
        } else if (method === 'set_opendata_custom_descriptors') {
            wasm_bindgen.set_opendata_custom_descriptors(args[0]);
            result = true;
        } else if (method === 'get_opendata') {
            result = await wasm_bindgen.get_opendata(args[0], args[1]);
        } else if (method === 'get_opendata_cached') {
            result = await wasm_bindgen.get_opendata_cached(args[0]);
        } else if (method === 'get_opendata_counts') {
            result = await wasm_bindgen.get_opendata_counts();
        } else if (method === 'get_roadworks_in_bbox') {
            result = await wasm_bindgen.get_roadworks_in_bbox(args[0], args[1], args[2], args[3], args[4]);
        } else if (method === 'get_opendata_in_bbox') {
            result = await wasm_bindgen.get_opendata_in_bbox(args[0], args[1], args[2], args[3], args[4], args[5]);
        } else if (method === 'store_opendata_data') {
            await wasm_bindgen.store_opendata_data(args[0], args[1]);
            result = true;
        } else if (method === 'clear_roadworks_cache') {
            await wasm_bindgen.clear_roadworks_cache(args[0]);
            result = true;
        } else if (method === 'clear_opendata_cache') {
            await wasm_bindgen.clear_opendata_cache(args[0]);
            result = true;
        } else if (method === 'get_polygon_groups') {
            result = await wasm_bindgen.get_polygon_groups();
        } else if (method === 'save_polygon_groups') {
            await wasm_bindgen.save_polygon_groups(args[0]);
            result = true;
        } else if (method === 'get_db_overview') {
            result = await wasm_bindgen.get_db_overview();
        } else if (method === 'get_db_table') {
            result = await wasm_bindgen.get_db_table(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]);
        } else if (method === 'delete_db_row') {
            result = await wasm_bindgen.delete_db_row(args[0], args[1]);
        } else {
            throw new Error('Unknown method: ' + method);
        }
        console.info('[wasm] posting RPC result for id', id, 'method', method);
        scope.postMessage({ type: 'ROADWORK_RPC_RESULT', id, result });
    } catch (err) {
        scope.postMessage({ type: 'ROADWORK_RPC_ERROR', id, error: String(err) });
    }
};

(async () => {
    const _origLog = console.log;
    const _origWarn = console.warn;
    const _origError = console.error;
    console.log = (...args: any[]) => {
        _origLog.apply(console, args);
        scope.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'log', args: args.map((a: any) => String(a)) });
    };
    console.warn = (...args: any[]) => {
        _origWarn.apply(console, args);
        scope.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'warn', args: args.map((a: any) => String(a)) });
    };
    console.error = (...args: any[]) => {
        _origError.apply(console, args);
        scope.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'error', args: args.map((a: any) => String(a)) });
    };

    try {
        const raw = atob(WASM_BYTES);
        const bytes = new Uint8Array(raw.length);
        for (let i = 0; i < raw.length; i++) {
            bytes[i] = raw.charCodeAt(i);
        }
        await wasm_bindgen({ module_or_path: bytes });
        await wasm_bindgen.open_store();
    } catch (e) {
        readyError = String(e);
    }

    // READY is one-shot and can be missed if the page has not yet registered its
    // listener. Re-send it until the parent acknowledges (capped at 30s).
    postReady(readyError);
    const retry = setInterval(() => {
        if (acked) {
            clearInterval(retry);
            return;
        }
        postReady(readyError);
    }, 200);
    setTimeout(() => clearInterval(retry), 30000);
})();
