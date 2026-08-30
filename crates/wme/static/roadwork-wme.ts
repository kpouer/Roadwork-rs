import {SdkFeature} from "wme-sdk-typings";

type WmeSDK = import("wme-sdk-typings").WmeSDK;
const { t, getLocale, detectLocale } = (window as any).__rw_i18n;

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
        setStatus(t("helper.opened"), "success");
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
            postHelperMessage({ type: "ROADWORK_SAVE_DONE", name: result.name, count: result.count });
        } else {
            postHelperMessage({ type: "ROADWORK_SAVE_ERROR", error: result.error });
        }
    } else if (e.data?.type === "ROADWORK_SAVE_ROADWORK_DESCRIPTOR") {
        const result = await saveRoadworkDescriptorFromHelper(
            e.data.name,
            e.data.descriptor,
            e.data.oldName,
        );
        if (result.ok) {
            postHelperMessage({ type: "ROADWORK_SAVE_DONE", name: result.name, target: "roadwork" });
        } else {
            postHelperMessage({ type: "ROADWORK_SAVE_ERROR", error: result.error });
        }
    } else if (e.data?.type === "ROADWORK_DELETE_OPENDATA_SERVICE") {
        removeOpendataService(e.data.name);
    } else if (e.data?.type === "ROADWORK_WASM_READY") {
        if (wasmIframe?.contentWindow && e.source === wasmIframe.contentWindow) {
            wasmIframe.contentWindow.postMessage({ type: "ROADWORK_WASM_ACK" }, "*");
        }
    } else if (e.data?.type === "ROADWORK_APP_RPC") {
        const { id, method, args } = e.data;
        const source = e.source as Window | null;
        try {
            if (method === "get_viewport_bounds") {
                source?.postMessage(
                    { type: "ROADWORK_APP_RPC_RESULT", id, result: getViewportBounds() },
                    "*"
                );
                return;
            }
            const result = await rpcCall(method, args || []);
            source?.postMessage({ type: "ROADWORK_APP_RPC_RESULT", id, result }, "*");
        } catch (err) {
            source?.postMessage(
                { type: "ROADWORK_APP_RPC_ERROR", id, error: String(err) },
                "*"
            );
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
const CUSTOM_ORIGINS_KEY = "roadwork-wme-custom-origins";
const LOCAL_DESCRIPTORS_KEY = "roadwork-wme-local-descriptors";
const KNOWN_MODIFIED_KEY = "roadwork-wme-known-modified";
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
};

const OPENDATA_LAYER = "Opendata";
const OPENDATA_TABLE_MAX = 100;
const PAGE_SIZE_OPTIONS = [20, 50, 100, 200, 500];
const ROADWORK_PAGE_SIZE_KEY = "roadwork-wme-rw-page-size";
const ROADWORK_VISIBLE_KEY = "roadwork-wme-rw-visible";
const DATA_PAGE_SIZE_KEY = "roadwork-wme-data-page-size";
const DATA_VISIBLE_KEY = "roadwork-wme-data-visible";

interface PaginationState {
    page: number;
    pageSize: number;
    onlyVisible: boolean;
    allItems: Record<string, any>;
    pageLabel: HTMLSpanElement | null;
    prevBtn: HTMLButtonElement | null;
    nextBtn: HTMLButtonElement | null;
    pageSizeKey: string;
    visibleKey: string;
}

function createPaginationState(pageSizeKey: string, visibleKey: string): PaginationState {
    return { page: 0, pageSize: 100, onlyVisible: false, allItems: {}, pageLabel: null, prevBtn: null, nextBtn: null, pageSizeKey, visibleKey };
}

const roadworksPagination = createPaginationState(ROADWORK_PAGE_SIZE_KEY, ROADWORK_VISIBLE_KEY);
const dataPagination = createPaginationState(DATA_PAGE_SIZE_KEY, DATA_VISIBLE_KEY);

let wmeSDK: WmeSDK | null = null;
let settings = {...DEFAULTS};
let opendataServices: Record<string, any> = {};
let currentRoadworks: any = {};
let currentOpendata: Record<string, any> = {};
let opendataTotals: Record<string, number> = {};
let dataSource: string = "";
let opendataFeatureIndex: Record<string, any> = {};
let servicesData = [];
let panelEl: HTMLDivElement | null = null;
let deleteRoadworkBtnEl: HTMLButtonElement | null = null;
let statusEl: HTMLDivElement | null = null;
let lastRefreshEl: HTMLSpanElement | null = null;
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

function loadPaginationState(p: PaginationState) {
    try {
        const v = localStorage.getItem(p.pageSizeKey);
        if (v !== null) p.pageSize = JSON.parse(v);
    } catch (_) {}
    try {
        const v = localStorage.getItem(p.visibleKey);
        if (v !== null) p.onlyVisible = JSON.parse(v);
    } catch (_) {}
}

function loadPaginationSettings() {
    loadPaginationState(roadworksPagination);
    loadPaginationState(dataPagination);
}

function paginate<T>(p: PaginationState, entries: T[]): T[] {
    const totalPages = Math.max(1, Math.ceil(entries.length / p.pageSize));
    if (p.page >= totalPages) p.page = totalPages - 1;
    if (p.page < 0) p.page = 0;
    const start = p.page * p.pageSize;
    return entries.slice(start, start + p.pageSize);
}

function updatePaginationUI(p: PaginationState, totalCount: number) {
    const totalPages = Math.max(1, Math.ceil(totalCount / p.pageSize));
    if (p.pageLabel) p.pageLabel.textContent = t("pagination.page", { current: String(p.page + 1), total: String(totalPages) });
    if (p.prevBtn) p.prevBtn.disabled = p.page <= 0;
    if (p.nextBtn) p.nextBtn.disabled = p.page >= totalPages - 1;
}

function createPaginationRow(p: PaginationState, onUpdate: () => void, extraBeforeVisible?: HTMLElement): HTMLDivElement {
    const paginationRow = document.createElement("div");
    paginationRow.className = "rw-pagination";

    if (extraBeforeVisible) paginationRow.appendChild(extraBeforeVisible);

    const visibleLabel = document.createElement("label");
    visibleLabel.className = "rw-visible-label";
    const visibleCheck = document.createElement("input");
    visibleCheck.type = "checkbox";
    visibleCheck.checked = p.onlyVisible;
    visibleCheck.addEventListener("change", () => {
        p.onlyVisible = visibleCheck.checked;
        localStorage.setItem(p.visibleKey, JSON.stringify(p.onlyVisible));
        p.page = 0;
        onUpdate();
    });
    const visibleText = document.createElement("span");
    visibleText.textContent = t("pagination.visible");
    visibleLabel.appendChild(visibleCheck);
    visibleLabel.appendChild(visibleText);
    paginationRow.appendChild(visibleLabel);

    const pageSizeSelect = document.createElement("select");
    for (const size of PAGE_SIZE_OPTIONS) {
        const opt = document.createElement("option");
        opt.value = String(size);
        opt.textContent = String(size);
        pageSizeSelect.appendChild(opt);
    }
    pageSizeSelect.value = String(p.pageSize);
    pageSizeSelect.addEventListener("change", () => {
        p.pageSize = parseInt(pageSizeSelect.value, 10);
        localStorage.setItem(p.pageSizeKey, String(p.pageSize));
        p.page = 0;
        onUpdate();
    });
    paginationRow.appendChild(pageSizeSelect);

    p.prevBtn = document.createElement("button");
    p.prevBtn.textContent = "\u25c0";
    p.prevBtn.title = t("pagination.prev");
    p.prevBtn.addEventListener("click", () => {
        if (p.page > 0) { p.page--; onUpdate(); }
    });
    paginationRow.appendChild(p.prevBtn);

    p.pageLabel = document.createElement("span");
    p.pageLabel.className = "rw-page-label";
    paginationRow.appendChild(p.pageLabel);

    p.nextBtn = document.createElement("button");
    p.nextBtn.textContent = "\u25b6";
    p.nextBtn.title = t("pagination.next");
    p.nextBtn.addEventListener("click", () => {
        p.page++;
        onUpdate();
    });
    paginationRow.appendChild(p.nextBtn);

    return paginationRow;
}

function changeRoadworkStatus(rwId: string, newStatus: string) {
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
        const knownModified = loadKnownModified();
        const result = await rpcCall("sync_index", [knownModified]);
        if (result && Array.isArray(result.services)) {
            if (result.known_modified && typeof result.known_modified === "object") {
                saveKnownModified(result.known_modified);
            }
            saveServicesCache(result.services);
            return result.services;
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

function loadLocalDescriptors(): Record<string, any> {
    try {
        const raw = localStorage.getItem(LOCAL_DESCRIPTORS_KEY);
        if (!raw) return {};
        const parsed = JSON.parse(raw);
        return parsed && typeof parsed === "object" ? parsed : {};
    } catch (_) {
        return {};
    }
}

function saveLocalDescriptors(descriptors: Record<string, any>) {
    try {
        localStorage.setItem(LOCAL_DESCRIPTORS_KEY, JSON.stringify(descriptors));
    } catch (_) {}
}

function loadKnownModified(): Record<string, string> {
    try {
        const raw = localStorage.getItem(KNOWN_MODIFIED_KEY);
        if (!raw) return {};
        const parsed = JSON.parse(raw);
        return parsed && typeof parsed === "object" ? parsed : {};
    } catch (_) {
        return {};
    }
}

function saveKnownModified(known: Record<string, string>) {
    try {
        localStorage.setItem(KNOWN_MODIFIED_KEY, JSON.stringify(known));
    } catch (_) {}
}

async function fetchCustomDescriptors(): Promise<{ pairs: Array<any>; origins: Record<string, string> }> {
    const sources = Array.isArray(settings.customSources) ? settings.customSources : [];
    const pairs = [];
    const origins: Record<string, string> = {};
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
                origins[name] = url;
            } catch (e) {
                console.warn(`[Roadwork] Failed to fetch descriptor ${descUrl}: ${e}`);
            }
        }
    }
    return { pairs, origins };
}

