type WmeSDK = import("wme-sdk-typings").WmeSDK;

const wasmIframe = document.getElementById('roadwork-wasm-iframe');

let rpcId = 0;
const rpcPending = new Map();
let helperAcked = false;

window.addEventListener("message", (e) => {
    if (e.data?.type === "ROADWORK_OPEN_HELPER_ACK") {
        helperAcked = true;
        setStatus("Descriptor helper opened", "success");
    } else if (e.data?.type === "ROADWORK_RPC_RESULT") {
        console.info("[Roadwork] RPC_RESULT received for id", e.data.id, "has result:", e.data.result !== undefined);
        const p = rpcPending.get(e.data.id);
        if (p) {
            rpcPending.delete(e.data.id);
            p.resolve(e.data.result);
        } else {
            console.warn("[Roadwork] RPC_RESULT no pending call for id", e.data.id);
        }
    } else if (e.data?.type === "ROADWORK_RPC_ERROR") {
        const p = rpcPending.get(e.data.id);
        if (p) {
            rpcPending.delete(e.data.id);
            p.reject(new Error(e.data.error));
        }
    } else if (e.data?.type === "ROADWORK_CONSOLE_LOG") {
        const method = e.data.level === "error" ? console.error : e.data.level === "warn" ? console.warn : console.log;
        method("[Roadwork WASM]", ...e.data.args);
    } else if (e.data?.type === "ROADWORK_WASM_READY") {
        if (wasmIframe?.contentWindow && e.source === wasmIframe.contentWindow) {
            wasmIframe.contentWindow.postMessage({ type: "ROADWORK_WASM_ACK" }, "*");
        }
    }
});

function rpcCall(method: string, args = []) {
    console.info("[Roadwork] rpcCall", method);
    return new Promise<any>((resolve, reject) => {
        const id = ++rpcId;
        const timer = setTimeout(() => {
            rpcPending.delete(id);
            reject(new Error("RPC timeout: " + method));
        }, 30000);
        rpcPending.set(id, {
            resolve: (v) => { clearTimeout(timer); resolve(v); },
            reject: (e) => { clearTimeout(timer); reject(e); },
        });
        wasmIframe.contentWindow.postMessage({ type: "ROADWORK_RPC", id, method, args }, "*");
    });
}

const wasmReady = new Promise<void>((resolve, reject) => {
    window.addEventListener("message", function onReady(e) {
        if (e.data?.type === "ROADWORK_WASM_READY") {
            window.removeEventListener("message", onReady);
            if (e.data.error) {
                reject(new Error(e.data.error));
            } else {
                resolve();
            }
        }
    });
});

const SCRIPT_ID = "roadwork-wme";
const SCRIPT_NAME = "Roadwork for WME";
const STORAGE_KEY = "roadwork-wme-settings";
const CACHE_KEY_PREFIX = "roadwork-wme-cache-";
const SERVICES_CACHE_KEY = "roadwork-wme-services-cache";
const CUSTOM_SOURCES_CACHE_KEY = "roadwork-wme-custom-sources-cache";
const STATUS_OVERRIDES_KEY = "roadwork-wme-status-overrides";
const MAX_WAIT = 120000;

const STATUS_COLORS = {
    New: "#ef4444",
    Later: "#f97316",
    Ignored: "#9ca3af",
    Finished: "#22c55e",
    Treated: "#3b82f6",
};

const DEFAULTS = {
    service: "France-Paris",
    logLevel: "info",
    customSources: [],
};

let wmeSDK: WmeSDK = null;
let settings = {...DEFAULTS};
let currentRoadworks: any = {};
let servicesData = [];
let panelEl = null;
let statusEl = null;
let countEl = null;
let lastRefreshEl = null;
let floatingPanelEl = null;
let floatingTableBody = null;
let floatingToggleBtn = null;
let selectedRoadworkId = null;
let polygonGroups: any = {};
let nextGroupId = 0;
const WKT_LAYER = "Roadwork - WKT";
const POLYGON_GROUPS_KEY = "roadwork-wme-polygon-groups";
let detailPanelEl = null;
let hideFinished = false;
let sortColumn = -1;
let sortDirection = 'asc';

async function initScript() {
    console.log("Roadwork tryInit");
    wmeSDK = window.getWmeSdk({
        scriptId: SCRIPT_ID,
        scriptName: SCRIPT_NAME,
    });
    try {
        console.log("Roadwork waiting iframe");
        await Promise.race([
            wasmReady,
            new Promise((_, reject) => setTimeout(() => reject(new Error("WASM iframe not ready after " + MAX_WAIT / 1000 + "s")), MAX_WAIT)),
        ]);
    } catch (e) {
        console.error("[Roadwork] init failed:", e);
        return;
    }
    console.log("Roadwork found iframe");
    init().catch((e) => {
        console.error("[Roadwork] init failed:", e);
    });
    console.log("Roadwork init done");
}

console.log("Roadwork initializing");
window.SDK_INITIALIZED.then(initScript);

const ALL_STATUSES = Object.keys(STATUS_COLORS);

function getLayerName(status: string) {
    return "Roadwork - " + status;
}

function loadSettings() {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw) {
            settings = {...DEFAULTS, ...JSON.parse(raw)};
        }
    } catch (_) {
        settings = {...DEFAULTS};
    }
}

function saveSettings() {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch (_) {}
}

async function applyLogLevel(level: string) {
    try {
        await rpcCall("set_log_level", [level]);
    } catch (_) {}
}

function getCacheKey(service: string) {
    return CACHE_KEY_PREFIX + service;
}

function loadCache(service: string) {
    try {
        const raw = localStorage.getItem(getCacheKey(service));
        if (!raw) return null;
        const cached = JSON.parse(raw);
        const maxAge = 86400 * 1000;
        if (Date.now() - cached.timestamp > maxAge) return null;
        return cached.data;
    } catch (_) {
        return null;
    }
}

function saveCache(service: string, data) {
    try {
        localStorage.setItem(getCacheKey(service), JSON.stringify({
            data: data,
            timestamp: Date.now(),
        }));
    } catch (_) {
    }
}

function clearCache(service: string) {
    try {
        localStorage.removeItem(getCacheKey(service));
    } catch (_) {
    }
}

function savePolygonGroups() {
    try {
        localStorage.setItem(POLYGON_GROUPS_KEY, JSON.stringify({ groups: polygonGroups, nextId: nextGroupId }));
    } catch (_) {}
}

function loadPolygonGroups() {
    try {
        const raw = localStorage.getItem(POLYGON_GROUPS_KEY);
        if (raw) {
            const parsed = JSON.parse(raw);
            if (parsed && typeof parsed === "object" && parsed.groups) {
                nextGroupId = parsed.nextId || 0;
                return parsed.groups;
            }
        }
    } catch (_) {}
    return {};
}

function loadStatusOverrides() {
    try {
        const raw = localStorage.getItem(STATUS_OVERRIDES_KEY);
        if (raw) return JSON.parse(raw);
    } catch (_) {}
    return {};
}

function saveStatusOverrides(overrides) {
    try {
        localStorage.setItem(STATUS_OVERRIDES_KEY, JSON.stringify(overrides));
    } catch (_) {}
}

function applyStatusOverrides() {
    const overrides = loadStatusOverrides();
    for (const [id, status] of Object.entries(overrides)) {
        if (currentRoadworks[id]) {
            currentRoadworks[id].syncData = currentRoadworks[id].syncData || {};
            currentRoadworks[id].syncData.status = status;
        }
    }
}

function loadHideFinished() {
    try {
        const v = localStorage.getItem(HIDE_FINISHED_KEY);
        if (v !== null) hideFinished = JSON.parse(v);
    } catch (_) {}
}

function loadSortState() {
    try {
        const v = localStorage.getItem(SORT_STATE_KEY);
        if (v !== null) {
            const parsed = JSON.parse(v);
            sortColumn = parsed.col;
            sortDirection = parsed.dir;
        }
    } catch (_) {}
}

function changeRoadworkStatus(rwId, newStatus) {
    const rw = currentRoadworks[rwId];
    if (!rw) return;

    rw.syncData = rw.syncData || {};
    rw.syncData.status = newStatus;

    const overrides = loadStatusOverrides();
    overrides[rwId] = newStatus;
    saveStatusOverrides(overrides);

    renderRoadworksToMap(currentRoadworks);
    updateFloatingTable();
    showDetailPanel(rw);
}

function loadServicesCache() {
    try {
        console.info("[Roadwork] loadServicesCache");
        const raw = localStorage.getItem(SERVICES_CACHE_KEY);
        if (!raw) {
            console.info("[Roadwork] loadServicesCache no cache");
            return null;
        }
        const cached = JSON.parse(raw);
        return Array.isArray(cached.data) ? cached.data : null;
    } catch (_) {
        return null;
    }
}

