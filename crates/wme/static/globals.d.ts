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
    };

    namespace chrome {
        namespace runtime {
            function getURL(path: string): string;
        }
    }

    interface HTMLElement {
        contentWindow: Window | null;
    }
}
