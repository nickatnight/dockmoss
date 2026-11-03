pub mod ax;
pub mod cursor;
pub mod dock;
pub mod overlay;
pub mod screen;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the usage thread runs the first-run read once slice 8 wires it"
    )
)]
pub mod spotlight;
pub mod workspace;

pub use objc2::MainThreadMarker;