function saveServicesCache(services) {
    try {
        localStorage.setItem(SERVICES_CACHE_KEY, JSON.stringify({
            data: services,
            timestamp: Date.now(),
        }));
    } catch (_) {
    }
}

async function fetchServices(forceRefresh = false) {
    if (!forceRefresh) {
        const cached = loadServicesCache();
        if (cached) return cached;
        console.info("[Roadwork] fetchServices no cache, will refresh");
    }
    try {
        const data = await rpcCall("get_services");
        if (Array.isArray(data)) {
            saveServicesCache(data);
            return data;
        }
        return [];
    } catch (_) {
        return [];
    }
}

async function fetchRoadworks(forceRefresh = false) {
    if (!forceRefresh) {
        const cached = loadCache(settings.service);
        if (cached) return cached;
    }
    try {
        const data = await rpcCall("get_roadworks", [settings.service]);
        console.info("[Roadwork] fetchRoadworks received data, type:", typeof data, "roadworks keys:", data?.roadworks ? Object.keys(data.roadworks).length : "no roadworks field");
        saveCache(settings.service, data);
        return data;
    } catch (e) {
        throw new Error(`Failed to fetch roadworks: ${e}`);
    }
}

function loadCustomDescriptorsCache(): Array<any> | null {
    try {
        const raw = localStorage.getItem(CUSTOM_SOURCES_CACHE_KEY);
        if (!raw) return null;
        const cached = JSON.parse(raw);
        return Array.isArray(cached.data) ? cached.data : null;
    } catch (_) {
        return null;
    }
}

function saveCustomDescriptorsCache(pairs: Array<any>) {
    try {
        localStorage.setItem(CUSTOM_SOURCES_CACHE_KEY, JSON.stringify({
            data: pairs,
            timestamp: Date.now(),
        }));
    } catch (_) {}
}

async function fetchCustomDescriptors() {
    const sources = Array.isArray(settings.customSources) ? settings.customSources : [];
    const pairs = [];
    for (const url of sources) {
        let index;
        try {
            const resp = await fetch(url);
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            index = await resp.json();
        } catch (e) {
            console.warn(`[Roadwork] Failed to fetch index ${url}: ${e}`);
            continue;
        }
        if (!index || !Array.isArray(index.files)) {
            console.warn(`[Roadwork] Invalid index.json at ${url}`);
            continue;
        }
        const baseDir = url.substring(0, url.lastIndexOf("/") + 1);
        for (const file of index.files) {
            const path = file.path;
            if (!path || path.includes("..") || path.startsWith("/")) {
                console.warn(`[Roadwork] Skipping invalid path ${path} in ${url}`);
                continue;
            }
            const name = file.key || path.replace(/\.json$/i, "");
            const descUrl = baseDir + path;
            try {
                const resp = await fetch(descUrl);
                if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                const json = await resp.text();
                pairs.push([name, json]);
            } catch (e) {
                console.warn(`[Roadwork] Failed to fetch descriptor ${descUrl}: ${e}`);
            }
        }
    }
    return pairs;
}

async function syncCustomDescriptorsToWasm(forceRefresh = false) {
    const sources = Array.isArray(settings.customSources) ? settings.customSources : [];
    if (sources.length === 0) {
        await rpcCall("set_custom_descriptors", [[]]);
        try {
            localStorage.removeItem(CUSTOM_SOURCES_CACHE_KEY);
        } catch (_) {}
        return;
    }
    let pairs = null;
    if (!forceRefresh) {
        pairs = loadCustomDescriptorsCache();
    }
    if (pairs === null) {
        pairs = await fetchCustomDescriptors();
        saveCustomDescriptorsCache(pairs);
    }
    await rpcCall("set_custom_descriptors", [pairs]);
    try {
        localStorage.removeItem(SERVICES_CACHE_KEY);
    } catch (_) {}
}

function setStatus(text: string, type?) {
    if (!statusEl) {
        return;
    }
    statusEl.textContent = text;
    statusEl.className = "roadwork-status" + (type ? " " + type : "");
}

function setCount(count: number) {
    if (!countEl) {
        return;
    }
    countEl.textContent = `${count} roadwork(s) loaded`;
}

function updateLastRefreshDisplay() {
    if (!lastRefreshEl) {
        return;
    }
    try {
        const stored = localStorage.getItem(LAST_REFRESH_KEY);
        if (stored) {
            lastRefreshEl.textContent = new Date(parseInt(stored, 10)).toLocaleString("fr-FR");
        }
    } catch (_) {
    }
}

const LAST_REFRESH_KEY = "roadwork-wme-last-refresh";
const PANEL_STORAGE_KEY = "roadwork-wme-panel-visible";
const HIDE_FINISHED_KEY = "roadwork-wme-hide-finished";
const PANEL_SIZE_KEY = "roadwork-wme-panel-size";
const SORT_STATE_KEY = "roadwork-wme-sort-state";

function isFloatingPanelVisible() {
    try {
        const v = localStorage.getItem(PANEL_STORAGE_KEY);
        return v === null ? true : v === "true";
    } catch (_) {
        return true;
    }
}

function setFloatingPanelVisible(visible: boolean) {
    try {
        localStorage.setItem(PANEL_STORAGE_KEY, String(visible));
    } catch (_) {
    }
    if (floatingPanelEl) {
        floatingPanelEl.classList.toggle("rw-hidden", !visible);
    }
    if (floatingToggleBtn) {
        floatingToggleBtn.style.display = visible ? "none" : "block";
    }
}

function openDescriptorHelper() {
    console.log("[Roadwork] openDescriptorHelper, service =", settings.service);
    if (!wasmIframe) {
        setStatus("WASM iframe not available", "error");
        return;
    }
    setStatus("Opening descriptor helper...", "info");
    helperAcked = false;
    window.postMessage({ type: "ROADWORK_OPEN_HELPER", service: settings.service }, "*");
    setTimeout(() => {
        if (!helperAcked) {
            setStatus("Content script did not respond - reload the extension (chrome://extensions)", "error");
        }
    }, 1000);
}

