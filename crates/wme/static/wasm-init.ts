(async () => {
    try {
        const _origLog = console.log;
        const _origWarn = console.warn;
        const _origError = console.error;
        console.log = (...args) => {
            _origLog.apply(console, args);
            window.parent.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'log', args: args.map(a => String(a)) }, '*');
        };
        console.warn = (...args) => {
            _origWarn.apply(console, args);
            window.parent.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'warn', args: args.map(a => String(a)) }, '*');
        };
        console.error = (...args) => {
            _origError.apply(console, args);
            window.parent.postMessage({ type: 'ROADWORK_CONSOLE_LOG', level: 'error', args: args.map(a => String(a)) }, '*');
        };

        const raw = atob(WASM_BYTES);
        const bytes = new Uint8Array(raw.length);
        for (let i = 0; i < raw.length; i++) {
            bytes[i] = raw.charCodeAt(i);
        }
        await wasm_bindgen({ module_or_path: bytes });
        window.parent.postMessage({ type: 'ROADWORK_WASM_READY' }, '*');

        window.addEventListener('message', async (e) => {
            if (e.data?.type !== 'ROADWORK_RPC') return;
            const { id, method, args } = e.data;
            try {
                let result;
                if (method === 'get_services') {
                    result = wasm_bindgen.get_services();
                } else if (method === 'get_roadworks') {
                    result = await wasm_bindgen.get_roadworks(args[0]);
                } else if (method === 'set_log_level') {
                    wasm_bindgen.set_log_level(args[0]);
                    result = true;
                } else if (method === 'set_custom_descriptors') {
                    wasm_bindgen.set_custom_descriptors(args[0]);
                    result = true;
                } else {
                    throw new Error('Unknown method: ' + method);
                }
                console.info('[wasm] posting RPC result for id', id, 'method', method);
                window.parent.postMessage({ type: 'ROADWORK_RPC_RESULT', id, result }, '*');
            } catch (err) {
                window.parent.postMessage({ type: 'ROADWORK_RPC_ERROR', id, error: String(err) }, '*');
            }
        });
    } catch (e) {
        window.parent.postMessage({ type: 'ROADWORK_WASM_READY', error: String(e) }, '*');
    }
})();
