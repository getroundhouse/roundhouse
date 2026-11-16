//! Portable row-major `out = x * weight^T + bias` for platforms without Accelerate, and to check Accelerate against.

/// Computes `out[r] = weight * x[r] + bias` for each of `rows` rows of length `in_dim`.
pub(in super::super) fn linear_rows(x: &[f32], rows: usize, weight: &[f32], bias: &[f32], out: &mut [f32]) {
    let out_dim = bias.len();
    if rows == 0 || out_dim == 0 {
        return;
    }
    let in_dim = weight.len() / out_dim;
    assert_eq!(weight.len(), out_dim * in_dim);
    assert_eq!(x.len(), rows * in_dim);
    assert_eq!(out.len(), rows * out_dim);
    for (x_row, out_row) in x.chunks_exact(in_dim).zip(out.chunks_exact_mut(out_dim)) {
        for ((dest, &b), w_row) in out_row.iter_mut().zip(bias.iter()).zip(weight.chunks_exact(in_dim)) {
            let mut sum = b;
            for (&xv, &wv) in x_row.iter().zip(w_row.iter()) {
                sum += xv * wv;
            }
            *dest = sum;
        }
    }
}