function createFloatingPanel() {
    floatingPanelEl = document.createElement("div");
    floatingPanelEl.className = "roadwork-floating-panel";
    if (!isFloatingPanelVisible()) {
        floatingPanelEl.classList.add("rw-hidden");
    }

    const header = document.createElement("div");
    header.className = "roadwork-floating-header";

    const title = document.createElement("h4");
    title.textContent = "Roadwork List";

    const filterLabel = document.createElement("label");
    filterLabel.className = "rw-filter-label";
    filterLabel.title = "Masquer les chantiers terminés";

    const filterCheck = document.createElement("input");
    filterCheck.type = "checkbox";
    filterCheck.checked = hideFinished;
    filterCheck.addEventListener("change", () => {
        hideFinished = filterCheck.checked;
        localStorage.setItem(HIDE_FINISHED_KEY, JSON.stringify(hideFinished));
        updateFloatingTable();
    });

    const filterText = document.createElement("span");
    filterText.textContent = "Hide finished";
    filterLabel.appendChild(filterCheck);
    filterLabel.appendChild(filterText);

    const btnGroup = document.createElement("div");

    const refreshBtn = document.createElement("button");
    refreshBtn.textContent = "\u21bb";
    refreshBtn.title = "Refresh";
    refreshBtn.addEventListener("click", () => refreshData());

    const resetBtn = document.createElement("button");
    resetBtn.textContent = "\u232b";
    resetBtn.title = "Reset all data (clear localStorage)";
    resetBtn.addEventListener("click", () => clearExtensionStorage());

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = "Hide";
    closeBtn.addEventListener("click", () => setFloatingPanelVisible(false));

    btnGroup.appendChild(refreshBtn);
    btnGroup.appendChild(resetBtn);
    btnGroup.appendChild(closeBtn);
    header.appendChild(title);
    header.appendChild(filterLabel);
    header.appendChild(btnGroup);

    const tableWrap = document.createElement("div");
    tableWrap.className = "roadwork-table-wrap";

    const table = document.createElement("table");
    table.className = "roadwork-table";
    const thead = document.createElement("thead");
    const headerRow = document.createElement("tr");
    const COLUMNS = ["Statut", "Route", "Début", "Fin", "Description", "Impact"];
    const headerCells = [];
    for (let i = 0; i < COLUMNS.length; i++) {
        const th = document.createElement("th");
        th.className = "rw-sortable";
        let label = COLUMNS[i];
        if (sortColumn === i) {
            label += sortDirection === 'asc' ? ' ▲' : ' ▼';
        }
        th.textContent = label;
        th.addEventListener("click", () => {
            if (sortColumn === i) {
                sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
            } else {
                sortColumn = i;
                sortDirection = 'asc';
            }
            localStorage.setItem(SORT_STATE_KEY, JSON.stringify({ col: sortColumn, dir: sortDirection }));
            for (let j = 0; j < headerCells.length; j++) {
                headerCells[j].textContent = COLUMNS[j];
            }
            if (sortColumn >= 0) {
                headerCells[sortColumn].textContent += sortDirection === 'asc' ? ' ▲' : ' ▼';
            }
            updateFloatingTable();
        });
        headerCells.push(th);
        headerRow.appendChild(th);
    }
    thead.appendChild(headerRow);
    table.appendChild(thead);
    floatingTableBody = document.createElement("tbody");
    table.appendChild(floatingTableBody);
    tableWrap.appendChild(table);

    floatingPanelEl.appendChild(header);
    floatingPanelEl.appendChild(tableWrap);

    const resizeHandle = document.createElement("div");
    resizeHandle.className = "rw-resize-handle";
    floatingPanelEl.appendChild(resizeHandle);

    document.body.appendChild(floatingPanelEl);

    try {
        const savedSize = JSON.parse(localStorage.getItem(PANEL_SIZE_KEY));
        if (savedSize) {
            if (savedSize.w >= 300) floatingPanelEl.style.width = savedSize.w + "px";
            if (savedSize.h >= 200) floatingPanelEl.style.height = savedSize.h + "px";
        }
    } catch (_) {}

    floatingToggleBtn = document.createElement("button");
    floatingToggleBtn.className = "rw-toggle-btn";
    floatingToggleBtn.textContent = "Roadwork List";
    floatingToggleBtn.addEventListener("click", () => setFloatingPanelVisible(true));
    if (isFloatingPanelVisible()) {
        floatingToggleBtn.style.display = "none";
    }
    if (toolbarEl) toolbarEl.appendChild(floatingToggleBtn);

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = floatingPanelEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        const x = e.clientX - dragOffsetX;
        const y = e.clientY - dragOffsetY;
        floatingPanelEl.style.left = x + "px";
        floatingPanelEl.style.top = y + "px";
        floatingPanelEl.style.right = "auto";
        floatingPanelEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });

    let isResizing = false;
    let resizeStartX = 0, resizeStartY = 0;
    let resizeStartW = 0, resizeStartH = 0;
    resizeHandle.addEventListener("mousedown", (e) => {
        isResizing = true;
        resizeStartX = e.clientX;
        resizeStartY = e.clientY;
        const rect = floatingPanelEl.getBoundingClientRect();
        resizeStartW = rect.width;
        resizeStartH = rect.height;
        e.preventDefault();
        e.stopPropagation();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isResizing) return;
        const newW = resizeStartW + (e.clientX - resizeStartX);
        const newH = resizeStartH + (e.clientY - resizeStartY);
        if (newW >= 300) floatingPanelEl.style.width = newW + "px";
        if (newH >= 200) floatingPanelEl.style.height = newH + "px";
    });
    document.addEventListener("mouseup", () => {
        if (isResizing) {
            isResizing = false;
            const rect = floatingPanelEl.getBoundingClientRect();
            localStorage.setItem(PANEL_SIZE_KEY, JSON.stringify({ w: rect.width, h: rect.height }));
        }
    });
}

function updateFloatingTable() {
    if (!floatingTableBody) return;
    floatingTableBody.replaceChildren();
    let entries = Object.entries(currentRoadworks as Record<string, any>);
    if (entries.length === 0) {
        const tr = document.createElement("tr");
        const td = document.createElement("td");
        td.colSpan = 7;
        td.style.textAlign = "center";
        td.style.color = "#999";
        td.style.padding = "16px";
        td.textContent = "Aucun roadwork chargé";
        tr.appendChild(td);
        floatingTableBody.appendChild(tr);
        return;
    }

    if (hideFinished) {
        entries = entries.filter(([, rw]) => (rw.syncData?.status || "New") !== "Finished");
    }

    if (sortColumn >= 0) {
        const getValue = (rw, col) => {
            switch (col) {
                case 0: return (rw.syncData?.status || "New");
                case 1: return (rw.road || "");
                case 2: return rw.start != null ? rw.start : Infinity;
                case 3: return rw.end != null ? rw.end : Infinity;
                case 4: return (rw.opendata?.description || "");
                case 5: return (rw.impactCirculationDetail || "");
                default: return "";
            }
        };
        entries.sort((a, b) => {
            const va = getValue(a[1], sortColumn);
            const vb = getValue(b[1], sortColumn);
            let cmp;
            if (typeof va === "number" && typeof vb === "number") {
                cmp = va - vb;
            } else {
                cmp = String(va).localeCompare(String(vb), "fr");
            }
            return sortDirection === 'asc' ? cmp : -cmp;
        });
    }

    if (entries.length === 0) {
        const tr = document.createElement("tr");
        const td = document.createElement("td");
        td.colSpan = 7;
        td.style.textAlign = "center";
        td.style.color = "#999";
        td.style.padding = "16px";
        td.textContent = "Aucun roadwork à afficher";
        tr.appendChild(td);
        floatingTableBody.appendChild(tr);
        return;
    }

    for (const [id, rw] of entries) {
        const status = rw.syncData?.status || "New";
        const color = STATUS_COLORS[status] || "#9ca3af";
        const road = rw.road || "";
        const start = formatTimestamp(rw.start);
        const end = formatTimestamp(rw.end);
        const desc = rw.opendata?.description || "";
        const impact = rw.impactCirculationDetail || "";

        const tr = document.createElement("tr");
        tr.title = desc;
        tr.addEventListener("click", () => {
            if (selectedRoadworkId === id) {
                deselectRoadwork();
            } else {
                selectedRoadworkId = id;
                showDetailPanel(rw);
                renderRoadworksToMap(currentRoadworks);
            }
            if (rw.opendata?.latitude && rw.opendata?.longitude && wmeSDK?.Map?.setMapCenter) {
                wmeSDK.Map.setMapCenter({lonLat: {lon: rw.opendata?.longitude, lat: rw.opendata?.latitude}});
            }
        });

        const tdStatus = document.createElement("td");
        const badge = document.createElement("span");
        badge.className = "rw-status-badge";
        badge.style.background = color;
        badge.textContent = status;
        tdStatus.appendChild(badge);

        const tdRoad = document.createElement("td");
        tdRoad.textContent = road;

        const tdStart = document.createElement("td");
        tdStart.textContent = start;

        const tdEnd = document.createElement("td");
        tdEnd.textContent = end;

        const tdDesc = document.createElement("td");
        tdDesc.className = "rw-desc";
        tdDesc.textContent = desc;

        const tdImpact = document.createElement("td");
        tdImpact.textContent = impact;

        tr.appendChild(tdStatus);
        tr.appendChild(tdRoad);
        tr.appendChild(tdStart);
        tr.appendChild(tdEnd);
        tr.appendChild(tdDesc);
        tr.appendChild(tdImpact);
        floatingTableBody.appendChild(tr);
    }
}

