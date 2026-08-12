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
        clear_all_cache(): Promise<unknown>;
        set_log_level(level: unknown): void;
        set_custom_descriptors(descriptors: unknown): void;
        set_opendata_custom_descriptors(descriptors: unknown): void;
        get_opendata(service: unknown, forceRefresh: unknown): Promise<unknown>;
        get_opendata_cached(service: unknown): Promise<unknown>;
        get_opendata_counts(): Promise<unknown>;
        get_roadworks_in_bbox(service: unknown, latMin: unknown, lonMin: unknown, latMax: unknown, lonMax: unknown): Promise<unknown>;
        get_opendata_in_bbox(service: unknown, latMin: unknown, lonMin: unknown, latMax: unknown, lonMax: unknown): Promise<unknown>;
        store_opendata_data(service: unknown, dataJson: unknown): Promise<unknown>;
        clear_roadworks_cache(service: unknown): Promise<unknown>;
        clear_opendata_cache(service: unknown): Promise<unknown>;
        get_polygon_groups(): Promise<unknown>;
        save_polygon_groups(payload: unknown): Promise<unknown>;
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
}
