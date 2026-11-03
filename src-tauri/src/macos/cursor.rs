use objc2_core_graphics::CGEvent;

use crate::geometry::ScreenPoint;

/// The pointer in global points with a top-left origin on the primary
/// display, the space AX reports the Dock in. Needs no TCC permission and is
/// safe to call off the main thread.
pub fn location() -> Option<ScreenPoint> {
    let event = CGEvent::new(None)?;
    let point = CGEvent::location(Some(&event));
    Some(ScreenPoint {
        x: point.x,
        y: point.y,
    })
}