function buildMarkerIcon(status) {
    const color = STATUS_COLORS[status] || "#9ca3af";
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="32" viewBox="0 0 24 32">
            <path d="M2 28 L22 28 L21 30 L3 30 Z" fill="#333" />
            <path d="M12 2 L3 28 L21 28 Z" fill="${color}" stroke="${color}" stroke-width="1" />
            <path d="M9.3 10 L14.7 10 L16.3 15 L7.7 15 Z" fill="white" opacity="0.9" />
            <path d="M7 19 L17 19 L18.7 24 L5.3 24 Z" fill="white" opacity="0.9" />
        </svg>`;
    return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

function formatTimestamp(millis) {
    if (!millis) {
        return "?";
    }
    try {
        return new Date(millis).toLocaleDateString("fr-FR", {
            day: "2-digit",
            month: "2-digit",
            year: "numeric",
        });
    } catch (_) {
        return String(millis);
    }
}

function findMatchingParen(str, openIdx) {
    let depth = 1;
    let i = openIdx;
    while (depth > 0 && i < str.length - 1) {
        i++;
        if (str[i] === '(') depth++;
        else if (str[i] === ')') depth--;
    }
    return i;
}

function parseCoordList(str) {
    return str.split(',').map(p => p.trim().split(/\s+/).map(Number)).filter(c => c.length >= 2 && isFinite(c[0]) && isFinite(c[1]));
}

function splitTopLevelParens(str) {
    str = str.trim();
    const groups = [];
    let depth = 0;
    let start = -1;
    for (let i = 0; i < str.length; i++) {
        const ch = str[i];
        if (ch === '(') {
            if (depth === 0) start = i + 1;
            depth++;
        } else if (ch === ')') {
            depth--;
            if (depth === 0 && start >= 0) {
                groups.push(str.substring(start, i).trim());
                start = -1;
            }
        }
    }
    return groups;
}

function parseRings(str) {
    return splitTopLevelParens(str).map(ringStr => {
        const coords = parseCoordList(ringStr);
        if (coords.length < 3) return coords;
        const first = coords[0];
        const last = coords[coords.length - 1];
        if (first[0] !== last[0] || first[1] !== last[1]) {
            coords.push([...first]);
        }
        return coords;
    });
}

function parseWkt(str) {
    str = str.replace(/^(?:--|#).*/gm, '').trim();
    if (!str) return [];

    const features = [];
    let idCounter = 0;
    const geomPattern = /(POINT|LINESTRING|POLYGON|MULTIPOINT|MULTILINESTRING|MULTIPOLYGON)\s*\(/gi;

    let match;
    while ((match = geomPattern.exec(str)) !== null) {
        const type = match[1].toUpperCase();
        const openIdx = match.index + match[0].length - 1;
        const closeIdx = findMatchingParen(str, openIdx);
        const inner = str.substring(openIdx + 1, closeIdx);

        switch (type) {
            case 'POINT': {
                const [x, y] = inner.trim().split(/\s+/).map(Number);
                if (isFinite(x) && isFinite(y)) {
                    features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'Point', coordinates: [x, y] }, properties: { geomType: 'Point' } });
                }
                break;
            }
            case 'LINESTRING': {
                const coords = parseCoordList(inner);
                if (coords.length >= 2) {
                    features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'LineString', coordinates: coords }, properties: { geomType: 'LineString' } });
                }
                break;
            }
            case 'POLYGON': {
                const rings = parseRings(inner);
                if (rings.length > 0 && rings[0].length >= 3) {
                    features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'Polygon', coordinates: rings }, properties: { geomType: 'Polygon' } });
                }
                break;
            }
            case 'MULTIPOINT': {
                const coords = inner.trim().startsWith('(')
                    ? splitTopLevelParens(inner).map(c => { const [x, y] = c.trim().split(/\s+/).map(Number); return [x, y]; })
                    : parseCoordList(inner);
                for (const [x, y] of coords) {
                    if (isFinite(x) && isFinite(y)) {
                        features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'Point', coordinates: [x, y] }, properties: { geomType: 'Point' } });
                    }
                }
                break;
            }
            case 'MULTILINESTRING': {
                const groups = splitTopLevelParens(inner);
                for (const g of groups) {
                    const coords = parseCoordList(g);
                    if (coords.length >= 2) {
                        features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'LineString', coordinates: coords }, properties: { geomType: 'LineString' } });
                    }
                }
                break;
            }
            case 'MULTIPOLYGON': {
                const groups = splitTopLevelParens(inner);
                for (const g of groups) {
                    const rings = parseRings(g);
                    if (rings.length > 0 && rings[0].length >= 3) {
                        features.push({ id: `wkt-${idCounter++}`, type: 'Feature', geometry: { type: 'Polygon', coordinates: rings }, properties: { geomType: 'Polygon' } });
                    }
                }
                break;
            }
        }
    }

    return features;
}

function buildPopupContent(rw) {
    const start = formatTimestamp(rw.start);
    const end = formatTimestamp(rw.end);
    const road = rw.road || "";
    const desc = rw.opendata?.description || "";
    const impact = rw.impactCirculationDetail || "";
    const status = rw.syncData?.status || "New";

    let html = `<div style="font-size:13px;max-width:280px;">`;
    html += `<strong style="color:${STATUS_COLORS[status] || "#333"};">[${status}]</strong> `;
    if (road) {
        html += `<strong>${road}</strong><br/>`;
    }
    html += `<span style="color:#666;">${start} — ${end}</span><br/>`;
    if (desc) {
        html += `<p style="margin:4px 0;">${desc}</p>`;
    }
    if (impact) {
        html += `<p style="margin:4px 0;color:#b45309;">Impact: ${impact}</p>`;
    }
    if (rw.url) {
        html += `<a href="${rw.url}" target="_blank" style="color:#4a90d9;">Source</a>`;
    }
    html += `</div>`;
    return html;
}

let detailOverlayEl = null;

function createDetailPanel() {
    detailOverlayEl = document.createElement("div");
    detailOverlayEl.className = "rw-detail-overlay rw-hidden";
    detailOverlayEl.addEventListener("click", () => deselectRoadwork());
    document.body.appendChild(detailOverlayEl);

    detailPanelEl = document.createElement("div");
    detailPanelEl.className = "rw-detail-panel rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-detail-header";

    const title = document.createElement("h4");
    title.textContent = "Détails du chantier";

    const closeBtn = document.createElement("button");
    closeBtn.className = "rw-detail-close";
    closeBtn.textContent = "\u00d7";
    closeBtn.addEventListener("click", () => deselectRoadwork());

    header.appendChild(title);
    header.appendChild(closeBtn);

    const body = document.createElement("div");
    body.className = "rw-detail-body";

    detailPanelEl.appendChild(header);
    detailPanelEl.appendChild(body);
    document.body.appendChild(detailPanelEl);
}

function showDetailPanel(rw) {
    if (!detailPanelEl) return;
    const body = detailPanelEl.querySelector(".rw-detail-body");
    if (!body) return;

    const status = rw.syncData?.status || "New";
    const color = STATUS_COLORS[status] || "#9ca3af";
    const road = rw.road || "";
    const start = formatTimestamp(rw.start);
    const end = formatTimestamp(rw.end);
    const desc = rw.opendata?.description || "";
    const impact = rw.impactCirculationDetail || "";

    body.replaceChildren();

    const addField = (labelText, valueEl) => {
        const div = document.createElement("div");
        div.className = "rw-detail-field";
        const label = document.createElement("label");
        label.textContent = labelText;
        div.appendChild(label);
        div.appendChild(valueEl);
        body.appendChild(div);
    };

    {
        const dropdown = document.createElement("div");
        dropdown.className = "rw-status-dropdown";

        const trigger = document.createElement("span");
        trigger.className = "rw-status-dropdown-trigger";
        trigger.style.background = color;
        trigger.textContent = status;

        const menu = document.createElement("div");
        menu.className = "rw-status-dropdown-menu rw-hidden";

        for (const s of ALL_STATUSES) {
            const item = document.createElement("div");
            item.className = "rw-status-dropdown-item";

            const dot = document.createElement("span");
            dot.className = "rw-status-dropdown-dot";
            dot.style.background = STATUS_COLORS[s];

            const label = document.createTextNode(s);

            item.appendChild(dot);
            item.appendChild(label);
            item.addEventListener("click", (e) => {
                e.stopPropagation();
                changeRoadworkStatus(selectedRoadworkId, s);
            });

            menu.appendChild(item);
        }

        trigger.addEventListener("click", (e) => {
            e.stopPropagation();
            menu.classList.toggle("rw-hidden");
        });

        dropdown.appendChild(trigger);
        dropdown.appendChild(menu);
        addField("Statut", dropdown);
    }

    if (road) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = road;
        addField("Route", val);
    }

    {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = `${start} — ${end}`;
        addField("Période", val);
    }

    if (rw.opendata?.latitude && rw.opendata?.longitude) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = `${rw.opendata?.latitude.toFixed(6)}, ${rw.opendata?.longitude.toFixed(6)}`;
        addField("Coordonnées", val);
    }

    if (desc) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = desc;
        addField("Description", val);
    }

    if (impact) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.style.color = "#b45309";
        val.textContent = impact;
        addField("Impact circulation", val);
    }

    if (rw.url) {
        const a = document.createElement("a");
        a.href = rw.url;
        a.target = "_blank";
        a.style.color = "#4a90d9";
        a.textContent = "Voir la source";
        addField("Source", a);
    }

    detailOverlayEl.classList.remove("rw-hidden");
    detailPanelEl.classList.remove("rw-hidden");
}

function hideDetailPanel() {
    if (detailPanelEl) {
        detailPanelEl.classList.add("rw-hidden");
    }
    if (detailOverlayEl) {
        detailOverlayEl.classList.add("rw-hidden");
    }
}

function deselectRoadwork() {
    selectedRoadworkId = null;
    hideDetailPanel();
    renderRoadworksToMap(currentRoadworks);
}

function clearMapFeatures() {
    for (const status of ALL_STATUSES) {
        try {
            wmeSDK.Map.removeAllFeaturesFromLayer({layerName: getLayerName(status)});
        } catch (_) {
        }
    }
}