function loadCustomOriginsCache(): Record<string, string> | null {
    try {
        const raw = localStorage.getItem(CUSTOM_ORIGINS_KEY);
        if (!raw) return null;
        const parsed = JSON.parse(raw);
        return parsed && typeof parsed === "object" ? parsed : null;
    } catch (_) {
        return null;
    }
}

function saveCustomOriginsCache(origins: Record<string, string>) {
    try {
        localStorage.setItem(CUSTOM_ORIGINS_KEY, JSON.stringify(origins));
    } catch (_) {}
}

async function syncCustomDescriptorsToWasm(forceRefresh = false) {
    const sources = Array.isArray(settings.customSources) ? settings.customSources : [];
    const localPairs: Array<any> = Object.entries(loadLocalDescriptors());
    if (sources.length === 0) {
        await rpcCall("set_custom_descriptors", [localPairs]);
        try {
            localStorage.removeItem(CUSTOM_SOURCES_CACHE_KEY);
        } catch (_) {}
        try {
            localStorage.removeItem(CUSTOM_ORIGINS_KEY);
        } catch (_) {}
        if (localPairs.length > 0) {
            try {
                localStorage.removeItem(SERVICES_CACHE_KEY);
            } catch (_) {}
        }
        return;
    }
    let pairs = null;
    let customOrigins: Record<string, string> | null = null;
    if (!forceRefresh) {
        pairs = loadCustomDescriptorsCache();
        customOrigins = loadCustomOriginsCache();
    }
    if (pairs === null) {
        const fetched = await fetchCustomDescriptors();
        pairs = fetched.pairs;
        customOrigins = fetched.origins;
        saveCustomDescriptorsCache(pairs);
        saveCustomOriginsCache(customOrigins);
    }
    pairs = pairs.concat(localPairs);
    await rpcCall("set_custom_descriptors", [pairs]);
    try {
        localStorage.removeItem(SERVICES_CACHE_KEY);
    } catch (_) {}
}

async function pruneStaleDescriptors() {
    const oldKnown = loadKnownModified();
    if (Object.keys(oldKnown).length === 0) return;
    await fetchServices(true).catch(() => []);
    const freshKnown = loadKnownModified();
    const stale = Object.keys(oldKnown).filter((name) => !(name in freshKnown));
    if (stale.length === 0) return;

    let cleanedCustom = false;
    for (const name of stale) {
        console.warn("[Roadwork] Suppression du descriptor obsolète: " + name);
        const local = loadLocalDescriptors();
        if (Object.prototype.hasOwnProperty.call(local, name)) {
            delete local[name];
            saveLocalDescriptors(local);
            cleanedCustom = true;
        }
        const pairs = loadCustomDescriptorsCache();
        if (pairs) {
            const filtered = pairs.filter((p) => p[0] !== name);
            if (filtered.length !== pairs.length) {
                saveCustomDescriptorsCache(filtered);
                cleanedCustom = true;
            }
        }
        const origins = loadCustomOriginsCache();
        if (origins && Object.prototype.hasOwnProperty.call(origins, name)) {
            delete origins[name];
            saveCustomOriginsCache(origins);
        }
        await rpcCall("clear_roadworks_cache", [name]).catch(() => {});
    }
    if (cleanedCustom) {
        await syncCustomDescriptorsToWasm(false).catch(() => {});
        try {
            localStorage.removeItem(SERVICES_CACHE_KEY);
        } catch (_) {}
    }
}

function setStatus(text: string, type?) {
    if (!statusEl) {
        return;
    }
    statusEl.textContent = text;
    statusEl.className = "roadwork-status" + (type ? " " + type : "");
    updateFloatingCount();
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
            lastRefreshEl.textContent = new Date(parseInt(stored, 10)).toLocaleString(getLocale());
        }
    } catch (_) {
    }
}

const LAST_REFRESH_KEY = "roadwork-wme-last-refresh";
const PANEL_STORAGE_KEY = "roadwork-wme-panel-visible";
const HIDE_FINISHED_KEY = "roadwork-wme-hide-finished";
const PANEL_SIZE_KEY = "roadwork-wme-panel-size";
const SORT_STATE_KEY = "roadwork-wme-sort-state";
const TOOLBAR_POSITION_KEY = "roadwork-wme-toolbar-position";
const TOOLBAR_COLLAPSED_KEY = "roadwork-wme-toolbar-collapsed";

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

function openRoadworkHelper() {
    openHelper("roadwork", true);
}

function editRoadworkService() {
    const name = serviceSelectEl?.value || settings.service;
    const local = loadLocalDescriptors()[name];
    if (local !== undefined) {
        openHelper("roadwork", false, name, local);
        return;
    }
    const pairs = loadCustomDescriptorsCache();
    const remote = Array.isArray(pairs) ? pairs.find((p) => p[0] === name) : null;
    if (remote && remote[1]) {
        openHelper("roadwork", false, name, remote[1]);
        return;
    }
    openHelper("builtin", false, name);
}

