use super::{macos, portable};

#[test]
fn accelerate_and_portable_agree_across_multiple_rows() {
    let x = [0.5_f32, -1.25, 2.0, 0.75, -0.5, 1.5];
    let weight = [
        1.0_f32, 0.5, -0.25, // out 0
        -2.0, 1.5, 0.75, // out 1
        0.25, -0.5, 1.25, // out 2
        3.0, -1.0, 0.5, // out 3
    ];
    let bias = [0.1_f32, -0.2, 0.3, -0.4];
    let mut accel = [0.0_f32; 8];
    let mut rust = [0.0_f32; 8];

    macos::linear_rows(&x, 2, &weight, &bias, &mut accel);
    portable::linear_rows(&x, 2, &weight, &bias, &mut rust);

    for (a, b) in accel.iter().zip(rust.iter()) {
        assert!((a - b).abs() < 1e-5, "{accel:?} vs {rust:?}");
    }
}