function renderAllGroupsToMap() {
    if (!wmeSDK) return;
    try {
        wmeSDK.Map.removeAllFeaturesFromLayer({layerName: WKT_LAYER});
    } catch (_) {}
    const allFeatures = [];
                for (const group of Object.values(polygonGroups as Record<string, any>)) {
        if (!group.visible) continue;
        for (const feature of group.features) {
            allFeatures.push(feature);
        }
    }
    if (allFeatures.length === 0) return;
    try {
        const checked = wmeSDK.LayerSwitcher.isLayerCheckboxChecked({name: WKT_LAYER});
        if (!checked) return;
    } catch (_) {}
    try {
        wmeSDK.Map.addFeaturesToLayer({features: allFeatures, layerName: WKT_LAYER});
    } catch (e) {
        console.warn("[Roadwork] Failed to add WKT features:", e);
    }
}

function clearAllPolygonGroups() {
    polygonGroups = {};
    nextGroupId = 0;
    savePolygonGroups();
    try {
        wmeSDK.Map.removeAllFeaturesFromLayer({layerName: WKT_LAYER});
    } catch (_) {}
    const statusEl = document.getElementById("rw-wkt-status");
    if (statusEl) statusEl.textContent = "Aucun fichier chargé";
    updatePolygonesPanel();
}

async function clearExtensionStorage() {
    if (!confirm("Vider tout le stockage local de l'extension Roadwork ?")) return;
    try {
        for (const key of Object.keys(localStorage)) {
            if (key.startsWith("roadwork-wme-")) {
                localStorage.removeItem(key);
            }
        }
    } catch (_) {}
    await syncCustomDescriptorsToWasm(true).catch(() => {});
    window.location.reload();
}

