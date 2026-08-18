import "wme-sdk-typings";

declare module "Waze/MapEditor/UI/Userscripts/WmeSDK" {
    type WmeSDK = any;
    export type { WmeSDK };
}

declare global {
    const WASM_BYTES: string;

    const wasm_bindgen: {
        (init_input: unknown): Promise<unknown>;
        open_store(): Promise<unknown>;
        get_services(): unknown;
        get_roadworks(descriptor: unknown, forceRefresh: unknown): Promise<unknown>;
        get_roadworks_cached(service: unknown): Promise<unknown>;
        clear_all_cache(): Promise<unknown>;
        set_log_level(level: unknown): void;
        set_custom_descriptors(descriptors: unknown): void;
        get_opendata_sources(): Promise<unknown>;
        save_opendata_source(name: unknown, descriptor: unknown, enabled: unknown, visible: unknown, oldName?: unknown): Promise<unknown>;
        set_opendata_source_flags(name: unknown, enabled: unknown, visible: unknown): Promise<unknown>;
        get_opendata(service: unknown, forceRefresh: unknown): Promise<unknown>;
        get_opendata_cached(service: unknown): Promise<unknown>;
        get_opendata_counts(): Promise<unknown>;
        get_roadworks_in_bbox(service: unknown, latMin: unknown, lonMin: unknown, latMax: unknown, lonMax: unknown): Promise<unknown>;
        get_opendata_in_bbox(service: unknown, latMin: unknown, lonMin: unknown, latMax: unknown, lonMax: unknown, limit?: unknown): Promise<unknown>;
        store_opendata_data(service: unknown, dataJson: unknown): Promise<unknown>;
        clear_roadworks_cache(service: unknown): Promise<unknown>;
        clear_opendata_cache(service: unknown): Promise<unknown>;
        get_polygon_groups(): Promise<unknown>;
        save_polygon_groups(payload: unknown): Promise<unknown>;
        get_db_overview(): Promise<unknown>;
        get_db_table(table: unknown, offset: unknown, limit: unknown, latMin?: unknown, lonMin?: unknown, latMax?: unknown, lonMax?: unknown, service?: unknown): Promise<unknown>;
        delete_db_row(table: unknown, keysJson: unknown): Promise<unknown>;
    };

    namespace chrome {
        namespace runtime {
            function getURL(path: string): string;
        }

        namespace action {
            namespace onClicked {
                function addListener(callback: () => void): void;
            }
        }

        namespace tabs {
            function create(props: { url: string }): void;
        }
    }

    interface HTMLElement {
        contentWindow: Window | null;
    }

    interface Window {
        __ROADWORK_LOCALE_DATA_ALL__?: Record<string, Record<string, string>>;
        __rw_i18n?: {
            t(key: string, vars?: Record<string, string | number>): string;
            setTranslations(locale: string, data: Record<string, string>): void;
            setFallbackTranslations(data: Record<string, string>): void;
            getLocale(): string;
            detectLocale(): string;
        };
    }
}
