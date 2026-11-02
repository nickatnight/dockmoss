/// How the tray menu must change after an Accessibility trust reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayChange {
    ShowGrantItem,
    HideGrantItem,
}

/// Compares a fresh trust reading with the one the tray already reflects.
///
/// `previous` is `None` before the first reading. At that point the tray
/// still has the layout it was built with, Quit alone, which is the trusted
/// layout, so a first trusted reading needs no change.
pub fn tray_change(previous: Option<bool>, trusted: bool) -> Option<TrayChange> {
    let shown_as_trusted = previous.unwrap_or(true);
    match (shown_as_trusted, trusted) {
        (true, false) => Some(TrayChange::ShowGrantItem),
        (false, true) => Some(TrayChange::HideGrantItem),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