function editOpendataService(name: string) {
    const services = getOpendataServices();
    const svc = services[name];
    if (!svc || !svc.descriptor) {
        setStatus(t("status.no_descriptor", { name }), "error");
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
    title.textContent = t("import.title");
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
    intro.textContent = t("import.intro");
    body.appendChild(intro);

    const createBtn = document.createElement("button");
    createBtn.className = "rw-import-choice-create";
    createBtn.textContent = t("import.create_new");
    createBtn.addEventListener("click", () => {
        overlay.remove();
        openHelper("opendata", true, baseName, undefined, text);
    });
    body.appendChild(createBtn);

    if (names.length > 0) {
        const label = document.createElement("div");
        label.className = "rw-import-choice-label";
        label.textContent = t("import.update_existing");
        body.appendChild(label);
        for (const name of names) {
            const btn = document.createElement("button");
            btn.className = "rw-import-choice-source";
            btn.textContent = name;
            btn.title = t("import.update_tooltip", { name });
            btn.addEventListener("click", () => {
                overlay.remove();
                openHelper("opendata", false, name, services[name]?.descriptor, text);
            });
            body.appendChild(btn);
        }
    } else {
        const empty = document.createElement("div");
        empty.className = "roadwork-opendata-empty";
        empty.textContent = t("import.no_existing");
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
    setStatus(t("helper.opening"), "info");
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
            setStatus(t("helper.no_response"), "error");
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
    title.textContent = t("panel.roadworks");

    const refreshBtn = document.createElement("button");
    refreshBtn.textContent = t("btn.refresh");
    refreshBtn.title = t("btn.refresh");
    refreshBtn.addEventListener("click", () => refreshData());

    const closeBtn = document.createElement("button");
    closeBtn.className = "rw-floating-close";
    closeBtn.textContent = "\u00d7";
    closeBtn.title = t("btn.hide");
    closeBtn.addEventListener("click", () => setFloatingPanelVisible(false));

    header.appendChild(title);
    header.appendChild(closeBtn);

    const tableWrap = document.createElement("div");
    tableWrap.className = "roadwork-table-wrap";

    const table = document.createElement("table");
    table.className = "roadwork-table";
    const thead = document.createElement("thead");
    const headerRow = document.createElement("tr");
    const COLUMNS = [t("table.status"), t("table.road"), t("table.start"), t("table.end"), t("table.description"), t("table.impact")];
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
    serviceSelectEl.title = t("opendata.service_select");
    controls.appendChild(serviceSelectEl);

    const centerBtn = document.createElement("button");
    centerBtn.textContent = t("btn.center");
    centerBtn.title = t("opendata.center_title");
    centerBtn.addEventListener("click", () => {
        const svcInfo = servicesData.find((s) => s.name === serviceSelectEl.value);
        if (svcInfo?.center && wmeSDK?.Map?.setMapCenter) {
            wmeSDK.Map.setMapCenter({
                lonLat: { lon: svcInfo.center.lon, lat: svcInfo.center.lat },
                zoomLevel: 12,
            });
        }
    });
    controls.appendChild(centerBtn);

    controls.appendChild(refreshBtn);

    const createBtn = document.createElement("button");
    createBtn.textContent = t("btn.create");
    createBtn.title = t("btn.create_opendata_title");
    createBtn.addEventListener("click", openRoadworkHelper);
    controls.appendChild(createBtn);

    const editBtn = document.createElement("button");
    editBtn.textContent = t("btn.edit");
    editBtn.title = t("btn.edit_roadwork_title");
    editBtn.addEventListener("click", () => {
        editRoadworkService();
    });
    controls.appendChild(editBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.textContent = t("btn.delete");
    deleteBtn.title = t("btn.delete_roadwork_title");
    deleteBtn.addEventListener("click", () => {
        void deleteSelectedRoadworkService();
    });
    controls.appendChild(deleteBtn);
    deleteRoadworkBtnEl = deleteBtn;

    const statusDiv = document.createElement("div");
    statusDiv.id = "rw-status";
    statusDiv.className = "roadwork-status rw-floating-status";
    statusEl = statusDiv;

    const lastRefreshSpan = document.createElement("span");
    lastRefreshSpan.style.cssText = "font-size:11px;color:#999;margin-left:8px;";
    lastRefreshEl = lastRefreshSpan;
    statusDiv.appendChild(lastRefreshSpan);

    floatingPanelEl.appendChild(header);
    floatingPanelEl.appendChild(controls);
    floatingPanelEl.appendChild(statusDiv);

    const filterLabel = document.createElement("label");
    filterLabel.className = "rw-visible-label";
    filterLabel.title = t("pagination.hide_finished_title");
    const filterCheck = document.createElement("input");
    filterCheck.type = "checkbox";
    filterCheck.checked = hideFinished;
    filterCheck.addEventListener("change", () => {
        hideFinished = filterCheck.checked;
        localStorage.setItem(HIDE_FINISHED_KEY, JSON.stringify(hideFinished));
        updateFloatingTable();
    });
    const filterText = document.createElement("span");
    filterText.textContent = t("pagination.hide_finished");
    filterLabel.appendChild(filterCheck);
    filterLabel.appendChild(filterText);

    floatingPanelEl.appendChild(createPaginationRow(roadworksPagination, updateFloatingTable, filterLabel));
    floatingPanelEl.appendChild(tableWrap);

    const resizeHandle = document.createElement("div");
    resizeHandle.className = "rw-resize-handle";
    floatingPanelEl.appendChild(resizeHandle);

    document.body.appendChild(floatingPanelEl);
    updateLastRefreshDisplay();

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

    serviceSelectEl.addEventListener("change", () => {
        updateDeleteRoadworkBtnState();
        void switchToService(serviceSelectEl.value);
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
    floatingToggleBtn.textContent = t("panel.roadworks");
    floatingToggleBtn.addEventListener("click", () => setFloatingPanelVisible(true));
    if (isFloatingPanelVisible()) {
        floatingToggleBtn.style.display = "none";
    }
    if (toolbarEl) {
        toolbarEl.appendChild(floatingToggleBtn);
        syncToolbarButtonVisibility(floatingToggleBtn);
    }

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
    const count = Object.keys(currentRoadworks).length;
    const statusText = statusEl?.textContent || "";
    if (statusText) {
        floatingTitleEl.textContent = t("panel.roadworks_count", { count: String(count) }) + ` — ${statusText}`;
    } else {
        floatingTitleEl.textContent = t("panel.roadworks_count", { count: String(count) });
    }
}

function updateFloatingTable() {
    if (!floatingTableBody) return;
    updateFloatingCount();
    floatingTableBody.replaceChildren();
    const source = roadworksPagination.onlyVisible ? currentRoadworks : roadworksPagination.allItems;
    let entries = Object.entries(source as Record<string, any>);

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

    const pageEntries = paginate(roadworksPagination, entries);
    updatePaginationUI(roadworksPagination, entries.length);

    if (entries.length === 0) {
        const tr = document.createElement("tr");
        const td = document.createElement("td");
        td.colSpan = 7;
        td.style.textAlign = "center";
        td.style.color = "#999";
        td.style.padding = "16px";
        td.textContent = entries.length === 0 && roadworksPagination.onlyVisible
            ? t("empty.no_roadwork_visible")
            : t("empty.no_roadwork");
        tr.appendChild(td);
        floatingTableBody.appendChild(tr);
        return;
    }

    for (const [id, rw] of pageEntries) {
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

function buildCircleIcon(color: string, size = 20) {
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
        return new Date(millis).toLocaleDateString(getLocale(), {
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

function parseWkt(str: string) {
    str = str.replace(/^(?:--|#).*/gm, '').trim();
    if (!str) return [];

    const features: SdkFeature[] = [];
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
                    features.push({
                        id: `wkt-${idCounter++}`,
                        type: 'Feature',
                        geometry: { type: 'Point', coordinates: [x, y] },
                        properties: { geomType: 'Point' }
                    } as SdkFeature);
                }
                break;
            }
            case 'LINESTRING': {
                const coords = parseCoordList(inner);
                if (coords.length >= 2) {
                    features.push({
                        id: `wkt-${idCounter++}`,
                        type: 'Feature',
                        geometry: { type: 'LineString', coordinates: coords },
                        properties: { geomType: 'LineString' }
                    } as SdkFeature);
                }
                break;
            }
            case 'POLYGON': {
                const rings = parseRings(inner);
                if (rings.length > 0 && rings[0].length >= 3) {
                    features.push({
                        id: `wkt-${idCounter++}`,
                        type: 'Feature',
                        geometry: { type: 'Polygon', coordinates: rings },
                        properties: { geomType: 'Polygon' }
                    } as SdkFeature);
                }
                break;
            }
            case 'MULTIPOINT': {
                const coords = inner.trim().startsWith('(')
                    ? splitTopLevelParens(inner).map(c => { const [x, y] = c.trim().split(/\s+/).map(Number); return [x, y]; })
                    : parseCoordList(inner);
                for (const [x, y] of coords) {
                    if (isFinite(x) && isFinite(y)) {
                        features.push({
                            id: `wkt-${idCounter++}`,
                            type: 'Feature',
                            geometry: { type: 'Point', coordinates: [x, y] },
                            properties: { geomType: 'Point' }
                        } as SdkFeature);
                    }
                }
                break;
            }
            case 'MULTILINESTRING': {
                const groups = splitTopLevelParens(inner);
                for (const g of groups) {
                    const coords = parseCoordList(g);
                    if (coords.length >= 2) {
                        features.push({
                            id: `wkt-${idCounter++}`,
                            type: 'Feature',
                            geometry: { type: 'LineString', coordinates: coords },
                            properties: { geomType: 'LineString' }
                        } as SdkFeature);
                    }
                }
                break;
            }
            case 'MULTIPOLYGON': {
                const groups = splitTopLevelParens(inner);
                for (const g of groups) {
                    const rings = parseRings(g);
                    if (rings.length > 0 && rings[0].length >= 3) {
                        features.push({
                            id: `wkt-${idCounter++}`,
                            type: 'Feature',
                            geometry: { type: 'Polygon', coordinates: rings },
                            properties: { geomType: 'Polygon' }
                        } as SdkFeature);
                    }
                }
                break;
            }
        }
    }

    return features;
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
    header.style.cursor = "move";

    const title = document.createElement("h4");
    title.textContent = t("detail.title");

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

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = detailPanelEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
        e.stopPropagation();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        const x = e.clientX - dragOffsetX;
        const y = e.clientY - dragOffsetY;
        detailPanelEl.style.left = x + "px";
        detailPanelEl.style.top = y + "px";
        detailPanelEl.style.right = "auto";
        detailPanelEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });
}

function showDetailPanel(rw) {
    if (!detailPanelEl) return;
    const body = detailPanelEl.querySelector(".rw-detail-body");
    if (!body) return;

    const status = rw.sync_data?.status || "New";
    const color: string = STATUS_COLORS[status] || "#9ca3af";
    const road: string = rw.road || "";
    const start = formatTimestamp(rw.start);
    const end = formatTimestamp(rw.end);
    const desc: string = rw.opendata?.description || "";
    const impact: string = rw.impact_circulation_detail || "";

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
        addField(t("detail.status"), dropdown);
    }

    if (road) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = road;
        addField(t("detail.road"), val);
    }

    {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = `${start} — ${end}`;
        addField(t("detail.period"), val);
    }

    if (rw.opendata?.latitude && rw.opendata?.longitude) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = `${rw.opendata?.latitude.toFixed(6)}, ${rw.opendata?.longitude.toFixed(6)}`;
        addField(t("detail.coordinates"), val);
    }

    if (desc) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.textContent = desc;
        addField(t("detail.description"), val);
    }

    if (impact) {
        const val = document.createElement("span");
        val.className = "rw-detail-value";
        val.style.color = "#b45309";
        val.textContent = impact;
        addField(t("detail.impact"), val);
    }

    const wasHidden = detailPanelEl.classList.contains("rw-hidden");
    detailOverlayEl.classList.remove("rw-hidden");
    detailPanelEl.classList.remove("rw-hidden");
    if (wasHidden) {
        detailPanelEl.style.left = "";
        detailPanelEl.style.top = "";
        detailPanelEl.style.right = "";
        detailPanelEl.style.bottom = "";
        const rect = detailPanelEl.getBoundingClientRect();
        detailPanelEl.style.left = ((window.innerWidth - rect.width) / 2) + "px";
        detailPanelEl.style.top = ((window.innerHeight - rect.height) / 2) + "px";
    }
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
    if (statusEl) statusEl.textContent = t("empty.no_files");
    updatePolygonesPanel();
}

async function clearExtensionStorage() {
    if (!confirm(t("confirm.clear_storage"))) return;
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

async function queryOpendataInViewport(name: string, bounds) {
    const args = [name, bounds.latMin, bounds.lonMin, bounds.latMax, bounds.lonMax, null];
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
        const bboxRoadworks = await queryRoadworksInViewport(bounds);
        if (Object.keys(bboxRoadworks).length > 0 || Object.keys(currentRoadworks).length === 0) {
            currentRoadworks = bboxRoadworks;
        }
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
    setStatus(t("status.loading"));
    try {
        const data = await fetchRoadworks(true);
        roadworksPagination.allItems = data.roadworks || {};
        currentRoadworks = roadworksPagination.allItems;
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
            lastRefreshEl.textContent = new Date(now).toLocaleString(getLocale());
        }
        await refreshViewport();
    } catch (e) {
        setStatus(e.message, "error");
    }
}

function getOpendataServices(): Record<string, any> {
    return opendataServices;
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

/// Loads the custom opendata sources from the wasm store into the in-memory
/// map. As a one-shot migration, sources still stored in the legacy settings
/// blob (`settings.opendataServices`) are first pushed to the store.
async function loadOpendataServices() {
    const legacy = (settings as Record<string, any>).opendataServices;
    if (legacy && typeof legacy === "object" && Object.keys(legacy).length > 0) {
        for (const [name, svc] of Object.entries(legacy) as [string, any][]) {
            if (!svc || typeof svc.descriptor !== "string") continue;
            try {
                await rpcCall("save_opendata_source", [
                    name,
                    svc.descriptor,
                    svc.enabled !== false,
                    svc.visible !== false,
                    undefined,
                ]);
            } catch (e) {
                console.warn(`[Roadwork] Failed to migrate opendata source ${name}:`, e);
            }
        }
        delete (settings as Record<string, any>).opendataServices;
        saveSettings();
    }
    try {
        const sources = await rpcCall("get_opendata_sources");
        const next: Record<string, any> = {};
        if (sources && Array.isArray(sources)) {
            for (const src of sources) {
                if (!src || typeof src.service !== "string" || typeof src.descriptor !== "string") {
                    continue;
                }
                next[src.service] = {
                    descriptor: src.descriptor,
                    enabled: src.enabled !== false,
                    visible: src.visible !== false,
                };
            }
        }
        opendataServices = next;
    } catch (e) {
        console.warn("[Roadwork] Failed to load opendata services:", e);
        opendataServices = {};
    }
}

async function fetchOpendataData(name: string, forceRefresh = false) {
    const data = await rpcCall("get_opendata", [name, forceRefresh]);
    dataPagination.allItems[name] = data;
    currentOpendata[name] = data;
    return data;
}

async function refreshOpendata() {
    setStatus(t("status.loading_opendata"));
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
            setStatus(t("status.refresh_failed", { name, error: e.message }), "error");
        }
    }
    renderOpendataToMap();
    setStatus(t("status.loaded_opendata", { count: String(count) }), "success");
    refreshOpendataTotals();
}

async function loadAllOpendataCaches() {
    const services = getOpendataServices();
    for (const name of Object.keys(services)) {
        const cached = await loadOpendataCache(name);
        if (cached) {
            dataPagination.allItems[name] = cached;
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
        let rendered = 0;
        for (const [id, od] of Object.entries(data.opendata as Record<string, any>)) {
            if (rendered >= OPENDATA_TABLE_MAX) break;
            rendered++;
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
    rpcCall("set_opendata_source_flags", [name, services[name].enabled !== false, visible]).catch((e) => {
        console.warn(`[Roadwork] Failed to update opendata flags for ${name}:`, e);
    });
    renderOpendataToMap();
}

async function refreshOpendataService(name: string) {
    const svc = getOpendataServices()[name];
    if (!getOpendataDescriptorUrl(svc) && !(await loadOpendataCache(name))) {
        setDataStatus(
            t("status.no_url_no_cache", { name }),
            "error",
        );
        return;
    }
    setDataStatus(t("status.loading_opendata_svc", { name }));
    try {
        const data = await fetchOpendataData(name, true);
        const count = Object.keys(data.opendata || {}).length;
        setDataStatus(t("status.loaded_opendata_svc", { count: String(count), name }), "success");
        renderOpendataToMap();
        refreshOpendataTotals();
    } catch (e) {
        setDataStatus(t("status.refresh_failed", { name, error: e.message }), "error");
    }
}

async function removeOpendataService(name: string) {
    const services = getOpendataServices();
    delete services[name];
    await clearOpendataCache(name);
    delete currentOpendata[name];
    delete dataPagination.allItems[name];
    renderOpendataToMap();
    refreshOpendataTotals();
}

interface SaveDescriptorResult {
    ok: boolean;
    name?: string;
    error?: string;
    count?: number;
}

async function saveOpendataDescriptorFromHelper(
    name: string,
    descriptor,
    oldName: string,
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
    const existing = (oldName && oldName !== name ? services[oldName] : services[name]) || {};
    const svc: any = {
        descriptor: descriptor,
        enabled: existing.enabled ?? true,
        visible: existing.visible ?? true,
    };
    if (oldName && oldName !== name) {
        delete services[oldName];
    }
    services[name] = svc;
    setStatus(t("status.svc_saved", { name }), "success");
    try {
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.saving_descriptor"),
            fraction: 0.1,
        });
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.syncing_engine"),
            fraction: 0.3,
        });
        await rpcCall("save_opendata_source", [
            name,
            descriptor,
            svc.enabled,
            svc.visible,
            oldName || undefined,
        ]);
        if (data) {
            try {
                const parsed = JSON.parse(data);
                dataPagination.allItems[name] = parsed;
                currentOpendata[name] = parsed;
                postHelperMessage({
                    type: "ROADWORK_SAVE_PROGRESS",
                    stage: t("progress.storing_data"),
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
                stage: t("progress.fetching_remote"),
                fraction: -1,
            });
            await fetchOpendataData(name, true);
        } else {
            setStatus(
                t("status.svc_no_url", { name }),
                "info",
            );
        }
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.updating_map"),
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
    const count = Object.keys(currentOpendata[name]?.opendata ?? {}).length;
    return { ok: true, name, count };
}

async function saveRoadworkDescriptorFromHelper(
    name,
    descriptor,
    oldName,
): Promise<SaveDescriptorResult> {
    if (!name || !descriptor) {
        return { ok: false, error: "Missing name or descriptor" };
    }
    try {
        const parsed = JSON.parse(descriptor);
        const svcName = parsed?.metadata?.name;
        if (svcName) name = svcName;
    } catch (_) {}
    const locals = loadLocalDescriptors();
    if (oldName && oldName !== name && locals[oldName] !== undefined) {
        delete locals[oldName];
    }
    locals[name] = descriptor;
    setStatus(t("status.svc_saved", { name }), "success");
    try {
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.saving_descriptor"),
            fraction: 0.1,
        });
        saveLocalDescriptors(locals);
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.syncing_engine"),
            fraction: 0.3,
        });
        await syncCustomDescriptorsToWasm(false);
        const services = await fetchServices(true);
        servicesData = services;
        postHelperMessage({
            type: "ROADWORK_SAVE_PROGRESS",
            stage: t("progress.updating_map"),
            fraction: 0.9,
        });
        try {
            await switchToService(name);
        } catch (e) {
            console.warn("[Roadwork] Failed to select the new service", e);
        }
        if (serviceSelectEl) {
            populateServiceSelect(serviceSelectEl, services);
            serviceSelectEl.value = name;
            updateDeleteRoadworkBtnState();
        }
    } catch (e) {
        const msg = e.message ? `${e.message}` : String(e);
        setStatus(msg, "error");
        return { ok: false, error: msg };
    }
    return { ok: true, name };
}

