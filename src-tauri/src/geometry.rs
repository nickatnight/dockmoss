use serde::Serialize;

/// A rect in global points with a top-left origin on the primary display,
/// as Accessibility reports it. Tauri's logical window positions on macOS
/// use the same space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub fn is_finite(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub physical_x: i32,
    pub physical_y: i32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
}

/// A rect in the overlay webview's logical pixels, with a top-left origin
/// at the overlay's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Display {
    pub fn frame(self) -> ScreenRect {
        ScreenRect {
            x: f64::from(self.physical_x) / self.scale_factor,
            y: f64::from(self.physical_y) / self.scale_factor,
            width: f64::from(self.physical_width) / self.scale_factor,
            height: f64::from(self.physical_height) / self.scale_factor,
        }
    }
}

/// Moves `rect` into the coordinates of an overlay that covers `display`.
///
/// Returns `None` when `rect` has no area inside the display, as for an
/// auto-hidden Dock or a Dock on another display, or when any of its values
/// is NaN or infinite. A rect that is only partly inside is kept whole, so a
/// half-shown icon keeps its real size.
pub fn to_overlay_local(rect: ScreenRect, display: Display) -> Option<LogicalRect> {
    if !rect.is_finite() {
        return None;
    }
    let frame = display.frame();
    let overlap_width = (rect.x + rect.width).min(frame.x + frame.width) - rect.x.max(frame.x);
    let overlap_height = (rect.y + rect.height).min(frame.y + frame.height) - rect.y.max(frame.y);
    (overlap_width > 0.0 && overlap_height > 0.0).then_some(LogicalRect {
        x: rect.x - frame.x,
        y: rect.y - frame.y,
        width: rect.width,
        height: rect.height,
    })
}

#[cfg(test)]
mod tests;
