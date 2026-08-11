// Thin relay between the WME page and the dedicated worker that hosts the wasm
// module. All RPC/console traffic is forwarded as-is.
(() => {
    const worker = new Worker(chrome.runtime.getURL('wasm-worker.js'));
    worker.onmessage = (e) => window.parent.postMessage(e.data, '*');
    window.addEventListener('message', (e) => {
        if (e.data?.type === 'ROADWORK_WASM_ACK' || e.data?.type === 'ROADWORK_RPC') {
            worker.postMessage(e.data);
        }
    });
})();
