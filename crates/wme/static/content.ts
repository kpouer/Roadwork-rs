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
    let pendingHelperData: string | null = null;

    let appOverlay: HTMLDivElement | null = null;
    let appIframe: HTMLIFrameElement | null = null;

    function openAppOverlay() {
        if (!appOverlay) {
            appOverlay = document.createElement('div');
            appOverlay.className = 'rw-app-overlay rw-app-hidden';

            const header = document.createElement('div');
            header.className = 'rw-helper-header';

            const title = document.createElement('h4');
            title.textContent = 'Roadwork';

            const closeBtn = document.createElement('button');
            closeBtn.className = 'rw-helper-close';
            closeBtn.textContent = '\u00d7';
            closeBtn.title = 'Close';
            closeBtn.addEventListener('click', closeAppOverlay);

            header.appendChild(title);
            header.appendChild(closeBtn);

            appIframe = document.createElement('iframe');
            appIframe.className = 'rw-app-iframe';
            appIframe.setAttribute('allow', 'clipboard-read; clipboard-write');

            appOverlay.appendChild(header);
            appOverlay.appendChild(appIframe);
            document.body.appendChild(appOverlay);
        }
        appIframe.src = chrome.runtime.getURL('app/index.html');
        appOverlay.classList.remove('rw-app-hidden');
    }

    function closeAppOverlay() {
        if (appOverlay) {
            appOverlay.classList.add('rw-app-hidden');
        }
        if (appIframe) {
            appIframe.src = 'about:blank';
        }
    }

    function closeHelper() {
        if (helperIframe) helperIframe.src = 'about:blank';
        if (helperOverlay) helperOverlay.classList.add('rw-helper-hidden');
        pendingHelperData = null;
    }

    function sendPendingHelperData() {
        if (pendingHelperData === null || !helperIframe?.contentWindow) return;
        let acked = false;
        const listener = (ev: MessageEvent) => {
            if (
                ev.source === helperIframe.contentWindow &&
                ev.data?.type === 'ROADWORK_HELPER_DATA_ACK'
            ) {
                acked = true;
                window.removeEventListener('message', listener);
                clearInterval(timer);
                clearTimeout(timeout);
            }
        };
        window.addEventListener('message', listener);
        const timer = setInterval(() => {
            if (acked || !helperIframe?.contentWindow) return;
            helperIframe.contentWindow.postMessage(
                { type: 'ROADWORK_HELPER_DATA', data: pendingHelperData },
                '*'
            );
        }, 300);
        const timeout = setTimeout(() => {
            if (acked) return;
            window.removeEventListener('message', listener);
            clearInterval(timer);
        }, 30000);
        helperIframe.contentWindow.postMessage(
            { type: 'ROADWORK_HELPER_DATA', data: pendingHelperData },
            '*'
        );
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
        if (e.data?.type === 'ROADWORK_OPEN_APP') {
            console.log('[Roadwork] content script opening app overlay');
            openAppOverlay();
            return;
        }
        if (e.data?.type !== 'ROADWORK_OPEN_HELPER') return;
        console.log('[Roadwork] content script opening helper for', e.data?.service);
        if (!helperOverlay) {
            createHelperOverlay();
        }
        const params = new URLSearchParams();
        if (e.data.service) {
            params.set('service', e.data.service);
        }
        if (e.data.helper === 'opendata') {
            params.set('opendata', '1');
        } else {
            params.set('serviceHelper', '1');
        }
        if (e.data.create) {
            params.set('create', '1');
        }
        if (e.data.descriptor) {
            params.set('descriptor', e.data.descriptor);
        }
        pendingHelperData = typeof e.data.data === 'string' ? e.data.data : null;
        helperIframe.src = chrome.runtime.getURL('app/index.html') + '?' + params.toString();
        helperOverlay.classList.remove('rw-helper-hidden');
        window.postMessage({ type: 'ROADWORK_OPEN_HELPER_ACK', service: e.data?.service }, '*');
        if (pendingHelperData !== null) {
            sendPendingHelperData();
        }
    });
})();