async function deleteSelectedRoadworkService() {
    const name = serviceSelectEl?.value || settings.service;
    const locals = loadLocalDescriptors();
    if (locals[name] === undefined) {
        setStatus(t("status.not_deletable"), "info");
        return;
    }
    if (!confirm(t("confirm.delete_source", { name }))) return;
    delete locals[name];
    saveLocalDescriptors(locals);
    try {
        await rpcCall("clear_roadworks_cache", [name]);
    } catch (_) {}
    await syncCustomDescriptorsToWasm(false);
    setStatus(t("status.svc_deleted", { name }), "success");
    const services = await fetchServices(true);
    servicesData = services;
    if (settings.service === name) {
        currentRoadworks = {};
        roadworksPagination.allItems = {};
        selectedRoadworkId = null;
        hideDetailPanel();
        clearMapFeatures();
        roadworksPagination.page = 0;
        updateFloatingTable();
        const fallback = services.find((s) => s.name !== name);
        if (fallback && serviceSelectEl) {
            serviceSelectEl.value = fallback.name;
            await switchToService(fallback.name);
        }
    }
    if (serviceSelectEl) {
        populateServiceSelect(serviceSelectEl, services);
        if (!services.some((s) => s.name === settings.service) && services.length > 0) {
            serviceSelectEl.value = services[0].name;
        }
        updateDeleteRoadworkBtnState();
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
    title.textContent = t("opendata.title", { name });
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
    copyBtn.textContent = t("btn.copy");
    copyBtn.addEventListener("click", () => {
        textarea.select();
        try {
            navigator.clipboard.writeText(svc.descriptor)
                .then(() => setStatus(t("status.copied"), "success"))
                .catch(() => setStatus(t("status.copy_failed"), "error"));
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
    updateDeleteRoadworkBtnState();
}

function updateDeleteRoadworkBtnState() {
    if (!deleteRoadworkBtnEl) return;
    const name = serviceSelectEl?.value || settings.service;
    const deletable = loadLocalDescriptors()[name] !== undefined;
    deleteRoadworkBtnEl.disabled = !deletable;
    deleteRoadworkBtnEl.title = deletable
        ? t("btn.delete_roadwork_title")
        : t("status.not_deletable");
}

async function switchToService(newService: string) {
    if (!newService || newService === settings.service) return;

    settings.service = newService;
    saveSettings();

    currentRoadworks = {};
    roadworksPagination.allItems = {};
    selectedRoadworkId = null;
    hideDetailPanel();
    clearMapFeatures();
    roadworksPagination.page = 0;
    updateFloatingTable();

    setStatus(t("status.loading"));
    try {
        const data = await fetchRoadworks(true);
        roadworksPagination.allItems = data.roadworks || {};
        currentRoadworks = roadworksPagination.allItems;
        applyStatusOverrides();
        const now = Date.now();
        try {
            localStorage.setItem(LAST_REFRESH_KEY, String(now));
        } catch (_) {}
        if (lastRefreshEl) {
            lastRefreshEl.textContent = new Date(now).toLocaleString(getLocale());
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
        const services = getOpendataServices();
        const names = Object.keys(services).sort();
        if (dataSourceSelectEl) {
            dataSourceSelectEl.replaceChildren();
            const noneOpt = document.createElement("option");
            noneOpt.value = "";
            noneOpt.textContent = t("empty.none");
            dataSourceSelectEl.appendChild(noneOpt);
            for (const name of names) {
                const opt = document.createElement("option");
                opt.value = name;
                const total = opendataTotals[name];
                opt.textContent = total === undefined ? name : `${name} (${total})`;
                dataSourceSelectEl.appendChild(opt);
            }
            if (!names.includes(dataSource)) {
                dataSource = "";
                saveDataSource();
            }
            dataSourceSelectEl.value = dataSource;
        }
        const filter = dataSource;
        let allEntries: [string, any][] = [];
        if (filter) {
            const source = dataPagination.onlyVisible ? currentOpendata[filter] : dataPagination.allItems[filter];
            if (source && source.opendata) {
                allEntries = Object.entries(source.opendata as Record<string, any>);
            }
        }
        const pageEntries = paginate(dataPagination, allEntries);
        updatePaginationUI(dataPagination, allEntries.length);

        for (const [id, od] of pageEntries) {
            const tr = document.createElement("tr");
            tr.title = id;

            const tdSource = document.createElement("td");
            tdSource.textContent = filter;

            const tdId = document.createElement("td");
            tdId.textContent = od.reference ?? id;

            const tdDesc = document.createElement("td");
            tdDesc.className = "rw-desc";
            tdDesc.textContent = od.description || "";

            const tdPos = document.createElement("td");
            tdPos.style.fontFamily = "monospace";
            tdPos.style.fontSize = "11px";
            if (od.latitude && od.longitude) {
                tdPos.textContent = `${od.latitude.toFixed(5)}, ${od.longitude.toFixed(5)}`;
            } else if (od.polygons && od.polygons.length > 0) {
                tdPos.textContent = t("dropzone.polygon");
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
        if (allEntries.length === 0) {
            const tr = document.createElement("tr");
            const td = document.createElement("td");
            td.colSpan = 4;
            td.style.textAlign = "center";
            td.style.color = "#999";
            td.style.padding = "16px";
            td.textContent = dataSource
                ? (dataPagination.onlyVisible ? t("empty.no_data_visible") : t("empty.no_data"))
                : t("empty.no_source");
            tr.appendChild(td);
            dataTableBody.appendChild(tr);
        }
        const total = opendataTotals[filter] ?? allEntries.length;
        const label = dataSource ? t("panel.data_count", { count: String(allEntries.length), total: String(total) }) : t("panel.data");
        const titleEl = document.getElementById("rw-data-title");
        if (titleEl) titleEl.textContent = label;
        if (dataToggleBtn) dataToggleBtn.textContent = label;
        const hasSource = !!dataSource;
        const canRefresh = hasSource && !!getOpendataDescriptorUrl(services[dataSource]);
        if (dataUpdateBtn) {
            dataUpdateBtn.disabled = !canRefresh;
            dataUpdateBtn.title = canRefresh
                ? t("btn.refresh_opendata_title")
                : t("btn.no_url_tooltip");
        }
        if (dataDeleteBtn) dataDeleteBtn.disabled = !hasSource;
        if (dataEditBtn) dataEditBtn.disabled = !hasSource;
    } catch (e) {
        console.warn("[Roadwork] Failed to render data panel:", e);
    }
}

async function refreshDataPanelFromViewport() {
    if (!dataSource) return;
    const bounds = getViewportBounds();
    if (!bounds) return;
    try {
        currentOpendata[dataSource] = await queryOpendataInViewport(dataSource, bounds);
    } catch (e) {
        console.warn("[Roadwork] Failed to refresh data panel from viewport:", e);
    }
    renderOpendataToMap();
    updateDataPanel();
}

function createDataPanel() {
    dataToggleBtn = document.createElement("button");
    dataToggleBtn.className = "rw-data-toggle-btn";
    dataToggleBtn.textContent = t("panel.data");
    dataToggleBtn.addEventListener("click", () => {
        console.info("[Roadwork] Opening data panel", { dataPanelEl: !!dataPanelEl });
        if (!dataPanelEl) return;
        dataPanelEl.classList.remove("rw-hidden");
        dataToggleBtn.style.display = "none";
        updateDataPanel();
        refreshDataPanelFromViewport();
    });
    if (toolbarEl) {
        toolbarEl.appendChild(dataToggleBtn);
        syncToolbarButtonVisibility(dataToggleBtn);
    }

    dataPanelEl = document.createElement("div");
    dataPanelEl.className = "rw-data-panel rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-data-header";

    const title = document.createElement("h4");
    title.id = "rw-data-title";
    title.textContent = t("panel.data");

    const headerBtns = document.createElement("div");
    headerBtns.style.cssText = "display:flex;gap:4px;";

    const refreshBtn = document.createElement("button");
    refreshBtn.textContent = "\u21bb";
    refreshBtn.title = t("btn.refresh");
    refreshBtn.addEventListener("click", () => refreshOpendata());

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = t("btn.close");
    closeBtn.addEventListener("click", () => {
        dataPanelEl.classList.add("rw-hidden");
        dataToggleBtn.style.display = "block";
    });

    headerBtns.appendChild(refreshBtn);
    headerBtns.appendChild(closeBtn);
    header.appendChild(title);
    const buildBadge = document.createElement("span");
    buildBadge.style.cssText = "font-size:10px;color:#999;margin-left:8px;align-self:center;";
    buildBadge.textContent = t("panel.build");
    header.appendChild(buildBadge);
    header.appendChild(headerBtns);

    const tableWrap = document.createElement("div");
    tableWrap.className = "rw-data-table-wrap";

    const table = document.createElement("table");
    table.className = "roadwork-table";
    const thead = document.createElement("thead");
    const headerRow = document.createElement("tr");
    for (const col of [t("table.source"), t("table.id"), t("table.description"), t("table.position")]) {
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
    dataSourceSelectEl.title = t("opendata.source_select");
    dataSourceSelectEl.addEventListener("change", () => {
        dataSource = dataSourceSelectEl.value;
        saveDataSource();
        dataPagination.page = 0;
        updateDataPanel();
        refreshDataPanelFromViewport();
    });
    controls.appendChild(dataSourceSelectEl);

    dataUpdateBtn = document.createElement("button");
    dataUpdateBtn.textContent = t("btn.refresh");
    dataUpdateBtn.title = t("btn.refresh_opendata_title");
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
    dataEditBtn.textContent = t("btn.edit");
    dataEditBtn.title = t("btn.edit_opendata_title");
    dataEditBtn.disabled = true;
    dataEditBtn.addEventListener("click", () => {
        if (!dataSource) return;
        editOpendataService(dataSource);
    });
    controls.appendChild(dataEditBtn);

    dataDeleteBtn = document.createElement("button");
    dataDeleteBtn.textContent = t("btn.delete");
    dataDeleteBtn.title = t("btn.delete_opendata_title");
    dataDeleteBtn.disabled = true;
    dataDeleteBtn.addEventListener("click", () => {
        if (!dataSource) return;
        if (!confirm(t("confirm.delete_source", { name: dataSource }))) return;
        removeOpendataService(dataSource);
        setDataStatus(t("status.svc_deleted", { name: dataSource }), "success");
    });
    controls.appendChild(dataDeleteBtn);

    const createBtn = document.createElement("button");
    createBtn.textContent = t("btn.create");
    createBtn.title = t("btn.create_opendata_title");
    createBtn.addEventListener("click", openOpendataHelper);
    controls.appendChild(createBtn);

    dataPanelEl.appendChild(controls);

    dataStatusEl = document.createElement("div");
    dataStatusEl.className = "roadwork-status rw-hidden";
    dataStatusEl.textContent = "";
    dataPanelEl.appendChild(dataStatusEl);

    dataDropzoneEl = document.createElement("div");
    dataDropzoneEl.className = "rw-data-dropzone";
    dataDropzoneEl.textContent = t("dropzone.json");
    dataPanelEl.appendChild(dataDropzoneEl);

    dataPanelEl.appendChild(createPaginationRow(dataPagination, updateDataPanel));
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

function syncToolbarButtonVisibility(btn: HTMLElement) {
    if (toolbarEl && toolbarEl.classList.contains("rw-collapsed")) {
        btn.style.display = "none";
    }
}

let polygonesPanelEl: HTMLDivElement | null = null;
let polygonesToggleBtn: HTMLButtonElement | null = null;
let polygonesPanelBody: HTMLDivElement | null = null;
let polygonesDropzoneEl: HTMLDivElement | null = null;

function addPolygonGroup(name: string, features: SdkFeature[]) {
    const gid = "group_" + nextGroupId;
    const prefixed = features
        .map(f => ({ ...f, id: gid + "-" + f.id }));
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

function hidePolygonGroup(group) {
    group.visible = false;
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

function getPolygonFeatures(group) {
    return (group.features || []).filter(f => f.geometry && f.geometry.type === 'Polygon');
}

function groupHasPolygon(group) {
    return getPolygonFeatures(group).length > 0;
}

function getMapCommentModel(mapCommentId: string) {
    const w = (window as any).W;
    return w?.model?.mapComments?.getObjectById?.(mapCommentId) ?? null;
}

function clearMapCommentEndDate(mapCommentId: string) {
    const modelComment = getMapCommentModel(mapCommentId);
    if (!modelComment) {
        console.warn("[Roadwork] Could not resolve internal map comment; keeping far-future endDate fallback");
        return;
    }
    const w = (window as any).W;
    const UpdateObject = (window as any).require?.("Waze/Action/UpdateObject");
    if (!w?.model?.actionManager?.add || !UpdateObject) {
        console.warn("[Roadwork] UpdateObject/actionManager unavailable; keeping far-future endDate fallback");
        return;
    }
    w.model.actionManager.add(new UpdateObject(modelComment, { endDate: null }));
}

function createMapCommentFromGroup(group) {
    const polys = getPolygonFeatures(group);
    if (polys.length === 0) {
        setStatus(t("polygones.no_polygon"), "error");
        return;
    }
    if (!wmeSDK?.DataModel?.MapComments) {
        setStatus(t("polygones.create_failed", { error: "MapComments unavailable" }), "error");
        return;
    }
    try {
        for (const f of polys) {
            const created = wmeSDK.DataModel.MapComments.addComment({
                subject: group.name,
                body: group.name,
                geometry: f.geometry,
                endDate: 4102444800,
            });
            // todo : remove that endDate
            clearMapCommentEndDate(created.id);
        }
        setStatus(t("polygones.comment_created", { count: String(polys.length) }), "success");
        hidePolygonGroup(group);
    } catch (e) {
        setStatus(t("polygones.create_failed", { error: (e as any)?.message || String(e) }), "error");
    }
}

function createPoiFromGroup(group) {
    const polys = getPolygonFeatures(group);
    if (polys.length === 0) {
        setStatus(t("polygones.no_polygon"), "error");
        return;
    }
    if (!wmeSDK?.DataModel?.Venues) {
        setStatus(t("polygones.create_failed", { error: "Venues unavailable" }), "error");
        return;
    }
    try {
        for (const f of polys) {
            wmeSDK.DataModel.Venues.addVenue({ category: "OTHER", geometry: f.geometry });
        }
        setStatus(t("polygones.poi_created", { count: String(polys.length) }), "success");
        hidePolygonGroup(group);
    } catch (e) {
        setStatus(t("polygones.create_failed", { error: (e as any)?.message || String(e) }), "error");
    }
}

function centerOnFirstFeature(features: SdkFeature[]) {
    if (features.length === 0 || !wmeSDK?.Map) return;
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

function showWktCreateDialog() {
    const overlay = document.createElement("div");
    overlay.className = "rw-opendata-export-overlay";

    const box = document.createElement("div");
    box.className = "rw-opendata-export-box";

    const header = document.createElement("div");
    header.className = "rw-opendata-export-header";
    const title = document.createElement("h4");
    title.textContent = t("wkt.create_title");
    const closeBtn = document.createElement("button");
    closeBtn.className = "roadwork-btn roadwork-btn-icon";
    closeBtn.textContent = "\u00d7";
    closeBtn.addEventListener("click", () => overlay.remove());
    header.appendChild(title);
    header.appendChild(closeBtn);
    box.appendChild(header);

    const body = document.createElement("div");
    body.className = "rw-wkt-create-body";

    const nameLabel = document.createElement("label");
    nameLabel.textContent = t("wkt.name_label");
    body.appendChild(nameLabel);

    const nameInput = document.createElement("input");
    nameInput.className = "rw-wkt-create-name";
    nameInput.type = "text";
    body.appendChild(nameInput);

    const textarea = document.createElement("textarea");
    textarea.className = "rw-opendata-export-textarea rw-wkt-create-textarea";
    textarea.placeholder = t("wkt.paste_placeholder");
    body.appendChild(textarea);
    box.appendChild(body);

    const actions = document.createElement("div");
    actions.className = "rw-opendata-export-actions";

    const createBtn = document.createElement("button");
    createBtn.className = "roadwork-btn";
    createBtn.textContent = t("btn.create");
    createBtn.disabled = true;
    const onCreate = () => {
        const features = parseWkt(textarea.value);
        if (features.length === 0) {
            alert(t("import.no_valid_geometry"));
            return;
        }
        overlay.remove();
        addPolygonGroup(nameInput.value.trim(), features);
        centerOnFirstFeature(features);
    };
    createBtn.addEventListener("click", onCreate);
    nameInput.addEventListener("input", () => {
        createBtn.disabled = nameInput.value.trim().length === 0;
    });
    nameInput.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && !createBtn.disabled) onCreate();
    });
    actions.appendChild(createBtn);
    box.appendChild(actions);

    overlay.appendChild(box);
    overlay.addEventListener("click", (e) => {
        if (e.target === overlay) overlay.remove();
    });
    document.body.appendChild(overlay);
    nameInput.focus();
}

function createPolygonesUI() {
    polygonesToggleBtn = document.createElement("button");
    polygonesToggleBtn.className = "rw-polygones-toggle-btn";
    polygonesToggleBtn.textContent = t("panel.polygones");
    polygonesToggleBtn.addEventListener("click", () => {
        polygonesPanelEl.classList.remove("rw-hidden");
        polygonesToggleBtn.style.display = "none";
        updatePolygonesPanel();
    });
    if (toolbarEl) {
        toolbarEl.appendChild(polygonesToggleBtn);
        syncToolbarButtonVisibility(polygonesToggleBtn);
    }

    polygonesPanelEl = document.createElement("div");
    polygonesPanelEl.className = "rw-polygones-panel rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-polygones-header";

    const title = document.createElement("h4");
    title.textContent = t("panel.polygones");

    const headerBtns = document.createElement("div");
    headerBtns.style.cssText = "display:flex;gap:4px;";

    const resetBtn = document.createElement("button");
    resetBtn.textContent = t("btn.reset");
    resetBtn.title = t("btn.reset");
    resetBtn.addEventListener("click", () => {
        if (confirm(t("confirm.delete_polygones"))) {
            clearAllPolygonGroups();
        }
    });

    const newBtn = document.createElement("button");
    newBtn.className = "rw-polygones-new-btn";
    newBtn.textContent = t("btn.create");
    newBtn.title = t("wkt.create_title");
    newBtn.addEventListener("click", () => showWktCreateDialog());

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = t("btn.close");
    closeBtn.addEventListener("click", () => {
        polygonesPanelEl.classList.add("rw-hidden");
        polygonesToggleBtn.style.display = "block";
    });

    headerBtns.appendChild(resetBtn);
    headerBtns.appendChild(newBtn);
    headerBtns.appendChild(closeBtn);
    header.appendChild(title);
    header.appendChild(headerBtns);

    polygonesPanelBody = document.createElement("div");    polygonesPanelBody.className = "rw-polygones-body";

    polygonesDropzoneEl = document.createElement("div");
    polygonesDropzoneEl.className = "rw-polygones-dropzone";
    polygonesDropzoneEl.textContent = t("dropzone.wkt");

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
        empty.textContent = t("empty.no_polygones");
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
        toggleCheck.title = group.visible ? t("opendata.toggle_hide") : t("opendata.toggle_show");
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
        deleteBtn.title = t("opendata.delete_title");
        deleteBtn.addEventListener("click", () => {
            if (confirm(t("confirm.delete_group", { name: group.name }))) {
                removePolygonGroup(group.id);
            }
        });

        row.appendChild(toggleCheck);
        row.appendChild(nameInput);
        row.appendChild(countSpan);
        if (groupHasPolygon(group)) {
            const centerBtn = document.createElement("button");
            centerBtn.className = "rw-polygon-group-action";
            centerBtn.textContent = "\uD83C\uDFAF";
            centerBtn.title = t("polygones.center");
            centerBtn.addEventListener("click", () => centerOnFirstFeature(group.features));

            const commentBtn = document.createElement("button");
            commentBtn.className = "rw-polygon-group-action";
            commentBtn.textContent = "\uD83D\uDCAC";
            commentBtn.title = t("polygones.add_comment");
            commentBtn.addEventListener("click", () => createMapCommentFromGroup(group));

            const poiBtn = document.createElement("button");
            poiBtn.className = "rw-polygon-group-action";
            poiBtn.textContent = "\uD83D\uDCCD";
            poiBtn.title = t("polygones.add_poi");
            poiBtn.addEventListener("click", () => createPoiFromGroup(group));

            row.appendChild(centerBtn);
            row.appendChild(commentBtn);
            row.appendChild(poiBtn);
        }
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
            const text = evt.target.result as string;
            const features = parseWkt(text);
            if (features.length === 0) {
                alert(t("import.no_valid_geometry"));
                return;
            }
            const fileName = file.name.replace(/\.[^/.]+$/, "");
            addPolygonGroup(fileName, features);
            updatePolygonesPanel();
            centerOnFirstFeature(features);
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
    heading.textContent = t("panel.settings");
    panelEl.appendChild(heading);

    const versionLine = document.createElement("div");
    versionLine.style.cssText = "font-size:11px;color:#888;margin-bottom:8px;";
    versionLine.textContent = t("panel.version");
    panelEl.appendChild(versionLine);

    const launchBtn = document.createElement("button");
    launchBtn.className = "roadwork-btn";
    launchBtn.textContent = t("btn.launch");
    launchBtn.title = t("btn.launch_title");
    launchBtn.addEventListener("click", () => {
        console.log("[Roadwork] launching app overlay");
        window.postMessage({ type: "ROADWORK_OPEN_APP" }, "*");
    });
    panelEl.appendChild(launchBtn);

    const exploreBtn = document.createElement("button");
    exploreBtn.className = "roadwork-btn";
    exploreBtn.textContent = t("btn.explore_db");
    exploreBtn.title = t("btn.explore_db_title");
    exploreBtn.addEventListener("click", () => {
        console.log("[Roadwork] opening DB explorer");
        window.postMessage({ type: "ROADWORK_OPEN_APP", dbExplorer: true }, "*");
    });
    panelEl.appendChild(exploreBtn);

    const sourcesBtn = document.createElement("button");
    sourcesBtn.className = "roadwork-btn";
    sourcesBtn.textContent = t("btn.sources");
    sourcesBtn.title = t("btn.sources_title");
    sourcesBtn.addEventListener("click", () => {
        console.log("[Roadwork] opening sources window");
        void openSourcesWindow();
    });
    panelEl.appendChild(sourcesBtn);

    const resetBtn = document.createElement("button");
    resetBtn.className = "roadwork-btn roadwork-btn-danger";
    resetBtn.textContent = t("btn.reset");
    resetBtn.title = t("btn.reset");
    resetBtn.addEventListener("click", () => clearExtensionStorage());
    const dangerDiv = document.createElement("div");
    dangerDiv.style.cssText = "margin-top:12px;padding-top:12px;border-top:1px solid #e5e7eb;";
    dangerDiv.appendChild(resetBtn);
    panelEl.appendChild(dangerDiv);

    const logLevelDiv = document.createElement("div");
    logLevelDiv.className = "roadwork-field";

    const lbl3 = document.createElement("label");
    lbl3.textContent = t("log_level.label");
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

    const langDiv = document.createElement("div");
    langDiv.className = "roadwork-field";

    const langLabel = document.createElement("label");
    langLabel.textContent = t("language.label");
    const langSel = document.createElement("select");
    langSel.id = "rw-language-select";
    const langOptions: [string, string][] = [
        ["auto", t("language.auto")],
        ["fr", "Français"],
        ["en", "English"],
    ];
    for (const [val, label] of langOptions) {
        const o = document.createElement("option");
        o.value = val;
        o.textContent = label;
        langSel.appendChild(o);
    }
    try {
        langSel.value = localStorage.getItem("roadwork-wme-language") || "auto";
    } catch (_) {
        langSel.value = "auto";
    }
    langSel.addEventListener("change", () => {
        try {
            localStorage.setItem("roadwork-wme-language", langSel.value);
        } catch (_) {}
        window.location.reload();
    });
    langDiv.appendChild(langLabel);
    langDiv.appendChild(langSel);
    panelEl.appendChild(langDiv);

    tabPane.appendChild(panelEl);
}

interface SourceInfo {
    name: string;
    country?: string | null;
    source_name: string;
    producer?: string | null;
    licence_name?: string | null;
    licence_url?: string | null;
    source_url?: string | null;
    descriptor_url?: string | null;
}

function buildAboutLink(url: string | null | undefined, text: string): HTMLAnchorElement {
    const link = document.createElement("a");
    if (url) {
        link.href = url;
        link.target = "_blank";
        link.rel = "noopener noreferrer";
    } else {
        link.classList.add("rw-about-link-disabled");
    }
    link.textContent = text;
    return link;
}

let sourceDetailWindowEl: HTMLDivElement | null = null;

function buildSourceDetailWindow() {
    sourceDetailWindowEl = document.createElement("div");
    sourceDetailWindowEl.className = "rw-about-window rw-source-detail-window rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-about-header";

    const title = document.createElement("h4");
    title.textContent = t("sources.detail_title");

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = t("btn.close");
    closeBtn.addEventListener("click", () => {
        sourceDetailWindowEl.classList.add("rw-hidden");
    });

    header.appendChild(title);
    header.appendChild(closeBtn);

    const body = document.createElement("div");
    body.className = "rw-about-body";
    body.id = "rw-source-detail-body";

    sourceDetailWindowEl.appendChild(header);
    sourceDetailWindowEl.appendChild(body);
    document.body.appendChild(sourceDetailWindowEl);

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = sourceDetailWindowEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        sourceDetailWindowEl.style.left = e.clientX - dragOffsetX + "px";
        sourceDetailWindowEl.style.top = e.clientY - dragOffsetY + "px";
        sourceDetailWindowEl.style.right = "auto";
        sourceDetailWindowEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });
}

function openSourceDetail(detail?: SourceDetail) {
    try {
        if (!sourceDetailWindowEl) {
            buildSourceDetailWindow();
        }
        const body = sourceDetailWindowEl.querySelector<HTMLDivElement>("#rw-source-detail-body");
        if (!body) return;

        body.innerHTML = "";
        if (!detail) {
            body.textContent = t("sources.no_info");
            sourceDetailWindowEl.classList.remove("rw-hidden");
            return;
        }
        const title = sourceDetailWindowEl.querySelector<HTMLHeadingElement>(".rw-about-header h4");
        if (title) title.textContent = t("sources.detail_title");

        const nameLine = document.createElement("div");
        nameLine.className = "rw-about-source-name";
        nameLine.textContent = detail.displayName || detail.sourceName;

        const serviceKey = document.createElement("span");
        serviceKey.className = "rw-about-service-key";
        serviceKey.textContent = detail.sourceName;
        nameLine.appendChild(serviceKey);
        body.appendChild(nameLine);

        const details = document.createElement("div");
        details.className = "rw-about-source-details";
        const detailLines: string[] = [];
        if (detail.country) {
            detailLines.push(`${t("sources.country")}: ${detail.country}`);
        }
        if (detail.producer) {
            detailLines.push(`${t("about.producer")}: ${detail.producer}`);
        }
        if (detailLines.length > 0) {
            details.textContent = detailLines.join(" — ");
            body.appendChild(details);
        }

        const links = document.createElement("div");
        links.className = "rw-about-links";
        if (!detail.isLocal && detail.descriptorUrl) {
            links.appendChild(buildAboutLink(detail.descriptorUrl, t("sources.descriptor_url")));
        }
        if (detail.licenceName) {
            links.appendChild(buildAboutLink(detail.licenceUrl, `${t("about.licence")}: ${detail.licenceName}`));
        }
        if (detail.sourceUrl) {
            links.appendChild(buildAboutLink(detail.sourceUrl, t("about.source_page")));
        }
        if (links.childElementCount > 0) {
            body.appendChild(links);
        }

        if (body.childElementCount === 0) {
            body.textContent = t("sources.no_info");
        }
        sourceDetailWindowEl.classList.remove("rw-hidden");
    } catch (e) {
        console.error("[Roadwork] Failed to open source detail:", e);
    }
}

let sourcesWindowEl: HTMLDivElement | null = null;

async function openSourcesWindow() {
    try {
        if (!sourcesWindowEl) {
            buildSourcesWindow();
        }
        sourcesWindowEl.classList.remove("rw-hidden");
    } catch (e) {
        console.error("[Roadwork] Failed to open sources window:", e);
    }
}

interface SourceDetail {
    sourceName: string;
    displayName: string;
    country?: string;
    producer?: string;
    licenceName?: string;
    licenceUrl?: string;
    sourceUrl?: string;
    descriptorUrl?: string;
    isLocal?: boolean;
}

interface SourceRow {
    name: string;
    country: string;
    origin: string;
    originLabel: string;
    originTooltip?: string;
    detail?: SourceDetail;
}

interface SourceColumn {
    key: keyof SourceRow;
    label: string;
    sortKey?: keyof SourceRow;
}

function sortValue(row: SourceRow, key: keyof SourceRow): string {
    const v = row[key];
    return typeof v === "string" ? v : "";
}

function sortSourceRows(rows: SourceRow[], keys: Array<keyof SourceRow>, dirs: number[]) {
    return rows.sort((a, b) => {
        for (let i = 0; i < keys.length; i++) {
            const c = sortValue(a, keys[i]).localeCompare(sortValue(b, keys[i]));
            if (c !== 0) return c * dirs[i];
        }
        return 0;
    });
}

function buildSourcesWindow() {
    sourcesWindowEl = document.createElement("div");
    sourcesWindowEl.className = "rw-about-window rw-sources-window rw-hidden";

    const header = document.createElement("div");
    header.className = "rw-about-header";

    const title = document.createElement("h4");
    title.textContent = t("sources.title");

    const closeBtn = document.createElement("button");
    closeBtn.textContent = "\u00d7";
    closeBtn.title = t("btn.close");
    closeBtn.addEventListener("click", () => {
        sourcesWindowEl.classList.add("rw-hidden");
    });

    header.appendChild(title);
    header.appendChild(closeBtn);

    const body = document.createElement("div");
    body.className = "rw-about-body";

    sourcesWindowEl.appendChild(header);
    sourcesWindowEl.appendChild(body);
    document.body.appendChild(sourcesWindowEl);

    let isDragging = false;
    let dragOffsetX = 0;
    let dragOffsetY = 0;
    header.addEventListener("mousedown", (e) => {
        if ((e.target as HTMLElement).tagName === "BUTTON") return;
        isDragging = true;
        const rect = sourcesWindowEl.getBoundingClientRect();
        dragOffsetX = e.clientX - rect.left;
        dragOffsetY = e.clientY - rect.top;
        e.preventDefault();
    });
    document.addEventListener("mousemove", (e) => {
        if (!isDragging) return;
        sourcesWindowEl.style.left = e.clientX - dragOffsetX + "px";
        sourcesWindowEl.style.top = e.clientY - dragOffsetY + "px";
        sourcesWindowEl.style.right = "auto";
        sourcesWindowEl.style.bottom = "auto";
    });
    document.addEventListener("mouseup", () => {
        isDragging = false;
    });

    void populateSourcesWindow(body);
}

function buildSourcesTableFor(rows: SourceRow[], columns: SourceColumn[], initialSort?: Array<keyof SourceRow>): HTMLElement {
    const initialKeys = (initialSort || columns.map((c) => c.key)).slice();
    const dirs: number[] = initialKeys.map(() => 1);
    const sortedRows = rows.slice();

    const render = () => {
        tbody.innerHTML = "";
        for (const row of sortedRows) {
            const tr = document.createElement("tr");
            for (const col of columns) {
                const td = document.createElement("td");
                if (col.sortKey === "origin" && row.originTooltip) {
                    td.classList.add("rw-sources-origin");
                    td.title = row.originTooltip;
                }
                td.textContent = col.key === "country" && !row.country ? "—" : row[col.key] as string;
                tr.appendChild(td);
            }
            const infoTd = document.createElement("td");
            infoTd.className = "rw-sources-info";
            const infoBtn = document.createElement("button");
            infoBtn.type = "button";
            infoBtn.className = "rw-sources-info-btn";
            infoBtn.textContent = "?";
            infoBtn.title = t("sources.info");
            infoBtn.addEventListener("click", (e) => {
                e.stopPropagation();
                openSourceDetail(row.detail);
            });
            infoTd.appendChild(infoBtn);
            tr.appendChild(infoTd);
            tbody.appendChild(tr);
        }
    };

    const table = document.createElement("table");
    table.className = "rw-sources-table";
    const thead = document.createElement("thead");
    const thr = document.createElement("tr");
    const headerElements: HTMLTableCellElement[] = [];
    columns.forEach((col, i) => {
        const sortKey = col.sortKey || col.key;
        const th = document.createElement("th");
        th.className = "sortable";
        th.textContent = col.label;
        th.addEventListener("click", () => {
            for (let j = 0; j < dirs.length; j++) dirs[j] = 1;
            const idx = initialKeys.indexOf(sortKey);
            dirs[idx] = -dirs[idx];
            sortSourceRows(sortedRows, [sortKey], [dirs[idx]]);
            headerElements.forEach((h, j) => h.classList.toggle("asc", j === i && dirs[idx] === 1));
            headerElements.forEach((h, j) => h.classList.toggle("desc", j === i && dirs[idx] === -1));
            render();
        });
        headerElements.push(th);
        thr.appendChild(th);
    });
    const infoTh = document.createElement("th");
    infoTh.className = "rw-sources-info-th";
    infoTh.textContent = "";
    thr.appendChild(infoTh);
    thead.appendChild(thr);
    table.appendChild(thead);
    const tbody = document.createElement("tbody");
    table.appendChild(tbody);

    sortSourceRows(sortedRows, initialKeys, dirs);
    const firstCol = columns.findIndex((c) => (c.sortKey || c.key) === initialKeys[0]);
    if (firstCol >= 0) {
        headerElements[firstCol].classList.add("asc");
    }
    render();
    return table;
}

const ORIGIN_OFFICIAL = "__official";
const ORIGIN_LOCAL = "__local";

// Builds a human-readable label, an optional tooltip (full URL) and a sort
// token for a roadwork source origin. Custom index URLs sort after the
// built-in "Official" and "Local" origins.
function originInfo(origin: string): { label: string; tooltip?: string; token: string } {
    if (origin === ORIGIN_OFFICIAL) {
        return { label: t("sources.official"), token: "\u0000official" };
    }
    if (origin === ORIGIN_LOCAL) {
        return { label: t("sources.local"), token: "\u0001local" };
    }
    const cleaned = origin.replace(/^https?:\/\//i, "").replace(/\/+$/, "");
    let label = cleaned;
    const segments = cleaned.split("/");
    if (segments.length >= 2) {
        label = segments.slice(0, 2).join("/");
    }
    if (label.length > 40) {
        label = label.slice(0, 37) + "…";
    }
    return { label, tooltip: origin, token: "\u0002" + cleaned };
}

async function populateSourcesWindow(body: HTMLDivElement) {
    const loading = document.createElement("div");
    loading.className = "rw-about-loading";
    loading.textContent = t("sources.loading");
    body.appendChild(loading);

    let info: SourceInfo[];
    try {
        info = await rpcCall("get_sources_info") as SourceInfo[];
    } catch (e) {
        console.warn("[Roadwork] Failed to load sources info:", e);
        loading.textContent = t("sources.error");
        return;
    }
    if (!sourcesWindowEl || loading.parentNode !== body) {
        return;
    }
    body.removeChild(loading);

    // Roadwork sources: one flat sortable table with an Origin column.
    const rwSection = document.createElement("div");
    rwSection.className = "rw-sources-section";
    const rwTitle = document.createElement("h4");
    rwTitle.textContent = t("sources.roadworks");
    rwSection.appendChild(rwTitle);

    const knownNames = new Set(info.map((s) => s.name));
    const local = loadLocalDescriptors();
    const customOrigins = loadCustomOriginsCache() || {};
    const customUrls = Array.isArray(settings.customSources) ? settings.customSources : [];

    // Per source, resolve the origin with a strict precedence: custom index
    // URL, then local (an edited/imported descriptor overrides any other
    // origin), then the built-in "Official" descriptors.
    const originByKey: Record<string, string> = {};
    const assigned = new Set<string>();
    for (const url of customUrls) {
        Object.keys(customOrigins).forEach((n) => {
            if (customOrigins[n] === url && knownNames.has(n) && !assigned.has(n)) {
                originByKey[n] = url;
                assigned.add(n);
            }
        });
    }
    Object.keys(local).forEach((n) => {
        if (knownNames.has(n) && !assigned.has(n)) {
            originByKey[n] = "__local";
            assigned.add(n);
        }
    });
    info.forEach((s) => {
        if (!assigned.has(s.name) && originByKey[s.name] === undefined) {
            originByKey[s.name] = "__official";
        }
    });

    const rwRows: SourceRow[] = info.map((s) => {
        const origin = originByKey[s.name] || "__official";
        const { label, tooltip, token } = originInfo(origin);
        return {
            name: s.source_name || s.name,
            country: s.country || "",
            origin: token,
            originLabel: label,
            originTooltip: tooltip,
            detail: {
                sourceName: s.name,
                displayName: s.source_name || s.name,
                country: s.country || undefined,
                producer: s.producer || undefined,
                licenceName: s.licence_name || undefined,
                licenceUrl: s.licence_url || undefined,
                sourceUrl: s.source_url || undefined,
                descriptorUrl: origin === "__official" ? s.descriptor_url || undefined : undefined,
                isLocal: origin === "__local",
            },
        };
    });

    const rwColumns: SourceColumn[] = [
        { key: "name", label: t("sources.name") },
        { key: "country", label: t("sources.country") },
        { key: "originLabel", sortKey: "origin", label: t("sources.origin") },
    ];
    if (rwRows.length === 0) {
        const empty = document.createElement("div");
        empty.className = "rw-sources-empty";
        empty.textContent = t("sources.none");
        rwSection.appendChild(empty);
    } else {
        rwSection.appendChild(buildSourcesTableFor(rwRows, rwColumns, ["origin", "country", "name"]));
    }
    body.appendChild(rwSection);

    // Opendata sources: same sortable table style as roadworks.
    const odSection = document.createElement("div");
    odSection.className = "rw-sources-section";
    const odTitle = document.createElement("h4");
    odTitle.textContent = t("sources.opendata");
    odSection.appendChild(odTitle);

    const odServices = getOpendataServices();
    const odRows: SourceRow[] = [];
    const seen = new Set<string>();
    for (const [name, svc] of Object.entries(odServices) as [string, any][]) {
        if (seen.has(name)) continue;
        seen.add(name);
        let displayName = name;
        let country = "";
        const detail: SourceDetail = { sourceName: name, displayName: name };
        if (svc && typeof svc.descriptor === "string") {
            try {
                const parsed = JSON.parse(svc.descriptor);
                const md = parsed?.metadata || {};
                if (typeof md.name === "string" && md.name.trim()) displayName = md.name;
                if (typeof md.country === "string") country = md.country;
                detail.displayName = displayName;
                if (country) detail.country = country;
                if (typeof md.producer === "string" && md.producer.trim()) detail.producer = md.producer;
                if (typeof md.licence_name === "string" && md.licence_name.trim()) detail.licenceName = md.licence_name;
                if (typeof md.licence_url === "string" && md.licence_url.trim()) detail.licenceUrl = md.licence_url;
                if (typeof md.source_url === "string" && md.source_url.trim()) detail.sourceUrl = md.source_url;
            } catch (_) {}
        }
        detail.isLocal = false;
        odRows.push({
            name: displayName,
            country,
            origin: "\u0002opendata",
            originLabel: t("sources.opendata"),
            detail,
        });
    }
    if (odRows.length === 0) {
        const empty = document.createElement("div");
        empty.className = "rw-sources-empty";
        empty.textContent = t("sources.none");
        odSection.appendChild(empty);
    } else {
        const odColumns: SourceColumn[] = [
            { key: "name", label: t("sources.name") },
            { key: "country", label: t("sources.country") },
            { key: "originLabel", sortKey: "origin", label: t("sources.origin") },
        ];
        odSection.appendChild(buildSourcesTableFor(odRows, odColumns, ["origin", "name", "country"]));
    }
    body.appendChild(odSection);
}

async function init() {
    await applyLogLevel(settings.logLevel);

    loadSettings();
    loadHideFinished();
    loadSortState();
    loadDataSource();
    loadPaginationSettings();
    await loadOpendataServices().catch((e) => {
        console.warn("[Roadwork] Failed to load opendata services:", e);
    });
    await syncCustomDescriptorsToWasm(false).catch((e) => {
        console.warn("[Roadwork] Failed to sync custom descriptors:", e);
    });
    await pruneStaleDescriptors().catch((e) => {
        console.warn("[Roadwork] Failed to prune stale descriptors:", e);
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
    grip.title = t("grip.title");
    grip.setAttribute("aria-label", t("grip.aria"));
    toolbarEl.appendChild(grip);
    const collapseBtn = document.createElement("button");
    collapseBtn.type = "button";
    collapseBtn.className = "rw-toolbar-collapse-btn";
    const applyToolbarCollapsed = (collapsed: boolean) => {
        toolbarEl.classList.toggle("rw-collapsed", collapsed);
        for (const child of Array.from(toolbarEl.children)) {
            if (child === grip || child === collapseBtn) continue;
            (child as HTMLElement).style.display = collapsed ? "none" : "";
        }
        collapseBtn.textContent = collapsed ? "\u00bb" : "\u00ab";
        collapseBtn.title = t(collapsed ? "toolbar.expand" : "toolbar.collapse");
        collapseBtn.setAttribute("aria-label", collapseBtn.title);
    };
    collapseBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    collapseBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const collapsed = !toolbarEl.classList.contains("rw-collapsed");
        applyToolbarCollapsed(collapsed);
        try {
            localStorage.setItem(TOOLBAR_COLLAPSED_KEY, String(collapsed));
        } catch (_) {}
    });
    toolbarEl.appendChild(collapseBtn);
    document.body.appendChild(toolbarEl);
    try {
        applyToolbarCollapsed(localStorage.getItem(TOOLBAR_COLLAPSED_KEY) === "true");
    } catch (_) {}
    try {
        const p = JSON.parse(localStorage.getItem(TOOLBAR_POSITION_KEY));
        if (typeof p?.x === "number" && typeof p?.y === "number") {
            const w = toolbarEl.offsetWidth;
            const h = toolbarEl.offsetHeight;
            if (p.x + w > 0 && p.y + h > 0 && p.x < window.innerWidth && p.y < window.innerHeight) {
                toolbarEl.style.left = p.x + "px";
                toolbarEl.style.top = p.y + "px";
                toolbarEl.style.right = "auto";
                toolbarEl.style.bottom = "auto";
            }
        }
    } catch (_) {}
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
            if (!isDragging) return;
            isDragging = false;
            const rect = toolbarEl.getBoundingClientRect();
            localStorage.setItem(
                TOOLBAR_POSITION_KEY,
                JSON.stringify({ x: Math.round(rect.left), y: Math.round(rect.top) }),
            );
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
            wktStatus.textContent = t("panel.wkt_count", { groupCount: String(groupCount), featCount: String(featCount) });
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
            roadworksPagination.allItems = currentRoadworks;
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
