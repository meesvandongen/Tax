//! Grid geometry helpers and fast spatial fields.
//!
//! Spatial interactions (job accessibility, market potential, pollution,
//! amenities) are computed as convolutions of a source grid with an
//! exponential distance-decay kernel `a^(|dx|+|dy|)`. Because the kernel is
//! separable and exponential it can be evaluated with a forward/backward
//! recursive filter in O(cells), independent of the decay length. The
//! Manhattan metric matches the grid street network commuters use.

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
    /// 4-neighbourhood.
    pub fn neighbors4(&self, i: usize) -> impl Iterator<Item = usize> {
        let (x, y) = self.xy(i);
        let w = self.w;
        let h = self.h;
        let mut out = [usize::MAX; 4];
        if x > 0 {
            out[0] = i - 1;
        }
        if x + 1 < w {
            out[1] = i + 1;
        }
        if y > 0 {
            out[2] = i - w;
        }
        if y + 1 < h {
            out[3] = i + w;
        }
        out.into_iter().filter(|&n| n != usize::MAX)
    }
    /// All cells within Chebyshev radius r (excluding i).
    pub fn ring(&self, i: usize, r: usize) -> Vec<usize> {
        let (x, y) = self.xy(i);
        let mut v = Vec::with_capacity((2 * r + 1) * (2 * r + 1));
        let x0 = x.saturating_sub(r);
        let y0 = y.saturating_sub(r);
        let x1 = (x + r).min(self.w - 1);
        let y1 = (y + r).min(self.h - 1);
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                let j = self.idx(xx, yy);
                if j != i {
                    v.push(j);
                }
            }
        }
        v
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

/// Weighted local mean: blur(values·weights) / blur(weights). Cells with
/// negligible weight fall back to `fallback`.
pub fn smoothed_mean(values: &[f32], weights: &[f32], dims: Dims, lambda: f32, fallback: f32) -> Vec<f32> {
    let mut num: Vec<f32> = values.iter().zip(weights).map(|(v, w)| v * w).collect();
    let mut den = weights.to_vec();
    exp_blur_in_place(&mut num, dims, lambda);
    exp_blur_in_place(&mut den, dims, lambda);
    num.iter()
        .zip(&den)
        .map(|(n, d)| if *d > 1e-3 { n / d } else { fallback })
        .collect()
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
