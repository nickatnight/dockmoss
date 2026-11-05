use tauri::Url;

use super::is_app_origin;

fn allowed(url: &str, dev: bool) -> bool {
    let url = Url::parse(url).expect("a valid URL");
    is_app_origin(url.scheme(), url.host_str(), dev)
}

#[test]
fn only_the_apps_own_origin_is_navigable() {
    let cases = [
        ("tauri://localhost/settings.html", false, true),
        ("tauri://localhost/settings.html", true, true),
        ("http://localhost:5110/overlay.html", true, true),
        ("http://localhost:5110/", false, false),
        ("https://example.com/", true, false),
        ("file:///etc/passwd", true, false),
        ("http://localhost.example.com/", true, false),
        ("http://localhost@example.com/", true, false),
        ("data:text/html,x", true, false),
    ];

    for (url, dev, expected) in cases {
        assert_eq!(allowed(url, dev), expected, "{url} dev={dev}");
    }
}
