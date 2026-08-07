import "wme-sdk-typings";

declare module "Waze/MapEditor/UI/Userscripts/WmeSDK" {
    type WmeSDK = any;
    export type { WmeSDK };
}

declare global {
    const WASM_BYTES: string;

    const wasm_bindgen: {
        (init_input: unknown): Promise<unknown>;
        get_services(): unknown;
        get_roadworks(descriptor: unknown): Promise<unknown>;
        set_log_level(level: unknown): void;
        set_custom_descriptors(descriptors: unknown): void;
        set_opendata_custom_descriptors(descriptors: unknown): void;
        get_opendata(service: unknown): Promise<unknown>;
        parse_opendata(json: string, service_name: string): unknown;
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
