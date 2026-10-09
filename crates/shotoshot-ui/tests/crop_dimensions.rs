use shotoshot_core::{Point, Rect};
use shotoshot_ui::crop::{dimension, resize_region};

#[test]
fn dimensions_evaluate_arithmetic_with_precedence_and_parentheses() {
    for (expression, expected) in [
        ("800+20", 820),
        ("1920/2", 960),
        ("(100 + 20) * 3", 360),
        ("100-20*2", 60),
        ("+80", 80),
        ("12.5*2", 25),
        ("12,5*2", 25),
    ] {
        assert_eq!(dimension(expression), Ok(expected), "{expression}");
    }
}

#[test]
fn dimensions_reject_invalid_noninteger_and_unbounded_input() {
    for expression in [
        "",
        "1/0",
        "0",
        "-10",
        "1",
        "3/2",
        "NaN",
        "2px",
        "10+",
        "(20",
        "20)",
        "2**3",
        "999999999999999999999",
        "2 3",
    ] {
        assert!(dimension(expression).is_err(), "{expression}");
    }
    assert!(dimension(&"(".repeat(1000)).is_err());
}

#[test]
fn resizing_keeps_origin_and_rejects_overflow_without_changing_region() {
    let region = Rect {
        min: Point { x: 50., y: 30. },
        max: Point { x: 100., y: 80. },
    };
    let resized = resize_region(region, "100+20", "80/2", 200, 100).unwrap();
    assert_eq!(resized.min, region.min);
    assert_eq!(resized.max, Point { x: 170., y: 70. });
    assert!(resize_region(region, "151", "40", 200, 100).is_err());
    assert!(resize_region(region, "120", "71", 200, 100).is_err());
}
