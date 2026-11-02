use std::sync::atomic::AtomicBool;

pub struct TrustState(pub AtomicBool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayChange {
    ShowGrantItem,
    HideGrantItem,
}

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
