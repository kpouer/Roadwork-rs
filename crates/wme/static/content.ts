(() => {
    console.log('[Roadwork] content script loaded');
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

    let helperOverlay: HTMLDivElement | null = null;
    let helperIframe: HTMLIFrameElement | null = null;

    function closeHelper() {
        if (helperIframe) helperIframe.src = 'about:blank';
        if (helperOverlay) helperOverlay.classList.add('rw-helper-hidden');
    }

    function createHelperOverlay() {
        helperOverlay = document.createElement('div');
        helperOverlay.className = 'rw-helper-overlay rw-helper-hidden';

        const header = document.createElement('div');
        header.className = 'rw-helper-header';

        const title = document.createElement('h4');
        title.textContent = 'Roadwork descriptor helper';

        const closeBtn = document.createElement('button');
        closeBtn.className = 'rw-helper-close';
        closeBtn.textContent = '\u00d7';
        closeBtn.title = 'Close';
        closeBtn.addEventListener('click', closeHelper);

        header.appendChild(title);
        header.appendChild(closeBtn);

        helperIframe = document.createElement('iframe');
        helperIframe.className = 'rw-helper-iframe';
        helperIframe.setAttribute('allow', 'clipboard-read; clipboard-write');

        helperOverlay.appendChild(header);
        helperOverlay.appendChild(helperIframe);
        document.body.appendChild(helperOverlay);

        let isDragging = false;
        let dragOffsetX = 0;
        let dragOffsetY = 0;

        header.addEventListener('mousedown', (e) => {
            isDragging = true;
            const rect = helperOverlay.getBoundingClientRect();
            dragOffsetX = e.clientX - rect.left;
            dragOffsetY = e.clientY - rect.top;
            e.preventDefault();
        });

        document.addEventListener('mousemove', (e) => {
            if (!isDragging) return;
            helperOverlay.style.left = e.clientX - dragOffsetX + 'px';
            helperOverlay.style.top = e.clientY - dragOffsetY + 'px';
            helperOverlay.style.right = 'auto';
        });

        document.addEventListener('mouseup', () => {
            isDragging = false;
        });
    }

    window.addEventListener('message', (e) => {
        if (e.data?.type !== 'ROADWORK_OPEN_HELPER') return;
        console.log('[Roadwork] content script opening helper for', e.data?.service);
        if (!helperOverlay) {
            createHelperOverlay();
        }
        const params = new URLSearchParams();
        if (e.data.service) {
            params.set('service', e.data.service);
        }
        params.set('serviceHelper', '1');
        helperIframe.src = chrome.runtime.getURL('app/index.html') + '?' + params.toString();
        helperOverlay.classList.remove('rw-helper-hidden');
        window.postMessage({ type: 'ROADWORK_OPEN_HELPER_ACK', service: e.data?.service }, '*');
    });
})();
