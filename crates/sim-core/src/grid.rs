//! The field raster and fast spatial fields.
//!
//! Spatial interactions (job accessibility, market potential, pollution,
//! amenities) are computed as convolutions of a source raster with an
//! exponential distance-decay kernel `a^(|dx|+|dy|)`. Because the kernel is
//! separable and exponential it can be evaluated with a forward/backward
//! recursive filter in O(cells), independent of the decay length. Parcels
//! live on an irregular mesh ([`crate::mesh`]); it splats parcel values onto
//! this raster and samples the blurred result back.

#[derive(Clone, Copy, Debug)]
pub struct Dims {
    pub w: usize,
    pub h: usize,
}

impl Dims {
    #[inline]
    pub fn len(&self) -> usize {
        self.w * self.h
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }
    #[inline]
    pub fn xy(&self, i: usize) -> (usize, usize) {
        (i % self.w, i / self.w)
    }
    #[inline]
    pub fn manhattan(&self, a: usize, b: usize) -> f32 {
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        (ax.abs_diff(bx) + ay.abs_diff(by)) as f32
    }
}

/// out[i] = Σ_j src[j] · a^(|dx|+|dy|), with a = exp(-1/lambda).
pub fn exp_blur(src: &[f32], dims: Dims, lambda: f32) -> Vec<f32> {
    let mut out = src.to_vec();
    exp_blur_in_place(&mut out, dims, lambda);
    out
}

pub fn exp_blur_in_place(buf: &mut [f32], dims: Dims, lambda: f32) {
    let a = (-1.0 / lambda.max(0.05)).exp();
    let Dims { w, h } = dims;
    let mut fwd = vec![0f32; w.max(h)];
    // rows
    for y in 0..h {
        let row = &mut buf[y * w..(y + 1) * w];
        two_sided(row, &mut fwd[..w], a);
    }
    // columns
    let mut col = vec![0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = buf[y * w + x];
        }
        two_sided(&mut col, &mut fwd[..h], a);
        for y in 0..h {
            buf[y * w + x] = col[y];
        }
    }
}

/// In-place 1D two-sided exponential filter: y_i = Σ_j x_j a^|i-j|.
fn two_sided(x: &mut [f32], scratch: &mut [f32], a: f32) {
    let n = x.len();
    if n == 0 {
        return;
    }
    // forward pass
    let mut acc = 0.0f32;
    for i in 0..n {
        acc = x[i] + a * acc;
        scratch[i] = acc;
    }
    // backward pass, excluding the centre (already counted)
    let mut back = 0.0f32;
    for i in (0..n).rev() {
        let xi = x[i];
        x[i] = scratch[i] + a * back;
        back = xi + a * back;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_matches_bruteforce() {
        let dims = Dims { w: 9, h: 7 };
        let mut src = vec![0f32; dims.len()];
        src[dims.idx(2, 3)] = 1.0;
        src[dims.idx(7, 1)] = 2.0;
        let lambda = 2.5;
        let a = (-1.0f32 / lambda).exp();
        let fast = exp_blur(&src, dims, lambda);
        for i in 0..dims.len() {
            let mut s = 0.0;
            for j in 0..dims.len() {
                s += src[j] * a.powf(dims.manhattan(i, j));
            }
            assert!((s - fast[i]).abs() < 1e-4, "cell {i}: {s} vs {}", fast[i]);
        }
    }
}
