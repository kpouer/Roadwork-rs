pub fn open_url(url: &str) {
    web_sys::window()
        .unwrap()
        .open_with_url_and_target(url, "_blank")
        .ok();
}
