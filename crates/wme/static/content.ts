(() => {
    const iframe = document.createElement('iframe');
    iframe.id = 'roadwork-wasm-iframe';
    iframe.style.display = 'none';
    iframe.src = chrome.runtime.getURL('wasm-iframe.html');

    iframe.onload = () => {
        const script = document.createElement('script');
        script.src = chrome.runtime.getURL('inject.js');
        document.head.appendChild(script);
    };

    document.body.appendChild(iframe);
})();
