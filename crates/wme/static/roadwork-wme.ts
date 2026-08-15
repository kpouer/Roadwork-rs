type WmeSDK = import("wme-sdk-typings").WmeSDK;

const wasmIframe = document.getElementById('roadwork-wasm-iframe');

let rpcId = 0;
const rpcPending = new Map();
let helperAcked = false;

function postHelperMessage(msg: any) {
    const helper = document.querySelector(
        "iframe.rw-helper-iframe",
    ) as HTMLIFrameElement | null;
    helper?.contentWindow?.postMessage(msg, "*");
}

window.addEventListener("message", async (e) => {
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
    } else if (e.data?.type === "ROADWORK_SAVE_OPENDATA_DESCRIPTOR") {
        const result = await saveOpendataDescriptorFromHelper(
            e.data.name,
            e.data.descriptor,
            e.data.oldName,
            e.data.data,
        );
        if (result.ok) {
            dataSource = result.name!;
            saveDataSource();
            updateDataPanel();
            postHelperMessage({ type: "ROADWORK_SAVE_DONE", name: result.name });
        } else {
            postHelperMessage({ type: "ROADWORK_SAVE_ERROR", error: result.error });
        }
    } else if (e.data?.type === "ROADWORK_DELETE_OPENDATA_SERVICE") {
        removeOpendataService(e.data.name);
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
const SERVICES_CACHE_KEY = "roadwork-wme-services-cache";
const CUSTOM_SOURCES_CACHE_KEY = "roadwork-wme-custom-sources-cache";
const STATUS_OVERRIDES_KEY = "roadwork-wme-status-overrides";
const DATA_SOURCE_KEY = "roadwork-wme-data-source";
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
    opendataServices: {},
};

const OPENDATA_LAYER = "Opendata";

let wmeSDK: WmeSDK | null = null;
let settings = {...DEFAULTS};
let currentRoadworks: any = {};
let currentOpendata: Record<string, any> = {};
let opendataTotals: Record<string, number> = {};
let dataSource: string = "";
let opendataFeatureIndex: Record<string, any> = {};
let opendataListEl = null;
let servicesData = [];
let panelEl: HTMLDivElement | null = null;
let statusEl: HTMLDivElement | null = null;
let lastRefreshEl = null;
let floatingPanelEl: HTMLDivElement | null = null;
let floatingTableBody: HTMLDivElement | null = null;
let floatingToggleBtn: HTMLButtonElement | null = null;
let floatingTitleEl: HTMLHeadElement | null = null;
let serviceSelectEl: HTMLSelectElement | null = null;
let selectedRoadworkId: string | null = null;
let polygonGroups: any = {};
let nextGroupId = 0;
const WKT_LAYER = "Roadwork - WKT";
let detailPanelEl: HTMLDivElement | null = null;
let hideFinished = false;
let sortColumn = -1;
let sortDirection = 'asc';
let dataPanelEl: HTMLDivElement | null = null;
let dataToggleBtn: HTMLButtonElement | null = null;
let dataTableBody: HTMLTableSectionElement | null = null;
let dataSourceSelectEl: HTMLSelectElement | null = null;
let dataUpdateBtn: HTMLButtonElement | null = null;
let dataDeleteBtn: HTMLButtonElement | null = null;
let dataEditBtn: HTMLButtonElement | null = null;
let dataStatusEl: HTMLDivElement | null = null;
let dataDropzoneEl: HTMLDivElement | null = null;
let viewportRefreshTimer = null;
let viewportRefreshInFlight = false;
let viewportRefreshPending = false;

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

async function savePolygonGroups() {
    try {
        await rpcCall("save_polygon_groups", [{ groups: polygonGroups, nextId: nextGroupId }]);
    } catch (_) {}
}

async function loadPolygonGroups() {
    try {
        const parsed = await rpcCall("get_polygon_groups");
        if (parsed && typeof parsed === "object" && parsed.groups) {
            nextGroupId = parsed.nextId || 0;
            return parsed.groups;
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
            currentRoadworks[id].sync_data = currentRoadworks[id].sync_data || {};
            currentRoadworks[id].sync_data.status = status;
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

    rw.sync_data = rw.sync_data || {};
    rw.sync_data.status = newStatus;

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
    try {
        const data = await rpcCall("get_roadworks", [settings.service, forceRefresh]);
        console.info("[Roadwork] fetchRoadworks received data, type:", typeof data, "roadworks keys:", data?.roadworks ? Object.keys(data.roadworks).length : "no roadworks field");
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

function setDataStatus(text: string, type?) {
    setStatus(text, type);
    if (dataStatusEl) {
        dataStatusEl.textContent = text;
        dataStatusEl.className = "roadwork-status" + (type ? " " + type : "");
        dataStatusEl.classList.toggle("rw-hidden", !text);
    }
    console.info("[Roadwork]", text);
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

function openOpendataHelper() {
    openHelper("opendata", true);
}

function editOpendataService(name: string) {
    const services = getOpendataServices();
    const svc = services[name];
    if (!svc || !svc.descriptor) {
        setStatus(`No descriptor for ${name}`, "error");
        return;
    }
    const cached = currentOpendata[name];
    openHelper("opendata", false, name, svc.descriptor, cached ? JSON.stringify(cached) : undefined);
}

function handleDataJsonDrop(text: string, fileName: string) {
    let parsed: any = null;
    try {
        parsed = JSON.parse(text);
    } catch (e) {
        setStatus("Invalid JSON: " + e.message, "error");
        return;
    }
    const name = parsed?.metadata?.name;
    if (typeof name === "string" && name.trim()) {
        setStatus(`Opening descriptor helper for "${name.trim()}"...`, "info");
        openHelper("opendata", false, name.trim(), text);
        return;
    }
    showDataImportChoice(text, fileName);
}

function showDataImportChoice(text: string, fileName: string) {
    const baseName = (fileName || "data").replace(/\.[^/.]+$/, "");
    const services = getOpendataServices();
    const names = Object.keys(services).sort();

    const overlay = document.createElement("div");
    overlay.className = "rw-opendata-export-overlay";

    const box = document.createElement("div");
    box.className = "rw-opendata-export-box";

    const header = document.createElement("div");
    header.className = "rw-opendata-export-header";
    const title = document.createElement("h4");
    title.textContent = "Importer les données";
    const closeBtn = document.createElement("button");
    closeBtn.className = "roadwork-btn roadwork-btn-icon";
    closeBtn.textContent = "\u00d7";
    closeBtn.addEventListener("click", () => overlay.remove());
    header.appendChild(title);
    header.appendChild(closeBtn);
    box.appendChild(header);

    const body = document.createElement("div");
    body.className = "rw-import-choice-body";

    const intro = document.createElement("div");
    intro.textContent =
        "Ce fichier contient des données brutes (pas un descripteur). " +
        "Choisissez une source à mettre à jour, ou créez-en une nouvelle.";
    body.appendChild(intro);

    const createBtn = document.createElement("button");
    createBtn.className = "rw-import-choice-create";
    createBtn.textContent = "Créer une nouvelle source";
    createBtn.addEventListener("click", () => {
        overlay.remove();
        openHelper("opendata", true, baseName, undefined, text);
    });
    body.appendChild(createBtn);

    if (names.length > 0) {
        const label = document.createElement("div");
        label.className = "rw-import-choice-label";
        label.textContent = "Mettre à jour une source existante";
        body.appendChild(label);
        for (const name of names) {
            const btn = document.createElement("button");
            btn.className = "rw-import-choice-source";
            btn.textContent = name;
            btn.title = "Ouvrir l'assistant avec ces données pour " + name;
            btn.addEventListener("click", () => {
                overlay.remove();
                openHelper("opendata", false, name, services[name]?.descriptor, text);
            });
            body.appendChild(btn);
        }
    } else {
        const empty = document.createElement("div");
        empty.className = "roadwork-opendata-empty";
        empty.textContent = "Aucune source existante.";
        body.appendChild(empty);
    }

    box.appendChild(body);
    overlay.appendChild(box);
    overlay.addEventListener("click", (e) => {
        if (e.target === overlay) overlay.remove();
    });
    document.body.appendChild(overlay);
}

function openHelper(helper: string, create: boolean = false, service?: string, descriptor?: string, data?: string) {
    const target = service || settings.service;
    console.log(`[Roadwork] openHelper, helper = ${helper}, service =`, target, ", create =", create, ", has data =", !!data);
    if (!wasmIframe) {
        setStatus("WASM iframe not available", "error");
        return;
    }
    setStatus("Opening descriptor helper...", "info");
    helperAcked = false;
    const msg: any = { type: "ROADWORK_OPEN_HELPER", helper, service: target, create };
    if (descriptor) {
        msg.descriptor = descriptor;
    }
    if (data) {
        msg.data = data;
    }
    window.postMessage(msg, "*");
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
    floatingTitleEl = title;
    title.textContent = "Roadworks";

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

    const refreshBtn = document.createElement("button");
    refreshBtn.textContent = "Refresh";
    refreshBtn.title = "Refresh";
    refreshBtn.addEventListener("click", () => refreshData());

    const resetBtn = document.createElement("button");
    resetBtn.textContent = "Reset";
    resetBtn.title = "Reset all data (clear storage)";
    resetBtn.addEventListener("click", () => clearExtensionStorage());

    const closeBtn = document.createElement("button");
    closeBtn.className = "rw-floating-close";
    closeBtn.textContent = "\u00d7";
    closeBtn.title = "Hide";
    closeBtn.addEventListener("click", () => setFloatingPanelVisible(false));

    header.appendChild(title);
    header.appendChild(filterLabel);
    header.appendChild(closeBtn);

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

    const controls = document.createElement("div");
    controls.className = "rw-floating-controls";
    serviceSelectEl = document.createElement("select");
    serviceSelectEl.id = "rw-service-select";
    serviceSelectEl.title = "Choisir le service";
    controls.appendChild(serviceSelectEl);
    controls.appendChild(refreshBtn);
    controls.appendChild(resetBtn);

    const statusDiv = document.createElement("div");
    statusDiv.id = "rw-status";
    statusDiv.className = "roadwork-status rw-floating-status";
    statusEl = statusDiv;

    floatingPanelEl.appendChild(header);
    floatingPanelEl.appendChild(controls);
    floatingPanelEl.appendChild(statusDiv);
    floatingPanelEl.appendChild(tableWrap);

    const resizeHandle = document.createElement("div");
    resizeHandle.className = "rw-resize-handle";
    floatingPanelEl.appendChild(resizeHandle);

    document.body.appendChild(floatingPanelEl);

    async function populateServices() {
        const services = await fetchServices();
        servicesData = services;
        if (services.length > 0) {
            populateServiceSelect(serviceSelectEl, services);
        } else {
            const opt = document.createElement("option");
            opt.value = settings.service;
            opt.textContent = settings.service;
            serviceSelectEl.appendChild(opt);
        }
    }
    populateServices();

    serviceSelectEl.addEventListener("change", async () => {
        const newService = serviceSelectEl.value;
        if (newService === settings.service) return;

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
            const now = Date.now();
            try {
                localStorage.setItem(LAST_REFRESH_KEY, String(now));
            } catch (_) {}
            if (lastRefreshEl) {
                lastRefreshEl.textContent = new Date(now).toLocaleString("fr-FR");
            }
            await refreshViewport();
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

    try {
        const savedSize = JSON.parse(localStorage.getItem(PANEL_SIZE_KEY));
        if (savedSize) {
            if (savedSize.w >= 300) floatingPanelEl.style.width = savedSize.w + "px";
            if (savedSize.h >= 200) floatingPanelEl.style.height = savedSize.h + "px";
        }
    } catch (_) {}

    floatingToggleBtn = document.createElement("button");
    floatingToggleBtn.className = "rw-toggle-btn";
    floatingToggleBtn.textContent = "Roadworks";
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

function updateFloatingCount() {
    if (!floatingTitleEl) return;
    floatingTitleEl.textContent = `Roadworks (${Object.keys(currentRoadworks).length})`;
}

function updateFloatingTable() {
    if (!floatingTableBody) return;
    updateFloatingCount();
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
        entries = entries.filter(([, rw]) => (rw.sync_data?.status || "New") !== "Finished");
    }

    if (sortColumn >= 0) {
        const getValue = (rw, col) => {
            switch (col) {
                case 0: return (rw.sync_data?.status || "New");
                case 1: return (rw.road || "");
                case 2: return rw.start != null ? rw.start : Infinity;
                case 3: return rw.end != null ? rw.end : Infinity;
                case 4: return (rw.opendata?.description || "");
                case 5: return (rw.impact_circulation_detail || "");
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
        const status = rw.sync_data?.status || "New";
        const color = STATUS_COLORS[status] || "#9ca3af";
        const road = rw.road || "";
        const start = formatTimestamp(rw.start);
        const end = formatTimestamp(rw.end);
        const desc = rw.opendata?.description || "";
        const impact = rw.impact_circulation_detail || "";

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

function buildCircleIcon(color, size = 20) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
            <circle cx="${size / 2}" cy="${size / 2}" r="${size / 2 - 1}" fill="${color}" stroke="#ffffff" stroke-width="2" />
        </svg>`;
    return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

function buildMarkerIcon(status, size = 20) {
    return buildCircleIcon(STATUS_COLORS[status] || "#9ca3af", size);
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

function findMatchingParen(str: string, openIdx: number) {
    let depth = 1;
    let i = openIdx;
    while (depth > 0 && i < str.length - 1) {
        i++;
        if (str[i] === '(') depth++;
        else if (str[i] === ')') depth--;
    }
    return i;
}

function parseCoordList(str: string) {
    return str.split(',').map(p => p.trim().split(/\s+/).map(Number)).filter(c => c.length >= 2 && isFinite(c[0]) && isFinite(c[1]));
}

function splitTopLevelParens(str: string) {
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

function parseRings(str: string) {
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
    const impact = rw.impact_circulation_detail || "";
    const status = rw.sync_data?.status || "New";

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
    html += `</div>`;
    return html;
}

let detailOverlayEl: HTMLDivElement | null = null;

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

    const status = rw.sync_data?.status || "New";
    const color = STATUS_COLORS[status] || "#9ca3af";
    const road = rw.road || "";
    const start = formatTimestamp(rw.start);
    const end = formatTimestamp(rw.end);
    const desc = rw.opendata?.description || "";
    const impact = rw.impact_circulation_detail || "";

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
    await rpcCall("clear_all_cache", []).catch(() => {});
    await syncCustomDescriptorsToWasm(true).catch(() => {});
    window.location.reload();
}

function buildWktMarkerIcon() {
    return buildCircleIcon("#8b5cf6");
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
                iconWidth: 20,
                iconHeight: 20,
                iconOffsetX: -10,
                iconOffsetY: -10,
            },
        },
    ];
}

function buildStyleRulesForLayer(status: string) {
    const color: string = STATUS_COLORS[status];
    const rules = [];

    if (selectedRoadworkId) {
        const selRw = currentRoadworks[selectedRoadworkId];
        if (selRw && (selRw.sync_data?.status || "New") === status) {
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
                    icon: buildMarkerIcon(status, 26),
                    iconWidth: 26,
                    iconHeight: 26,
                    iconOffsetX: -13,
                    iconOffsetY: -13,
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
            iconWidth: 20,
            iconHeight: 20,
            iconOffsetX: -10,
            iconOffsetY: -10,
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

    for (const [id, rw] of Object.entries(roadworks as Record<string, any>)) {
        const status = rw.sync_data?.status || "New";
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
    }
}

function getViewportBounds() {
    if (!wmeSDK?.Map?.getMapExtent) return null;
    try {
        const extent = wmeSDK.Map.getMapExtent();
        if (!Array.isArray(extent) || extent.length < 4) return null;
        return {
            latMin: extent[1],
            lonMin: extent[0],
            latMax: extent[3],
            lonMax: extent[2],
        };
    } catch (_) {
        return null;
    }
}

async function queryRoadworksInViewport(bounds) {
    const args = [settings.service, bounds.latMin, bounds.lonMin, bounds.latMax, bounds.lonMax];
    let data = await rpcCall("get_roadworks_in_bbox", args);
    if (!data || !data.roadworks) {
        await rpcCall("get_roadworks", [settings.service, false]).catch(() => {});
        data = await rpcCall("get_roadworks_in_bbox", args);
    }
    return data?.roadworks || {};
}

async function queryOpendataInViewport(name, bounds) {
    const args = [name, bounds.latMin, bounds.lonMin, bounds.latMax, bounds.lonMax];
    let data = await rpcCall("get_opendata_in_bbox", args);
    if (!data || !data.opendata) {
        const cached = await rpcCall("get_opendata_cached", [name]).catch(() => null);
        if (!cached) {
            try {
                await rpcCall("get_opendata", [name, false]);
            } catch (_) {
                return null;
            }
            data = await rpcCall("get_opendata_in_bbox", args);
        }
    }
    return data?.opendata ? data : null;
}

async function refreshViewport() {
    const bounds = getViewportBounds();
    if (!bounds) {
        renderRoadworksToMap(currentRoadworks);
        renderOpendataToMap();
        updateFloatingTable();
        updateDataPanel();
        return;
    }
    try {
        currentRoadworks = await queryRoadworksInViewport(bounds);
        applyStatusOverrides();
        if (selectedRoadworkId && !currentRoadworks[selectedRoadworkId]) {
            selectedRoadworkId = null;
            hideDetailPanel();
        }
    } catch (e) {
        console.warn("[Roadwork] Failed to refresh roadworks for viewport:", e);
    }
    const services = getOpendataServices();
    for (const name of Object.keys(services)) {
        const svc = services[name];
        if (svc && svc.enabled === false) continue;
        try {
            currentOpendata[name] = await queryOpendataInViewport(name, bounds);
        } catch (e) {
            console.warn(`[Roadwork] Failed to refresh opendata ${name} for viewport:`, e);
            currentOpendata[name] = null;
        }
    }
    renderRoadworksToMap(currentRoadworks);
    renderOpendataToMap();
    updateFloatingTable();
    updateDataPanel();
    const count = Object.keys(currentRoadworks).length;
    setStatus(`${count} roadwork(s) dans la zone visible`);
}

function scheduleViewportRefresh() {
    if (viewportRefreshTimer) {
        clearTimeout(viewportRefreshTimer);
    }
    viewportRefreshTimer = setTimeout(() => {
        viewportRefreshTimer = null;
        runViewportRefresh();
    }, 400);
}

async function runViewportRefresh() {
    if (viewportRefreshInFlight) {
        viewportRefreshPending = true;
        return;
    }
    viewportRefreshInFlight = true;
    try {
        await refreshViewport();
    } finally {
        viewportRefreshInFlight = false;
        if (viewportRefreshPending) {
            viewportRefreshPending = false;
            runViewportRefresh();
        }
    }
}

async function refreshData() {
    setStatus("Loading...");
    try {
        const data = await fetchRoadworks(true);
        currentRoadworks = data.roadworks || {};
        applyStatusOverrides();
        console.info("[Roadwork] refreshData: currentRoadworks count", Object.keys(currentRoadworks).length);
        if (selectedRoadworkId && !currentRoadworks[selectedRoadworkId]) {
            selectedRoadworkId = null;
            hideDetailPanel();
        }
        const now = Date.now();
        try {
            localStorage.setItem(LAST_REFRESH_KEY, String(now));
        } catch (_) {}
        if (lastRefreshEl) {
            lastRefreshEl.textContent = new Date(now).toLocaleString("fr-FR");
        }
        await refreshViewport();
    } catch (e) {
        setStatus(e.message, "error");
    }
}

function getOpendataServices(): Record<string, any> {
    const raw = settings.opendataServices;
    if (raw && typeof raw === "object") return raw;
    return {};
}

function getOpendataDescriptorUrl(svc: any): string | undefined {
    if (!svc || typeof svc.descriptor !== "string") return undefined;
    try {
        const url = JSON.parse(svc.descriptor)?.metadata?.url;
        return typeof url === "string" && url.trim() ? url : undefined;
    } catch (_) {
        return undefined;
    }
}

async function loadOpendataCache(name: string) {
    try {
        return await rpcCall("get_opendata_cached", [name]);
    } catch (_) {
        return null;
    }
}

function loadDataSource() {
    try {
        const v = localStorage.getItem(DATA_SOURCE_KEY);
        if (v !== null) dataSource = v;
    } catch (_) {}
}

function saveDataSource() {
    try {
        localStorage.setItem(DATA_SOURCE_KEY, dataSource);
    } catch (_) {}
}

async function refreshOpendataTotals() {
    try {
        const counts = await rpcCall("get_opendata_counts");
        opendataTotals = counts && typeof counts === "object" ? counts : {};
    } catch (_) {
        opendataTotals = {};
    }
    updateDataPanel();
}

async function saveOpendataCache(name: string, data) {
    try {
        await rpcCall("store_opendata_data", [name, JSON.stringify(data)]);
    } catch (_) {}
}

async function clearOpendataCache(name: string) {
    try {
        await rpcCall("clear_opendata_cache", [name]);
    } catch (_) {}
}

async function syncOpendataDescriptorsToWasm() {
    const services = getOpendataServices();
    const pairs = Object.entries(services).map(([name, svc]) => [name, svc.descriptor]);
    await rpcCall("set_opendata_custom_descriptors", [pairs]);
}

async function fetchOpendataData(name: string, forceRefresh = false) {
    const data = await rpcCall("get_opendata", [name, forceRefresh]);
    currentOpendata[name] = data;
    return data;
}

async function refreshOpendata() {
    setStatus("Loading opendata...");
    const services = getOpendataServices();
    const enabled = Object.entries(services).filter(
        ([, svc]) => svc.enabled && getOpendataDescriptorUrl(svc),
    );
    let count = 0;
    for (const [name] of enabled) {
        try {
            const data = await fetchOpendataData(name, true);
            count += Object.keys(data.opendata || {}).length;
        } catch (e) {
            console.warn(`[Roadwork] Failed to refresh opendata ${name}:`, e);
            setStatus(`Failed to refresh opendata ${name}: ${e.message}`, "error");
        }
    }
    renderOpendataToMap();
    setStatus(`${count} opendata item(s) loaded`, "success");
    renderOpendataList();
    refreshOpendataTotals();
}

async function loadAllOpendataCaches() {
    const services = getOpendataServices();
    for (const name of Object.keys(services)) {
        const cached = await loadOpendataCache(name);
        if (cached) {
            currentOpendata[name] = cached;
        }
    }
}

const DEFAULT_OPENDATA_COLOR = "#0d9488";

function getOpendataServiceColor(name: unknown): string {
    if (typeof name !== "string" || !name) return DEFAULT_OPENDATA_COLOR;
    const svc = getOpendataServices()[name];
    if (!svc?.descriptor) return DEFAULT_OPENDATA_COLOR;
    try {
        const parsed = JSON.parse(svc.descriptor);
        const color = parsed?.metadata?.color;
        if (typeof color === "string" && /^#[0-9a-fA-F]{6}$/.test(color)) {
            return color;
        }
    } catch (_) {}
    return DEFAULT_OPENDATA_COLOR;
}

function buildStyleRulesForOpendataLayer() {
    return [
        {
            predicate: (props) => props.geomType === "Polygon",
            style: {
                fillColor: "${getFillColor}",
                fillOpacity: 0.3,
                strokeColor: "${getStrokeColor}",
                strokeOpacity: 0.8,
                strokeWidth: 2,
                title: "${opendataTitle}",
            },
        },
        {
            predicate: (props) => props.geomType === "Point",
            style: {
                icon: "${getIcon}",
                iconWidth: 20,
                iconHeight: 20,
                iconOffsetX: -10,
                iconOffsetY: -10,
                title: "${opendataTitle}",
            },
        },
    ];
}

function renderOpendataToMap() {
    try {
        wmeSDK.Map.removeAllFeaturesFromLayer({layerName: OPENDATA_LAYER});
    } catch (_) {}
    opendataFeatureIndex = {};
    updateDataPanel();
    const features = [];
    const services = getOpendataServices();
    for (const [name, svc] of Object.entries(services)) {
        if (svc.visible === false) continue;
        const data = currentOpendata[name];
        if (!data || !data.opendata) continue;
        for (const [id, od] of Object.entries(data.opendata as Record<string, any>)) {
            if (od.polygons && od.polygons.length > 0) {
                for (let pIdx = 0; pIdx < od.polygons.length; pIdx++) {
                    const polygon = od.polygons[pIdx];
                    if (!polygon.xpoints || !polygon.ypoints || polygon.xpoints.length < 3) {
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
                    const featureId = `opendata-${name}-${id}-${pIdx}`;
                    features.push({
                        id: featureId,
                        type: "Feature",
                        geometry: {
                            type: "Polygon",
                            coordinates: [coords],
                        },
                        properties: {
                            geomType: "Polygon",
                            serviceName: name,
                            opendataId: id,
                        },
                    });
                    opendataFeatureIndex[featureId] = { name: name, item: od };
                }
            } else if (od.latitude && od.longitude) {
                const featureId = `opendata-${name}-${id}`;
                features.push({
                    id: featureId,
                    type: "Feature",
                    geometry: {
                        type: "Point",
                        coordinates: [od.longitude, od.latitude],
                    },
                    properties: {
                        geomType: "Point",
                        serviceName: name,
                        opendataId: id,
                    },
                });
                opendataFeatureIndex[featureId] = { name: name, item: od };
            }
        }
    }
    if (features.length === 0) return;
    try {
        const checked = wmeSDK.LayerSwitcher.isLayerCheckboxChecked({name: OPENDATA_LAYER});
        if (!checked) return;
    } catch (_) {}
    try {
        wmeSDK.Map.addFeaturesToLayer({features: features, layerName: OPENDATA_LAYER});
    } catch (e) {
        console.warn("[Roadwork] Failed to add Opendata features:", e);
    }
}

function setOpendataServiceVisible(name: string, visible: boolean) {
    const services = getOpendataServices();
    if (!services[name]) return;
    services[name].visible = visible;
    settings.opendataServices = services;
    saveSettings();
    renderOpendataToMap();
    renderOpendataList();
}

async function refreshOpendataService(name: string) {
    const svc = getOpendataServices()[name];
    if (!getOpendataDescriptorUrl(svc) && !(await loadOpendataCache(name))) {
        setDataStatus(
            `Cannot refresh ${name}: the descriptor has no URL and no cached data. Edit the source and drop a data file to import its data.`,
            "error",
        );
        return;
    }
    setDataStatus(`Loading opendata ${name}...`);
    try {
        const data = await fetchOpendataData(name, true);
        const count = Object.keys(data.opendata || {}).length;
        setDataStatus(`${count} opendata item(s) loaded for ${name}`, "success");
        renderOpendataToMap();
        renderOpendataList();
        refreshOpendataTotals();
    } catch (e) {
        setDataStatus(`Failed to refresh opendata ${name}: ${e.message}`, "error");
    }
}

async function removeOpendataService(name: string) {
    const services = getOpendataServices();
    delete services[name];
    settings.opendataServices = services;
    saveSettings();
    await clearOpendataCache(name);
    delete currentOpendata[name];
    renderOpendataToMap();
    renderOpendataList();
    syncOpendataDescriptorsToWasm().catch(() => {});
    refreshOpendataTotals();
}

interface SaveDescriptorResult {
    ok: boolean;
    name?: string;
    error?: string;
}

async function saveOpendataDescriptorFromHelper(
    name,
    descriptor,
    oldName,
    data,
): Promise<SaveDescriptorResult> {
    if (!name || !descriptor) {
        return { ok: false, error: "Missing name or descriptor" };
    }
    try {
        const parsed = JSON.parse(descriptor);
        const svcName = parsed?.metadata?.name;
        if (svcName) name = svcName;
    } catch (_) {}
    const services = getOpendataServices();
    if (oldName && oldName !== name) {
        delete services[oldName];
    }
    const existing = services[name] || {};
    const svc: any = {
        descriptor: descriptor,
        enabled: existing.enabled ?? true,
        visible: existing.visible ?? true,
    };
    services[name] = svc;
    settings.opendataServices = services;
    saveSettings();
    setStatus(`Opendata service "${name}" saved`, "success");
    renderOpendataList();
    try {
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: "Saving descriptor\u2026",
            fraction: 0.1,
        });
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: "Syncing to extension engine\u2026",
            fraction: 0.3,
        });
        await syncOpendataDescriptorsToWasm();
        if (data) {
            try {
                currentOpendata[name] = JSON.parse(data);
                postHelperMessage({
                    type: "ROADWORK_SAVE_PROGRESS",
                    stage: "Storing imported data\u2026",
                    fraction: -1,
                });
                await saveOpendataCache(name, currentOpendata[name]);
            } catch (e) {
                const msg = `Failed to parse stored data for ${name}: ${e.message}`;
                setStatus(msg, "error");
                return { ok: false, error: msg };
            }
        } else if (getOpendataDescriptorUrl(services[name])) {
            postHelperMessage({
                type: "ROADWORK_SAVE_PROGRESS",
                stage: "Fetching remote data\u2026",
                fraction: -1,
            });
            await fetchOpendataData(name, true);
        } else {
            setStatus(
                `Opendata service "${name}" has no URL and no data - drop a data file when editing it to import its data`,
                "info",
            );
        }
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: "Updating map\u2026",
            fraction: 0.9,
        });
        renderOpendataToMap();
        refreshOpendataTotals();
    } catch (e) {
        const msg = data
            ? `Failed to parse ${name}: ${e.message}`
            : `Failed to fetch ${name}: ${e.message}`;
        setStatus(msg, "error");
        return { ok: false, error: msg };
    }
    return { ok: true, name };
}

async function renderOpendataList() {
    if (!opendataListEl) return;
    opendataListEl.replaceChildren();
    const services = getOpendataServices();
    const names = Object.keys(services).sort();
    if (names.length === 0) {
        const empty = document.createElement("div");
        empty.className = "roadwork-opendata-empty";
        empty.textContent = "No opendata service yet. Import or create one.";
        opendataListEl.appendChild(empty);
        return;
    }
    for (const name of names) {
        const svc = services[name];
        const row = document.createElement("div");
        row.className = "roadwork-opendata-row";

        const displayCheck = document.createElement("input");
        displayCheck.type = "checkbox";
        displayCheck.checked = svc.visible !== false;
        displayCheck.title = "Display on map";
        displayCheck.addEventListener("change", () => setOpendataServiceVisible(name, displayCheck.checked));
        row.appendChild(displayCheck);

        const loadBtn = document.createElement("button");
        loadBtn.className = "roadwork-btn roadwork-btn-icon";
        loadBtn.textContent = "\u21bb";
        const svcUrl = getOpendataDescriptorUrl(svc);
        loadBtn.disabled = !svcUrl;
        loadBtn.title = svcUrl
            ? "Reload data"
            : "No URL in descriptor - data must be imported by dropping a file when editing";
        loadBtn.addEventListener("click", () => refreshOpendataService(name));
        row.appendChild(loadBtn);

        const nameSpan = document.createElement("span");
        nameSpan.textContent = name;
        nameSpan.style.flex = "1";
        nameSpan.style.wordBreak = "break-all";
        row.appendChild(nameSpan);

        const exportBtn = document.createElement("button");
        exportBtn.className = "roadwork-btn roadwork-btn-icon";
        exportBtn.textContent = "\u21e1";
        exportBtn.title = "Export descriptor";
        exportBtn.addEventListener("click", () => showOpendataExport(name));
        row.appendChild(exportBtn);

        const editBtn = document.createElement("button");
        editBtn.className = "roadwork-btn roadwork-btn-icon";
        editBtn.textContent = "\u270e";
        editBtn.title = "Edit descriptor";
        editBtn.addEventListener("click", () => editOpendataService(name));
        row.appendChild(editBtn);

        const delBtn = document.createElement("button");
        delBtn.className = "roadwork-btn roadwork-btn-icon";
        delBtn.textContent = "\u00d7";
        delBtn.title = "Remove";
        delBtn.addEventListener("click", () => removeOpendataService(name));
        row.appendChild(delBtn);

        opendataListEl.appendChild(row);
    }
}

function showOpendataExport(name: string) {
    const services = getOpendataServices();
    const svc = services[name];
    if (!svc) return;
    const overlay = document.createElement("div");
    overlay.className = "rw-opendata-export-overlay";

    const box = document.createElement("div");
    box.className = "rw-opendata-export-box";

    const header = document.createElement("div");
    header.className = "rw-opendata-export-header";
    const title = document.createElement("h4");
    title.textContent = "Descriptor \u2014 " + name;
    const closeBtn = document.createElement("button");
    closeBtn.className = "roadwork-btn roadwork-btn-icon";
    closeBtn.textContent = "\u00d7";
    closeBtn.addEventListener("click", () => overlay.remove());
    header.appendChild(title);
    header.appendChild(closeBtn);
    box.appendChild(header);

    const textarea = document.createElement("textarea");
    textarea.className = "rw-opendata-export-textarea";
    textarea.value = svc.descriptor;
    textarea.readOnly = true;
    box.appendChild(textarea);

    const actions = document.createElement("div");
    actions.className = "rw-opendata-export-actions";
    const copyBtn = document.createElement("button");
    copyBtn.className = "roadwork-btn roadwork-btn-secondary";
    copyBtn.textContent = "Copy";
    copyBtn.addEventListener("click", () => {
        textarea.select();
        try {
            navigator.clipboard.writeText(svc.descriptor)
                .then(() => setStatus("Copied to clipboard", "success"))
                .catch(() => setStatus("Copy failed", "error"));
        } catch (_) {
            document.execCommand("copy");
        }
    });
    actions.appendChild(copyBtn);
    box.appendChild(actions);

    overlay.appendChild(box);
    overlay.addEventListener("click", (e) => {
        if (e.target === overlay) overlay.remove();
    });
    document.body.appendChild(overlay);
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

function centerOnOpendataItem(od) {
    if (!wmeSDK?.Map) return;
    if (od.latitude && od.longitude) {
        wmeSDK.Map.setMapCenter({lonLat: {lon: od.longitude, lat: od.latitude}});
    } else if (od.polygons && od.polygons.length > 0) {
        const polygon = od.polygons[0];
        const mid = Math.floor(polygon.xpoints.length / 2);
        if (polygon.xpoints[mid] !== undefined) {
            wmeSDK.Map.setMapCenter({lonLat: {lon: polygon.xpoints[mid], lat: polygon.ypoints[mid]}});
        }
    }
}

function updateDataPanel() {
    if (!dataTableBody) return;
    try {
        dataTableBody.replaceChildren();
        let count = 0;
        const services = getOpendataServices();
        const names = Object.keys(services).sort();
        if (dataSourceSelectEl) {
            dataSourceSelectEl.replaceChildren();
            const noneOpt = document.createElement("option");
            noneOpt.value = "";
            noneOpt.textContent = "Aucune source";
            dataSourceSelectEl.appendChild(noneOpt);
            for (const name of names) {
                const opt = document.createElement("option");
                opt.value = name;
                const total = opendataTotals[name];
                opt.textContent = total !== undefined ? `${name} (${total})` : name;
                dataSourceSelectEl.appendChild(opt);
            }
            if (!names.includes(dataSource)) {
                dataSource = "";
                saveDataSource();
            }
            dataSourceSelectEl.value = dataSource;
        }
        const filter = dataSource;
        let total = 0;
        if (filter) {
            const data = currentOpendata[filter];
            if (data && data.opendata) {
                for (const [id, od] of Object.entries(data.opendata as Record<string, any>)) {
                    count++;
                    const tr = document.createElement("tr");
                    tr.title = id;

                    const tdSource = document.createElement("td");
                    tdSource.textContent = filter;

                    const tdId = document.createElement("td");
                    tdId.textContent = id;

                    const tdDesc = document.createElement("td");
                    tdDesc.className = "rw-desc";
                    tdDesc.textContent = od.description || "";

                    const tdPos = document.createElement("td");
                    tdPos.style.fontFamily = "monospace";
                    tdPos.style.fontSize = "11px";
                    if (od.latitude && od.longitude) {
                        tdPos.textContent = `${od.latitude.toFixed(5)}, ${od.longitude.toFixed(5)}`;
                    } else if (od.polygons && od.polygons.length > 0) {
                        tdPos.textContent = "polygon";
                    } else {
                        tdPos.textContent = "-";
                    }

                    tr.appendChild(tdSource);
                    tr.appendChild(tdId);
                    tr.appendChild(tdDesc);
                    tr.appendChild(tdPos);

                    tr.addEventListener("click", () => centerOnOpendataItem(od));

                    dataTableBody.appendChild(tr);
                }
            }
            total = opendataTotals[filter] ?? count;
        }
        if (count === 0) {
            const tr = document.createElement("tr");
            const td = document.createElement("td");
            td.colSpan = 4;
            td.style.textAlign = "center";
            td.style.color = "#999";
            td.style.padding = "16px";
            td.textContent = dataSource
                ? "Aucune donn\u00e9e opendata charg\u00e9e"
                : "S\u00e9lectionnez une source";
            tr.appendChild(td);
            dataTableBody.appendChild(tr);
        }
        const label = dataSource ? `Data (${count}/${total})` : "Data";
        const titleEl = document.getElementById("rw-data-title");
        if (titleEl) titleEl.textContent = label;
        if (dataToggleBtn) dataToggleBtn.textContent = label;
        const hasSource = !!dataSource;
        if (dataUpdateBtn) dataUpdateBtn.disabled = !hasSource;
        if (dataDeleteBtn) dataDeleteBtn.disabled = !hasSource;
        if (dataEditBtn) dataEditBtn.disabled = !hasSource;
    } catch (e) {
        console.warn("[Roadwork] Failed to render data panel:", e);
    }
}

function createDataPanel() {
    dataToggleBtn = document.createElement("button");
    dataToggleBtn.className = "rw-data-toggle-btn";
    dataToggleBtn.textContent = "Data";
    dataToggleBtn.addEventListener("click", () => {
        console.info("[Roadwork] Opening data panel", { dataPanelEl: !!dataPanelEl });
        if (!dataPanelEl) return;
        dataPanelEl.classList.remove("rw-hidden");
        dataToggleBtn.style.display = "none";
        updateDataPanel();
    });
    if (toolbarEl) toolbarEl.appendChild(dataToggleBtn);

    dataPanelEl = document.createElement("div");
    dataPanelEl.className = "rw-data-panel rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-data-header";

    const title = document.createElement("h4");
    title.id = "rw-data-title";
    title.textContent = "Data";

    const headerBtns = document.createElement("div");
    headerBtns.style.cssText = "display:flex;gap:4px;";

    const refreshBtn = document.createElement("button");
    refreshBtn.textContent = "\u21bb";
    refreshBtn.title = "Refresh";
    refreshBtn.addEventListener("click", () => refreshOpendata());

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = "Fermer";
    closeBtn.addEventListener("click", () => {
        dataPanelEl.classList.add("rw-hidden");
        dataToggleBtn.style.display = "block";
    });

    headerBtns.appendChild(refreshBtn);
    headerBtns.appendChild(closeBtn);
    header.appendChild(title);
    const buildBadge = document.createElement("span");
    buildBadge.style.cssText = "font-size:10px;color:#999;margin-left:8px;align-self:center;";
    buildBadge.textContent = "build __BUILD_DATE__";
    header.appendChild(buildBadge);
    header.appendChild(headerBtns);

    const tableWrap = document.createElement("div");
    tableWrap.className = "rw-data-table-wrap";

    const table = document.createElement("table");
    table.className = "roadwork-table";
    const thead = document.createElement("thead");
    const headerRow = document.createElement("tr");
    for (const col of ["Source", "ID", "Description", "Position"]) {
        const th = document.createElement("th");
        th.textContent = col;
        headerRow.appendChild(th);
    }
    thead.appendChild(headerRow);
    table.appendChild(thead);
    dataTableBody = document.createElement("tbody");
    table.appendChild(dataTableBody);
    tableWrap.appendChild(table);

    dataPanelEl.appendChild(header);

    const controls = document.createElement("div");
    controls.className = "rw-floating-controls";

    dataSourceSelectEl = document.createElement("select");
    dataSourceSelectEl.id = "rw-data-source-select";
    dataSourceSelectEl.title = "Choisir la source de données à afficher";
    dataSourceSelectEl.addEventListener("change", () => {
        dataSource = dataSourceSelectEl.value;
        saveDataSource();
        updateDataPanel();
    });
    controls.appendChild(dataSourceSelectEl);

    dataUpdateBtn = document.createElement("button");
    dataUpdateBtn.textContent = "Refresh";
    dataUpdateBtn.title = "Recharger les données de la source sélectionnée";
    dataUpdateBtn.disabled = true;
    dataUpdateBtn.addEventListener("click", async () => {
        if (!dataSource) return;
        const original = dataUpdateBtn.textContent;
        dataUpdateBtn.textContent = "Updating...";
        try {
            await refreshOpendataService(dataSource);
        } finally {
            dataUpdateBtn.textContent = original;
        }
    });
    controls.appendChild(dataUpdateBtn);

    dataEditBtn = document.createElement("button");
    dataEditBtn.textContent = "Edit";
    dataEditBtn.title = "Ouvrir l'assistant pour éditer la source sélectionnée";
    dataEditBtn.disabled = true;
    dataEditBtn.addEventListener("click", () => {
        if (!dataSource) return;
        editOpendataService(dataSource);
    });
    controls.appendChild(dataEditBtn);

    dataDeleteBtn = document.createElement("button");
    dataDeleteBtn.textContent = "Delete";
    dataDeleteBtn.title = "Supprimer la source sélectionnée";
    dataDeleteBtn.disabled = true;
    dataDeleteBtn.addEventListener("click", () => {
        if (!dataSource) return;
        if (!confirm(`Supprimer la source \u00ab ${dataSource} \u00bb ?`)) return;
        removeOpendataService(dataSource);
        setDataStatus(`Source ${dataSource} supprim\u00e9e`, "success");
    });
    controls.appendChild(dataDeleteBtn);

    const createBtn = document.createElement("button");
    createBtn.textContent = "Create";
    createBtn.title = "Ouvrir l'assistant de création de service opendata";
    createBtn.addEventListener("click", openOpendataHelper);
    controls.appendChild(createBtn);

    dataPanelEl.appendChild(controls);

    dataStatusEl = document.createElement("div");
    dataStatusEl.className = "roadwork-status rw-hidden";
    dataStatusEl.textContent = "";
    dataPanelEl.appendChild(dataStatusEl);

    dataDropzoneEl = document.createElement("div");
    dataDropzoneEl.className = "rw-data-dropzone";
    dataDropzoneEl.textContent = "D\u00e9posez un fichier .json ici (descripteur ou donn\u00e9es)";
    dataPanelEl.appendChild(dataDropzoneEl);

    dataPanelEl.appendChild(tableWrap);
    document.body.appendChild(dataPanelEl);

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = dataPanelEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        dataPanelEl.style.left = e.clientX - dragOffsetX + "px";
        dataPanelEl.style.top = e.clientY - dragOffsetY + "px";
        dataPanelEl.style.right = "auto";
        dataPanelEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });

    updateDataPanel();
}

let toolbarEl = null;
let polygonesPanelEl: HTMLDivElement | null = null;
let polygonesToggleBtn: HTMLButtonElement | null = null;
let polygonesPanelBody: HTMLDivElement | null = null;
let polygonesDropzoneEl: HTMLDivElement | null = null;

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

function getDraggedFileExtension(e: DragEvent): string | null {
    const items = e.dataTransfer?.items;
    if (!items) return null;
    for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item.kind === "file") {
            const f = item.getAsFile();
            if (f && f.name) {
                const dot = f.name.lastIndexOf(".");
                return dot >= 0 ? f.name.substring(dot + 1).toLowerCase() : "";
            }
        }
    }
    return null;
}

function setupPolygonesDragDrop() {
    let dragCounter = 0;

    document.addEventListener("dragenter", (e) => {
        const ext = getDraggedFileExtension(e);
        if (ext !== null && ext !== "wkt" && ext !== "txt") return;
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
        if (dragCounter === 0) return;
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

function setupDataDragDrop() {
    let dragCounter = 0;

    document.addEventListener("dragenter", (e) => {
        const ext = getDraggedFileExtension(e);
        if (ext !== "json") return;
        e.preventDefault();
        dragCounter++;
        if (dragCounter === 1) {
            if (dataPanelEl) dataPanelEl.classList.remove("rw-hidden");
            if (dataToggleBtn) dataToggleBtn.style.display = "none";
            updateDataPanel();
            if (dataDropzoneEl) dataDropzoneEl.classList.add("rw-data-dropzone-active");
        }
    });

    document.addEventListener("dragover", (e) => {
        const ext = getDraggedFileExtension(e);
        if (ext !== "json") return;
        e.preventDefault();
    });

    document.addEventListener("dragleave", (e) => {
        if (dragCounter === 0) return;
        e.preventDefault();
        dragCounter--;
        if (dragCounter <= 0) {
            dragCounter = 0;
            if (dataDropzoneEl) dataDropzoneEl.classList.remove("rw-data-dropzone-active");
        }
    });

    document.addEventListener("drop", (e) => {
        const ext = getDraggedFileExtension(e);
        if (ext !== "json") return;
        e.preventDefault();
        dragCounter = 0;
        if (dataDropzoneEl) dataDropzoneEl.classList.remove("rw-data-dropzone-active");
        const files = e.dataTransfer?.files;
        if (!files || files.length === 0) return;
        const file = files[0];
        const reader = new FileReader();
        reader.onload = (evt) => {
            handleDataJsonDrop(String(evt.target.result || ""), file.name);
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

    const launchBtn = document.createElement("button");
    launchBtn.className = "roadwork-btn";
    launchBtn.textContent = "Lancer l'appli";
    launchBtn.title = "Ouvrir l'appli Roadwork";
    launchBtn.addEventListener("click", () => {
        console.log("[Roadwork] launching app overlay");
        window.postMessage({ type: "ROADWORK_OPEN_APP" }, "*");
    });
    panelEl.appendChild(launchBtn);

    const logLevelDiv = document.createElement("div");
    logLevelDiv.className = "roadwork-field";

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
    logLevelDiv.appendChild(lbl3);
    logLevelDiv.appendChild(logLevelSel);
    panelEl.appendChild(logLevelDiv);

    tabPane.appendChild(panelEl);
}

async function init() {
    await applyLogLevel(settings.logLevel);

    loadSettings();
    loadHideFinished();
    loadSortState();
    loadDataSource();
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

    try {
        wmeSDK.Map.addLayer({
            layerName: OPENDATA_LAYER,
            styleRules: buildStyleRulesForOpendataLayer(),
            styleContext: {
                opendataTitle: ({ feature }) => {
                    const props = feature?.properties || {};
                    return `${props.serviceName} — ${props.opendataId}`;
                },
                getFillColor: ({ feature }) => getOpendataServiceColor(feature?.properties?.serviceName),
                getStrokeColor: ({ feature }) => getOpendataServiceColor(feature?.properties?.serviceName),
                getIcon: ({ feature }) => buildCircleIcon(getOpendataServiceColor(feature?.properties?.serviceName)),
            },
        });
    } catch (e) {
        console.warn("[Roadwork] Failed to add Opendata layer:", e);
    }

    try {
        wmeSDK.Events.trackLayerEvents({layerName: OPENDATA_LAYER});
    } catch (e) {
        console.warn("[Roadwork] Failed to track Opendata layer events:", e);
    }

    try {
        wmeSDK.LayerSwitcher.addLayerCheckbox({
            name: OPENDATA_LAYER,
            isChecked: true,
        });
    } catch (e) {
        console.warn("[Roadwork] Failed to add Opendata layer checkbox:", e);
    }

    await syncOpendataDescriptorsToWasm().catch((e) => {
        console.warn("[Roadwork] Failed to sync opendata descriptors:", e);
    });
    await loadAllOpendataCaches();
    renderOpendataToMap();
    refreshOpendataTotals();

    const restored = await loadPolygonGroups();
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
    createDataPanel();
    setupDataDragDrop();
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
            if (evt && evt.name === OPENDATA_LAYER) {
                renderOpendataToMap();
            }
        },
    });

    wmeSDK.Events.on({
        eventName: "wme-map-move-end",
        eventHandler: () => scheduleViewportRefresh(),
    });
    wmeSDK.Events.on({
        eventName: "wme-map-zoom-changed",
        eventHandler: () => scheduleViewportRefresh(),
    });

    wmeSDK.Events.on({
        eventName: "wme-layer-feature-clicked",
        eventHandler: (evt) => {
            if (evt && evt.layerName === OPENDATA_LAYER && evt.featureId) {
                const entry = opendataFeatureIndex[evt.featureId];
                const item = entry?.item;
                if (item && wmeSDK?.Map) {
                    if (item.latitude && item.longitude) {
                        wmeSDK.Map.setMapCenter({lonLat: {lon: item.longitude, lat: item.latitude}});
                    } else if (item.polygons && item.polygons.length > 0) {
                        const polygon = item.polygons[0];
                        const mid = Math.floor(polygon.xpoints.length / 2);
                        if (polygon.xpoints[mid] !== undefined) {
                            wmeSDK.Map.setMapCenter({lonLat: {lon: polygon.xpoints[mid], lat: polygon.ypoints[mid]}});
                        }
                    }
                }
                return;
            }
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

            let rwId : string | null = null;
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

    try {
        const cached = await rpcCall("get_roadworks", [settings.service, false]);
        if (cached && cached.roadworks) {
            currentRoadworks = cached.roadworks || {};
            applyStatusOverrides();
        } else {
            await refreshData();
        }
    } catch (_) {
        await refreshData();
    }
    await refreshViewport();
    console.log("Roadwork init done");
}
