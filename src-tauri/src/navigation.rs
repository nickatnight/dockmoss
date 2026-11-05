/// Both windows load only the app's own pages: the bundled `tauri:` origin,
/// and, when `dev` is true, the Vite server on localhost.
pub fn is_app_origin(scheme: &str, host: Option<&str>, dev: bool) -> bool {
    scheme == "tauri" || (dev && host == Some("localhost"))
}

#[cfg(test)]
mod tests;
