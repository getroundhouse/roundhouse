//! Row-major `out = x * weight^T + bias` via macOS's `Accelerate.framework`.

#[link(name = "Accelerate", kind = "framework")]
unsafe extern "C" {
    fn cblas_sgemm(
        order: i32,
        trans_a: i32,
        trans_b: i32,
        m: i32,
        n: i32,
        k: i32,
        alpha: f32,
        a: *const f32,
        lda: i32,
        b: *const f32,
        ldb: i32,
        beta: f32,
        c: *mut f32,
        ldc: i32,
    );
}

const ROW_MAJOR: i32 = 101;
const NO_TRANS: i32 = 111;
const TRANS: i32 = 112;

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
    for row in out.chunks_exact_mut(out_dim) {
        row.copy_from_slice(bias);
    }
    if in_dim == 0 {
        return;
    }
    let (Ok(m), Ok(n), Ok(k)) = (i32::try_from(rows), i32::try_from(out_dim), i32::try_from(in_dim)) else {
        panic!("matrix dimensions exceed i32");
    };
    // SAFETY: `x` holds `m * k` initialized floats, `weight` holds `n * k` initialized floats, and `out` holds
    // `m * n` initialized floats exclusively borrowed here, matching the leading dimensions `k`, `k` and `n`.
    unsafe {
        cblas_sgemm(
            ROW_MAJOR,
            NO_TRANS,
            TRANS,
            m,
            n,
            k,
            1.0,
            x.as_ptr(),
            k,
            weight.as_ptr(),
            k,
            1.0,
            out.as_mut_ptr(),
            n,
        );
    }
}
