pub mod ax;
#[deny(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "polling path: release builds abort on panic"
)]
pub mod cursor;
#[deny(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "polling path: release builds abort on panic"
)]
pub mod dock;
pub mod overlay;
pub mod screen;
pub mod workspace;

pub use objc2::MainThreadMarker;
