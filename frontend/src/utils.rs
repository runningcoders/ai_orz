use web_sys::window;

pub fn local_storage() -> Option<web_sys::Storage> {
    window()?.local_storage().ok()?
}
