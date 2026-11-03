pub mod ax;
#[expect(
    dead_code,
    reason = "the dock-tracker thread reads it from the next commit"
)]
pub mod cursor;
pub mod dock;
pub mod overlay;
pub mod workspace;

pub use objc2::MainThreadMarker;
