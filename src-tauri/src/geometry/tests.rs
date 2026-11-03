use super::{Display, LogicalRect, ScreenRect, to_overlay_local};

fn display(
    physical_x: i32,
    physical_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
) -> Display {
    Display {
        physical_x,
        physical_y,
        physical_width,
        physical_height,
        scale_factor,
    }
}

fn screen(x: f64, y: f64, width: f64, height: f64) -> ScreenRect {
    ScreenRect {
        x,
        y,
        width,
        height,
    }
}

fn logical(x: f64, y: f64, width: f64, height: f64) -> LogicalRect {
    LogicalRect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn frame_is_the_display_in_global_points() {
    let cases = [
        (
            "primary at 1x",
            display(0, 0, 1920, 1080, 1.0),
            screen(0.0, 0.0, 1920.0, 1080.0),
        ),
        (
            "primary at 2x",
            display(0, 0, 3024, 1964, 2.0),
            screen(0.0, 0.0, 1512.0, 982.0),
        ),
        (
            "secondary at 1x left of and above the primary",
            display(-1920, -200, 1920, 1080, 1.0),
            screen(-1920.0, -200.0, 1920.0, 1080.0),
        ),
        (
            "secondary at 2x right of a 1x primary",
            display(3840, 0, 5120, 2880, 2.0),
            screen(1920.0, 0.0, 2560.0, 1440.0),
        ),
    ];

    for (case, display, frame) in cases {
        assert_eq!(display.frame(), frame, "{case}");
    }
}

#[test]
fn dock_icon_on_the_overlay_display_becomes_overlay_local() {
    let primary_1x = display(0, 0, 1920, 1080, 1.0);
    let primary_2x = display(0, 0, 3024, 1964, 2.0);
    let secondary_negative = display(-1920, -200, 1920, 1080, 1.0);
    let secondary_1x_beside_2x = display(1512, 0, 1920, 1080, 1.0);
    let secondary_2x_beside_1x = display(3840, 0, 5120, 2880, 2.0);
    let cases = [
        (
            "primary at 1x",
            primary_1x,
            screen(900.0, 1010.0, 64.0, 64.0),
            logical(900.0, 1010.0, 64.0, 64.0),
        ),
        (
            "primary at 2x keeps AX points as logical pixels",
            primary_2x,
            screen(700.5, 920.0, 56.0, 56.0),
            logical(700.5, 920.0, 56.0, 56.0),
        ),
        (
            "secondary with a negative origin",
            secondary_negative,
            screen(-1000.0, 820.0, 64.0, 64.0),
            logical(920.0, 1020.0, 64.0, 64.0),
        ),
        (
            "1x secondary beside a 2x primary",
            secondary_1x_beside_2x,
            screen(2000.0, 1000.0, 64.0, 64.0),
            logical(488.0, 1000.0, 64.0, 64.0),
        ),
        (
            "2x secondary beside a 1x primary",
            secondary_2x_beside_1x,
            screen(3000.0, 1360.0, 64.0, 64.0),
            logical(1080.0, 1360.0, 64.0, 64.0),
        ),
    ];

    for (case, display, rect, expected) in cases {
        assert_eq!(to_overlay_local(rect, display), Some(expected), "{case}");
    }
}

#[test]
fn rect_partly_outside_the_frame_is_kept_unclipped() {
    let primary_2x = display(0, 0, 3024, 1964, 2.0);

    let local = to_overlay_local(screen(-20.0, 950.0, 64.0, 64.0), primary_2x);

    assert_eq!(local, Some(logical(-20.0, 950.0, 64.0, 64.0)));
}

#[test]
fn rect_with_no_area_inside_the_frame_is_dropped() {
    let primary_2x = display(0, 0, 3024, 1964, 2.0);
    let secondary_1x_beside_2x = display(1512, 0, 1920, 1080, 1.0);
    let cases = [
        (
            "below the bottom edge, as an auto-hidden Dock",
            primary_2x,
            screen(700.0, 990.0, 56.0, 56.0),
        ),
        (
            "touching the bottom edge only",
            primary_2x,
            screen(700.0, 982.0, 56.0, 56.0),
        ),
        (
            "on the other display",
            primary_2x,
            screen(2000.0, 1000.0, 64.0, 64.0),
        ),
        (
            "on the 2x primary, read against the 1x secondary",
            secondary_1x_beside_2x,
            screen(700.0, 920.0, 56.0, 56.0),
        ),
        (
            "zero-sized inside the frame",
            primary_2x,
            screen(700.0, 920.0, 0.0, 0.0),
        ),
    ];

    for (case, display, rect) in cases {
        assert_eq!(to_overlay_local(rect, display), None, "{case}");
    }
}

#[test]
fn rect_with_a_non_finite_value_is_dropped() {
    let primary_2x = display(0, 0, 3024, 1964, 2.0);
    let icon = screen(700.0, 920.0, 56.0, 56.0);
    let bad_values = [
        ("NaN", f64::NAN),
        ("+inf", f64::INFINITY),
        ("-inf", f64::NEG_INFINITY),
    ];

    for (name, bad) in bad_values {
        let cases = [
            ("x", ScreenRect { x: bad, ..icon }),
            ("y", ScreenRect { y: bad, ..icon }),
            ("width", ScreenRect { width: bad, ..icon }),
            (
                "height",
                ScreenRect {
                    height: bad,
                    ..icon
                },
            ),
        ];
        for (field, rect) in cases {
            assert_eq!(to_overlay_local(rect, primary_2x), None, "{field} = {name}");
        }
    }
}

#[test]
fn infinite_rect_spanning_the_display_is_dropped() {
    let primary_2x = display(0, 0, 3024, 1964, 2.0);

    let local = to_overlay_local(
        screen(f64::NEG_INFINITY, 920.0, f64::INFINITY, 56.0),
        primary_2x,
    );

    assert_eq!(local, None);
}