function buildWktMarkerIcon() {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="32" viewBox="0 0 24 32">
            <path d="M2 28 L22 28 L21 30 L3 30 Z" fill="#333" />
            <path d="M12 2 L3 28 L21 28 Z" fill="#8b5cf6" stroke="#8b5cf6" stroke-width="1" />
            <path d="M9.3 10 L14.7 10 L16.3 15 L7.7 15 Z" fill="white" opacity="0.9" />
            <path d="M7 19 L17 19 L18.7 24 L5.3 24 Z" fill="white" opacity="0.9" />
        </svg>`;
    return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

function buildStyleRulesForWktLayer() {
    const color = "#8b5cf6";
    const strokeColor = "#7c3aed";
    return [
        {
            predicate: (props) => props.geomType === "Polygon",
            style: {
                fillColor: color,
                fillOpacity: 0.3,
                strokeColor: strokeColor,
                strokeOpacity: 0.8,
                strokeWidth: 2,
            },
        },
        {
            predicate: (props) => props.geomType === "LineString",
            style: {
                strokeColor: strokeColor,
                strokeOpacity: 0.8,
                strokeWidth: 2,
            },
        },
        {
            predicate: (props) => props.geomType === "Point",
            style: {
                icon: buildWktMarkerIcon(),
                iconWidth: 24,
                iconHeight: 32,
                iconOffsetX: -12,
                iconOffsetY: -32,
            },
        },
    ];
}

function buildStyleRulesForLayer(status: string) {
    const color: string = STATUS_COLORS[status];
    const rules = [];

    if (selectedRoadworkId) {
        const selRw = currentRoadworks[selectedRoadworkId];
        if (selRw && (selRw.syncData?.status || "New") === status) {
            rules.push({
                predicate: (props) => {
                    return props.roadworkId === selectedRoadworkId && props.geomType === "Polygon";
                },
                style: {
                    fillColor: "#2dd4bf",
                    fillOpacity: 0.5,
                    strokeColor: "#ffffff",
                    strokeOpacity: 1,
                    strokeWidth: 4,
                },
            });
            rules.push({
                predicate: (props) => {
                    return props.roadworkId === selectedRoadworkId && props.geomType === "Point";
                },
                style: {
                    icon: buildMarkerIcon(status),
                    iconWidth: 30,
                    iconHeight: 40,
                    iconOffsetX: -15,
                    iconOffsetY: -40,
                },
            });
        }
    }

    rules.push({
        predicate: (props) => props.geomType === "Polygon",
        style: {
            fillColor: color,
            fillOpacity: 0.3,
            strokeColor: color,
            strokeOpacity: 0.8,
            strokeWidth: 2,
        },
    });
    rules.push({
        predicate: (props) => props.geomType === "Point",
        style: {
            icon: buildMarkerIcon(status),
            iconWidth: 24,
            iconHeight: 32,
            iconOffsetX: -12,
            iconOffsetY: -32,
        },
    });

    return rules;
}

function isLayerChecked(status: string) {
    try {
        return wmeSDK.LayerSwitcher.isLayerCheckboxChecked({name: getLayerName(status)});
    } catch (_) {
        return false;
    }
}

function renderRoadworksToMap(roadworks) {
    clearMapFeatures();

    const featuresByStatus = {};
    for (const status of ALL_STATUSES) {
        featuresByStatus[status] = [];
    }

    let totalFeatures = 0;

    for (const [id, rw] of Object.entries(roadworks as Record<string, any>)) {
        const status = rw.syncData?.status || "New";
        const features = featuresByStatus[status] || featuresByStatus["New"];

        if (rw.opendata?.polygons && rw.opendata?.polygons.length > 0) {
            for (let polyIdx = 0; polyIdx < rw.opendata?.polygons.length; polyIdx++) {
                const polygon = rw.opendata?.polygons[polyIdx];
                if (
                    !polygon.xpoints ||
                    !polygon.ypoints ||
                    polygon.xpoints.length < 3
                ) {
                    continue;
                }
                const coords = [];
                for (let i = 0; i < polygon.xpoints.length; i++) {
                    coords.push([polygon.xpoints[i], polygon.ypoints[i]]);
                }
                if (
                    coords.length > 0 &&
                    (coords[0][0] !== coords[coords.length - 1][0] ||
                        coords[0][1] !== coords[coords.length - 1][1])
                ) {
                    coords.push(coords[0]);
                }
                features.push({
                    id: `roadwork-polygon-${id}-${polyIdx}`,
                    type: "Feature",
                    geometry: {
                        type: "Polygon",
                        coordinates: [coords],
                    },
                    properties: {
                        roadworkId: id,
                        geomType: "Polygon",
                    },
                });
            }
        }
        if ((!rw.opendata?.polygons || rw.opendata?.polygons.length === 0) && rw.opendata?.latitude && rw.opendata?.longitude) {
            features.push({
                id: `roadwork-marker-${id}`,
                type: "Feature",
                geometry: {
                    type: "Point",
                    coordinates: [rw.opendata?.longitude, rw.opendata?.latitude],
                },
                properties: {
                    roadworkId: id,
                    geomType: "Point",
                },
            });
        }
    }

    for (const status of ALL_STATUSES) {
        const features = featuresByStatus[status];
        if (features.length === 0 || !isLayerChecked(status)) {
            continue;
        }
        try {
            wmeSDK.Map.addFeaturesToLayer({
                features: features,
                layerName: getLayerName(status),
            });
        } catch (e) {
            console.warn("[Roadwork] Failed to add features to layer " + status + ":", e);
        }
        totalFeatures += features.length;
    }

    setCount(totalFeatures);
}

async function refreshData() {
    setStatus("Loading...");
    try {
        clearCache(settings.service);
        const data = await fetchRoadworks(true);
        currentRoadworks = data.roadworks || {};
        applyStatusOverrides();
        console.info("[Roadwork] refreshData: currentRoadworks count", Object.keys(currentRoadworks).length);
        if (selectedRoadworkId && !currentRoadworks[selectedRoadworkId]) {
            selectedRoadworkId = null;
            hideDetailPanel();
        }
        renderRoadworksToMap(currentRoadworks);
        updateFloatingTable();
        const count = Object.keys(currentRoadworks).length;
        setStatus(`${count} roadwork(s) loaded`, "success");
        const now = Date.now();
        try {
            localStorage.setItem(LAST_REFRESH_KEY, String(now));
        } catch (_) {}
        if (lastRefreshEl) {
            lastRefreshEl.textContent = new Date(now).toLocaleString("fr-FR");
        }
    } catch (e) {
        setStatus(e.message, "error");
    }
}

function populateServiceSelect(selectEl, services) {
    selectEl.replaceChildren();
    for (const svc of services) {
        const opt = document.createElement("option");
        opt.value = svc.name;
        opt.textContent = svc.name;
        if (svc.name === settings.service) {
            opt.selected = true;
        }
        selectEl.appendChild(opt);
    }
}

let toolbarEl = null;
let polygonesPanelEl = null;
let polygonesToggleBtn = null;
let polygonesPanelBody = null;
let polygonesDropzoneEl = null;

function addPolygonGroup(name: string, features) {
    const gid = "group_" + nextGroupId;
    const prefixed = features.map(f => ({ ...f, id: gid + "-" + f.id }));
    polygonGroups[gid] = { id: gid, name, features: prefixed, visible: true };
    nextGroupId++;
    savePolygonGroups();
    renderAllGroupsToMap();
    updatePolygonesPanel();
    return gid;
}

function removePolygonGroup(id) {
    delete polygonGroups[id];
    savePolygonGroups();
    renderAllGroupsToMap();
    updatePolygonesPanel();
}

function togglePolygonGroup(id) {
    const g = polygonGroups[id];
    if (!g) return;
    g.visible = !g.visible;
    savePolygonGroups();
    renderAllGroupsToMap();
    updatePolygonesPanel();
}

function renamePolygonGroup(id, newName: string) {
    const g = polygonGroups[id];
    if (!g) return;
    g.name = newName;
    savePolygonGroups();
    updatePolygonesPanel();
}

function createPolygonesUI() {
    polygonesToggleBtn = document.createElement("button");
    polygonesToggleBtn.className = "rw-polygones-toggle-btn";
    polygonesToggleBtn.textContent = "Polygones";
    polygonesToggleBtn.addEventListener("click", () => {
        polygonesPanelEl.classList.remove("rw-hidden");
        polygonesToggleBtn.style.display = "none";
        updatePolygonesPanel();
    });
    if (toolbarEl) toolbarEl.appendChild(polygonesToggleBtn);

    polygonesPanelEl = document.createElement("div");
    polygonesPanelEl.className = "rw-polygones-panel rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-polygones-header";

    const title = document.createElement("h4");
    title.textContent = "Polygones";

    const headerBtns = document.createElement("div");
    headerBtns.style.cssText = "display:flex;gap:4px;";

    const resetBtn = document.createElement("button");
    resetBtn.textContent = "Reset";
    resetBtn.title = "Supprimer tous les polygones";
    resetBtn.addEventListener("click", () => {
        if (confirm("Supprimer tous les polygones ?")) {
            clearAllPolygonGroups();
        }
    });

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = "Fermer";
    closeBtn.addEventListener("click", () => {
        polygonesPanelEl.classList.add("rw-hidden");
        polygonesToggleBtn.style.display = "block";
    });

    headerBtns.appendChild(resetBtn);
    headerBtns.appendChild(closeBtn);
    header.appendChild(title);
    header.appendChild(headerBtns);

    polygonesPanelBody = document.createElement("div");
    polygonesPanelBody.className = "rw-polygones-body";

    polygonesDropzoneEl = document.createElement("div");
    polygonesDropzoneEl.className = "rw-polygones-dropzone";
    polygonesDropzoneEl.textContent = "D\u00e9posez un fichier .wkt ici";

    polygonesPanelEl.appendChild(header);
    polygonesPanelEl.appendChild(polygonesPanelBody);
    polygonesPanelEl.appendChild(polygonesDropzoneEl);
    document.body.appendChild(polygonesPanelEl);

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = polygonesPanelEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        polygonesPanelEl.style.left = e.clientX - dragOffsetX + "px";
        polygonesPanelEl.style.top = e.clientY - dragOffsetY + "px";
        polygonesPanelEl.style.right = "auto";
        polygonesPanelEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });
}

function updatePolygonesPanel() {
    if (!polygonesPanelBody) return;
    polygonesPanelBody.replaceChildren();
    const entries = Object.values(polygonGroups as Record<string, any>);
    if (entries.length === 0) {
        const empty = document.createElement("div");
        empty.className = "rw-polygones-empty";
        empty.textContent = "Aucun polygone charg\u00e9";
        polygonesPanelBody.appendChild(empty);
        return;
    }
    for (const group of entries) {
        const row = document.createElement("div");
        row.className = "rw-polygon-group-row";

        const toggleCheck = document.createElement("input");
        toggleCheck.type = "checkbox";
        toggleCheck.className = "rw-polygon-group-toggle";
        toggleCheck.checked = group.visible;
        toggleCheck.title = group.visible ? "Masquer" : "Afficher";
        toggleCheck.addEventListener("change", () => togglePolygonGroup(group.id));

        const nameInput = document.createElement("input");
        nameInput.className = "rw-polygon-group-name";
        nameInput.type = "text";
        nameInput.value = group.name;
        nameInput.addEventListener("blur", () => {
            if (nameInput.value.trim() && nameInput.value !== group.name) {
                renamePolygonGroup(group.id, nameInput.value.trim());
            } else {
                nameInput.value = group.name;
            }
        });
        nameInput.addEventListener("keydown", (e) => {
            if (e.key === "Enter") nameInput.blur();
            if (e.key === "Escape") { nameInput.value = group.name; nameInput.blur(); }
        });

        const countSpan = document.createElement("span");
        countSpan.className = "rw-polygon-group-count";
        const geomCounts = {};
        for (const f of group.features) {
            const t = f.properties.geomType;
            geomCounts[t] = (geomCounts[t] || 0) + 1;
        }
        const parts = Object.entries(geomCounts).map(([t, c]) => `${c} ${t}`);
        countSpan.textContent = "(" + parts.join(", ") + ")";

        const deleteBtn = document.createElement("button");
        deleteBtn.className = "rw-polygon-group-delete";
        deleteBtn.textContent = "\uD83D\uDDD1";
        deleteBtn.title = "Supprimer";
        deleteBtn.addEventListener("click", () => {
            if (confirm("Supprimer le groupe \u00ab " + group.name + " \u00bb ?")) {
                removePolygonGroup(group.id);
            }
        });

        row.appendChild(nameInput);
        row.appendChild(countSpan);
        row.appendChild(deleteBtn);
        polygonesPanelBody.appendChild(row);
    }
}

function setupPolygonesDragDrop() {
    let dragCounter = 0;

    document.addEventListener("dragenter", (e) => {
        e.preventDefault();
        dragCounter++;
        if (dragCounter === 1) {
            polygonesPanelEl.classList.remove("rw-hidden");
            polygonesToggleBtn.style.display = "none";
            updatePolygonesPanel();
            polygonesDropzoneEl.classList.add("rw-polygones-dropzone-active");
        }
    });

    document.addEventListener("dragover", (e) => {
        e.preventDefault();
    });

    document.addEventListener("dragleave", (e) => {
        e.preventDefault();
        dragCounter--;
        if (dragCounter <= 0) {
            dragCounter = 0;
            polygonesDropzoneEl.classList.remove("rw-polygones-dropzone-active");
        }
    });

    document.addEventListener("drop", (e) => {
        e.preventDefault();
        dragCounter = 0;
        polygonesDropzoneEl.classList.remove("rw-polygones-dropzone-active");
        const files = e.dataTransfer?.files;
        if (!files || files.length === 0) return;
        const file = files[0];
        if (!file.name.match(/\.(wkt|txt)$/i)) {
            alert("Veuillez d\u00e9poser un fichier .wkt ou .txt");
            return;
        }
        const reader = new FileReader();
        reader.onload = (evt) => {
            const text = evt.target.result;
            const features = parseWkt(text);
            if (features.length === 0) {
                alert("Aucune g\u00e9om\u00e9trie valide trouv\u00e9e dans le fichier");
                return;
            }
            const fileName = file.name.replace(/\.[^/.]+$/, "");
            addPolygonGroup(fileName, features);
            updatePolygonesPanel();
            if (features.length > 0 && wmeSDK?.Map) {
                const first = features[0];
                if (first.geometry.type === 'Point') {
                    const [lon, lat] = first.geometry.coordinates;
                    wmeSDK.Map.setMapCenter({lonLat: {lon, lat}});
                } else {
                    const coords = first.geometry.type === 'Polygon' ? first.geometry.coordinates[0] : first.geometry.coordinates;
                    if (coords && coords.length > 0) {
                        const mid = Math.floor(coords.length / 2);
                        const [lon, lat] = coords[mid];
                        wmeSDK.Map.setMapCenter({lonLat: {lon, lat}});
                    }
                }
            }
        };
        reader.readAsText(file);
    });
}

async function buildPanel(tabPane: Element) {
    panelEl = document.createElement("div");
    panelEl.className = "roadwork-panel";
    const heading = document.createElement("h3");
    heading.textContent = "Roadwork Settings";
    panelEl.appendChild(heading);

    const versionLine = document.createElement("div");
    versionLine.style.cssText = "font-size:11px;color:#888;margin-bottom:8px;";
    versionLine.textContent = "v__VERSION__ — built __BUILD_DATE__";
    panelEl.appendChild(versionLine);

    const field1 = document.createElement("div");
    field1.className = "roadwork-field";
    const lbl1 = document.createElement("label");
    lbl1.textContent = "Service";
    const flexDiv = document.createElement("div");
    flexDiv.style.cssText = "display:flex;gap:4px;";
    const sel = document.createElement("select");
    sel.id = "rw-service-select";
    sel.style.flex = "1";
    const srvRefreshBtn = document.createElement("button");
    srvRefreshBtn.className = "roadwork-btn roadwork-btn-icon";
    srvRefreshBtn.id = "rw-service-refresh-btn";
    srvRefreshBtn.title = "Refresh services list";
    srvRefreshBtn.textContent = "↻";
    flexDiv.appendChild(sel);
    flexDiv.appendChild(srvRefreshBtn);
    field1.appendChild(lbl1);
    field1.appendChild(flexDiv);
    panelEl.appendChild(field1);

    const customField = document.createElement("div");
    customField.className = "roadwork-field";
    const customLbl = document.createElement("label");
    customLbl.textContent = "Custom sources (index.json URLs)";
    customField.appendChild(customLbl);

    const customRow = document.createElement("div");
    customRow.style.cssText = "display:flex;gap:4px;";
    const customInput = document.createElement("input");
    customInput.type = "url";
    customInput.placeholder = "https://example.com/index.json";
    customInput.style.flex = "1";
    const addBtn = document.createElement("button");
    addBtn.className = "roadwork-btn roadwork-btn-secondary";
    addBtn.textContent = "Add";
    customRow.appendChild(customInput);
    customRow.appendChild(addBtn);
    customField.appendChild(customRow);

    const customList = document.createElement("div");
    customList.className = "roadwork-custom-sources";
    customField.appendChild(customList);

    panelEl.appendChild(customField);

    const field2 = document.createElement("div");
    field2.className = "roadwork-field";
    const lbl2 = document.createElement("label");
    lbl2.textContent = "Dernier rafraîchissement";
    const lastRefreshSpan = document.createElement("span");
    lastRefreshSpan.id = "rw-last-refresh";
    lastRefreshSpan.style.color = "#666";
    lastRefreshSpan.textContent = "—";
    field2.appendChild(lbl2);
    field2.appendChild(lastRefreshSpan);
    panelEl.appendChild(field2);

    const field3 = document.createElement("div");
    field3.className = "roadwork-field";

    const btnDiv = document.createElement("div");
    const refBtn = document.createElement("button");
    refBtn.className = "roadwork-btn roadwork-btn-secondary";
    refBtn.id = "rw-refresh-btn";
    refBtn.textContent = "Refresh now";
    btnDiv.appendChild(refBtn);
    const debugBtn = document.createElement("button");
    debugBtn.className = "roadwork-btn roadwork-btn-secondary";
    debugBtn.id = "rw-debug-btn";
    debugBtn.textContent = "Debug";
    debugBtn.title = "Open the descriptor helper for the current service";
    btnDiv.appendChild(debugBtn);
    panelEl.appendChild(btnDiv);

    const lbl3 = document.createElement("label");
    lbl3.textContent = "Log level";
    const logLevelSel = document.createElement("select");
    logLevelSel.id = "rw-loglevel-select";
    for (const opt of ["error", "warn", "info", "debug", "trace"]) {
        const o = document.createElement("option");
        o.value = opt;
        o.textContent = opt;
        if (opt === settings.logLevel) {
            o.selected = true;
        }
        logLevelSel.appendChild(o);
    }
    logLevelSel.addEventListener("change", () => {
        settings.logLevel = logLevelSel.value;
        saveSettings();
        applyLogLevel(settings.logLevel);
    });
    field3.appendChild(lbl3);
    field3.appendChild(logLevelSel);
    panelEl.appendChild(field3);

    const statDiv = document.createElement("div");
    statDiv.id = "rw-status";
    statDiv.className = "roadwork-status";
    panelEl.appendChild(statDiv);

    const cntDiv = document.createElement("div");
    cntDiv.id = "rw-count";
    cntDiv.className = "roadwork-count";
    panelEl.appendChild(cntDiv);

    tabPane.appendChild(panelEl);

    statusEl = panelEl.querySelector("#rw-status");
    countEl = tabPane.querySelector("#rw-count") || panelEl.querySelector("#rw-count");
    lastRefreshEl = panelEl.querySelector("#rw-last-refresh");

    const serviceSelect = panelEl.querySelector("#rw-service-select");
    const refreshBtn = panelEl.querySelector("#rw-refresh-btn");
    const serviceRefreshBtn = panelEl.querySelector("#rw-service-refresh-btn");

    async function populateServices() {
        const services = await fetchServices();
        servicesData = services;
        if (services.length > 0) {
            populateServiceSelect(serviceSelect, services);
        } else {
            const opt = document.createElement("option");
            opt.value = settings.service;
            opt.textContent = settings.service;
            serviceSelect.appendChild(opt);
        }
    }
    await populateServices();

    serviceSelect.addEventListener("change", async () => {
        const newService = serviceSelect.value;
        if (newService === settings.service) return;

        clearCache(settings.service);

        settings.service = newService;
        saveSettings();

        currentRoadworks = {};
        selectedRoadworkId = null;
        hideDetailPanel();
        clearMapFeatures();
        updateFloatingTable();

        setStatus("Loading...");
        try {
            const data = await fetchRoadworks(true);
            currentRoadworks = data.roadworks || {};
            applyStatusOverrides();
            renderRoadworksToMap(currentRoadworks);
            updateFloatingTable();
            const count = Object.keys(currentRoadworks).length;
            setStatus(`${count} roadwork(s) loaded`, "success");
            const now = Date.now();
            try {
                localStorage.setItem(LAST_REFRESH_KEY, String(now));
            } catch (_) {}
            if (lastRefreshEl) {
                lastRefreshEl.textContent = new Date(now).toLocaleString("fr-FR");
            }
        } catch (e) {
            setStatus(e.message, "error");
        }

        const svcInfo = servicesData.find((s) => s.name === newService);
        if (svcInfo?.center && wmeSDK?.Map?.setMapCenter) {
            wmeSDK.Map.setMapCenter({
                lonLat: { lon: svcInfo.center.lon, lat: svcInfo.center.lat },
                zoomLevel: 12,
            });
        }
    });

    serviceRefreshBtn.addEventListener("click", async () => {
        setStatus("Refreshing services...");
        await syncCustomDescriptorsToWasm(true);
        const services = await fetchServices(true);
        servicesData = services;
        if (services.length > 0) {
            populateServiceSelect(serviceSelect, services);
            setStatus("Services refreshed", "success");
        } else {
            setStatus("Failed to refresh services", "error");
        }
    });

    refreshBtn.addEventListener("click", () => {
        refreshData();
    });

    const debugBtnEl = panelEl.querySelector("#rw-debug-btn");
    debugBtnEl.addEventListener("click", openDescriptorHelper);

    function renderCustomSources() {
        customList.replaceChildren();
        const sources = Array.isArray(settings.customSources) ? settings.customSources : [];
        for (const url of sources) {
            const row = document.createElement("div");
            row.className = "roadwork-custom-source-row";
            const span = document.createElement("span");
            span.textContent = url;
            span.style.flex = "1";
            span.style.wordBreak = "break-all";
            const del = document.createElement("button");
            del.className = "roadwork-btn roadwork-btn-icon";
            del.textContent = "\u00d7";
            del.title = "Remove";
            del.addEventListener("click", async () => {
                settings.customSources = settings.customSources.filter((s) => s !== url);
                saveSettings();
                renderCustomSources();
                setStatus("Reloading services...");
                await syncCustomDescriptorsToWasm(true);
                const services = await fetchServices(true);
                servicesData = services;
                if (services.length > 0) {
                    populateServiceSelect(serviceSelect, services);
                    setStatus("Services refreshed", "success");
                } else {
                    setStatus("Failed to load custom sources", "error");
                }
            });
            row.appendChild(span);
            row.appendChild(del);
            customList.appendChild(row);
        }
    }

    addBtn.addEventListener("click", async () => {
        const url = customInput.value.trim();
        if (!/^https?:\/\//i.test(url)) {
            setStatus("URL must be absolute http(s)", "error");
            return;
        }
        if (settings.customSources.includes(url)) return;
        settings.customSources = [...settings.customSources, url];
        saveSettings();
        customInput.value = "";
        renderCustomSources();
        setStatus("Loading custom sources...");
        await syncCustomDescriptorsToWasm(true);
        const services = await fetchServices(true);
        servicesData = services;
        if (services.length > 0) {
            populateServiceSelect(serviceSelect, services);
            setStatus("Services refreshed", "success");
        } else {
            setStatus("Failed to load custom sources", "error");
        }
    });

    renderCustomSources();
}

async function init() {
    await applyLogLevel(settings.logLevel);

    loadSettings();
    loadHideFinished();
    loadSortState();
    await syncCustomDescriptorsToWasm(false).catch((e) => {
        console.warn("[Roadwork] Failed to sync custom descriptors:", e);
    });

    document.addEventListener("click", () => {
        document.querySelectorAll(".rw-status-dropdown-menu:not(.rw-hidden)").forEach(menu => {
            menu.classList.add("rw-hidden");
        });
    });

    toolbarEl = document.createElement("div");
    toolbarEl.className = "rw-toolbar";
    const grip = document.createElement("div");
    grip.className = "rw-toolbar-grip";
    grip.title = "Déplacer";
    grip.setAttribute("aria-label", "Déplacer la barre d'outils");
    toolbarEl.appendChild(grip);
    document.body.appendChild(toolbarEl);
    (() => {
        let isDragging = false;
        let dragOffsetX = 0;
        let dragOffsetY = 0;
        grip.addEventListener("mousedown", (e) => {
            isDragging = true;
            const rect = toolbarEl.getBoundingClientRect();
            dragOffsetX = e.clientX - rect.left;
            dragOffsetY = e.clientY - rect.top;
            e.preventDefault();
        });
        toolbarEl.addEventListener("mousedown", (e) => {
            if (e.target.tagName === "BUTTON" || e.target.tagName === "INPUT") return;
            isDragging = true;
            const rect = toolbarEl.getBoundingClientRect();
            dragOffsetX = e.clientX - rect.left;
            dragOffsetY = e.clientY - rect.top;
            e.preventDefault();
        });
        document.addEventListener("mousemove", (e) => {
            if (!isDragging) return;
            toolbarEl.style.left = e.clientX - dragOffsetX + "px";
            toolbarEl.style.top = e.clientY - dragOffsetY + "px";
            toolbarEl.style.right = "auto";
            toolbarEl.style.bottom = "auto";
        });
        document.addEventListener("mouseup", () => {
            isDragging = false;
        });
    })();
    createFloatingPanel();
    createDetailPanel();
    updateLastRefreshDisplay();

    const result = await wmeSDK.Sidebar.registerScriptTab();
    result.tabLabel.innerText = "Roadwork";

    const tabPane = result?.tabPane;

    if (tabPane) {
        await buildPanel(tabPane);
    } else {
        console.warn("[Roadwork] tabPane not found in result, falling back to event listeners");
        wmeSDK.Events.on({
            eventName: "wme-sidebar-tab-opened",
            eventHandler: async (evt) => {
                if (
                    (evt && String(evt.tabName) === SCRIPT_ID) ||
                    (evt && evt.domId && evt.domId.includes(SCRIPT_ID))
                ) {
                    if (!panelEl) {
                        const pane =
                            document.querySelector(`#tab-${SCRIPT_ID}-pane`) ||
                            document.querySelector(
                                `[data-tab-id="${SCRIPT_ID}"]`
                            );
                        if (pane) {
                            await buildPanel(pane);
                        }
                    }
                }
            },
        });
        wmeSDK.Events.once({eventName: "wme-ready"}).then(async () => {
            const pane =
                document.querySelector(`#tab-${SCRIPT_ID}-pane`) ||
                document.querySelector(`[data-tab-id="${SCRIPT_ID}"]`);
            if (pane && !panelEl) {
                await buildPanel(pane);
            }
        });
    }

    for (const status of ALL_STATUSES) {
        const layerName = getLayerName(status);
        try {
            wmeSDK.Map.addLayer({
                layerName: layerName,
                styleRules: buildStyleRulesForLayer(status),
            });
        } catch (e) {
            console.warn("[Roadwork] Failed to add layer " + status + ":", e);
        }

        try {
            wmeSDK.Events.trackLayerEvents({layerName: layerName});
        } catch (e) {
            console.warn("[Roadwork] Failed to track layer events for " + status + ":", e);
        }

        try {
            wmeSDK.LayerSwitcher.addLayerCheckbox({
                name: layerName,
                isChecked: true,
            });
        } catch (e) {
            console.warn("[Roadwork] Failed to add layer checkbox for " + status + ":", e);
        }
    }

    try {
        wmeSDK.Map.addLayer({
            layerName: WKT_LAYER,
            styleRules: buildStyleRulesForWktLayer(),
        });
    } catch (e) {
        console.warn("[Roadwork] Failed to add WKT layer:", e);
    }

    try {
        wmeSDK.Events.trackLayerEvents({layerName: WKT_LAYER});
    } catch (e) {
        console.warn("[Roadwork] Failed to track WKT layer events:", e);
    }

    try {
        wmeSDK.LayerSwitcher.addLayerCheckbox({
            name: WKT_LAYER,
            isChecked: true,
        });
    } catch (e) {
        console.warn("[Roadwork] Failed to add WKT layer checkbox:", e);
    }

    const restored = loadPolygonGroups();
    const hasRestored = Object.keys(restored).length > 0;
    if (hasRestored) {
        polygonGroups = restored;
        const wktStatus = document.getElementById("rw-wkt-status");
        if (wktStatus) {
            const groupCount = Object.keys(restored).length;
            const featCount = Object.values(restored as Record<string, any>).reduce((s, g) => s + g.features.length, 0);
            wktStatus.textContent = `${groupCount} groupe(s), ${featCount} g\u00e9om\u00e9trie(s)`;
        }
    }
    renderAllGroupsToMap();

    createPolygonesUI();
    setupPolygonesDragDrop();
    if (hasRestored) {
        updatePolygonesPanel();
    }

    wmeSDK.Events.on({
        eventName: "wme-layer-checkbox-toggled",
        eventHandler: (evt) => {
            if (evt && evt.name && evt.name.startsWith("Roadwork - ")) {
                renderRoadworksToMap(currentRoadworks);
            }
            if (evt && evt.name === WKT_LAYER) {
                renderAllGroupsToMap();
            }
        },
    });

    wmeSDK.Events.on({
        eventName: "wme-layer-feature-clicked",
        eventHandler: (evt) => {
            if (!evt || !evt.layerName || !evt.layerName.startsWith("Roadwork - ")) {
                return;
            }
            const featureId = evt.featureId as string;
            if (!featureId) {
                return;
            }

            if (evt.layerName === WKT_LAYER) {
                let feature = null;
    for (const group of Object.values(polygonGroups as Record<string, any>)) {
                    feature = group.features.find(f => f.id === featureId);
                    if (feature) break;
                }
                if (feature && wmeSDK?.Map) {
                    const geom = feature.geometry;
                    if (geom.type === 'Point') {
                        const [lon, lat] = geom.coordinates;
                        wmeSDK.Map.setMapCenter({lonLat: {lon, lat}});
                    } else if (geom.type === 'Polygon') {
                        const coords = geom.coordinates[0];
                        if (coords && coords.length > 0) {
                            const mid = Math.floor(coords.length / 2);
                            const [lon, lat] = coords[mid];
                            wmeSDK.Map.setMapCenter({lonLat: {lon, lat}});
                        }
                    } else if (geom.type === 'LineString') {
                        const coords = geom.coordinates;
                        if (coords && coords.length > 0) {
                            const mid = Math.floor(coords.length / 2);
                            const [lon, lat] = coords[mid];
                            wmeSDK.Map.setMapCenter({lonLat: {lon, lat}});
                        }
                    }
                }
                return;
            }

            let rwId = null;
            if (featureId.startsWith("roadwork-marker-")) {
                rwId = featureId.replace("roadwork-marker-", "");
            } else if (featureId.startsWith("roadwork-polygon-")) {
                const rest = featureId.slice("roadwork-polygon-".length);
                rwId = rest.slice(0, rest.lastIndexOf("-"));
            }
            if (rwId && currentRoadworks[rwId]) {
                if (selectedRoadworkId === rwId) {
                    deselectRoadwork();
                } else {
                    selectedRoadworkId = rwId;
                    showDetailPanel(currentRoadworks[rwId]);
                    renderRoadworksToMap(currentRoadworks);
                }
            }
        },
    });

    const cached = loadCache(settings.service);
    if (cached) {
        currentRoadworks = cached.roadworks || {};
        applyStatusOverrides();
        renderRoadworksToMap(currentRoadworks);
        updateFloatingTable();
        const count = Object.keys(currentRoadworks).length;
        setStatus(`${count} roadwork(s) from cache`, "success");
    }
    refreshData();
    console.log("Roadwork init done");
}
