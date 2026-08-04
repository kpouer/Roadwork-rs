pub fn open_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .unwrap()
            .open_with_url_and_target(url, "_blank")
            .ok();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let _ = std::process::Command::new(opener).arg(url).spawn();
    }
}
