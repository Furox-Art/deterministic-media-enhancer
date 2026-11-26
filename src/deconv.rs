//! Bounded, candidate-PSF inverse restoration. Not unrestricted blind PSF identification.
use crate::media::*;
const KERNELS: &[(i32, i32, i32)] = &[
    (1, 0, 1),
    (0, 1, 1),
    (1, 1, 1),
    (1, -1, 1),
    (1, 0, 2),
    (0, 1, 2),
    (1, 1, 2),
    (1, -1, 2),
    (0, 0, 1),
];
fn at(p: &[i32], w: usize, h: usize, x: i32, y: i32) -> i32 {
    p[y.clamp(0, h as i32 - 1) as usize * w + x.clamp(0, w as i32 - 1) as usize]
}
fn blur(p: &[i32], w: usize, h: usize, k: (i32, i32, i32)) -> Vec<i32> {
    let (dx, dy, r) = k;
    (0..p.len())
        .map(|i| {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            if dx == 0 && dy == 0 {
                let mut sum = 0;
                for j in -1i32..=1 {
                    for a in -1i32..=1 {
                        sum += at(p, w, h, x + a, y + j) * (2 - a.abs()) * (2 - j.abs())
                    }
                }
                (sum + 8) / 16
            } else {
                let sum: i32 = (-r..=r).map(|a| at(p, w, h, x + a * dx, y + a * dy)).sum();
                (sum + r) / (2 * r + 1)
            }
        })
        .collect()
}
fn inverse(src: &[i32], w: usize, h: usize, k: (i32, i32, i32)) -> Vec<i32> {
    let mut p = src.to_vec();
    // Bounded regularized fixed-point Landweber iteration, symmetric kernel.
    // Clamp extension is used at boundaries; this is an approximate adjoint there.
    for _ in 0..6 {
        let pred = blur(&p, w, h, k);
        let residual: Vec<_> = src.iter().zip(pred).map(|(&a, b)| a - b + 256).collect();
        let correction = blur(&residual, w, h, k);
        for i in 0..p.len() {
            let delta = (3 * (correction[i] - 256) - (p[i] - src[i])) / 4;
            p[i] = (p[i] + delta).clamp((src[i] - 24).max(0), (src[i] + 24).min(255));
        }
    }
    p
}
fn stats(src: &[i32], p: &[i32], w: usize, h: usize, k: (i32, i32, i32)) -> (i64, i64, i64) {
    let pred = blur(p, w, h, k);
    let mut residual = 0i64;
    let (mut gain, mut changes, mut count) = (0i64, 0i64, 0i64);
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let i = y * w + x;
            let g = (src[i + 1] - src[i - 1]).abs() + (src[i + w] - src[i - w]).abs();
            let new = (p[i + 1] - p[i - 1]).abs() + (p[i + w] - p[i - w]).abs();
            residual += (src[i] - pred[i]).abs() as i64;
            changes += (src[i] - p[i]).abs() as i64;
            if g >= 16 {
                gain += (new - g).clamp(-32, 32) as i64;
                count += 1
            }
        }
    }
    let n = ((w - 4) * (h - 4)) as i64;
    (
        residual * 100 / n,
        gain * 100 / count.max(1),
        changes * 100 / n,
    )
}
pub fn restore(src: &[u8], w: usize, h: usize, motion: bool) -> Result<(Vec<u8>, i64)> {
    if w < 32 || h < 32 || w.checked_mul(h) != Some(src.len()) || src.len() > 2073600 {
        return err("Deconvolution requires 32x32 to 2073600 pixels");
    }
    // Estimate a PSF on a bounded central sample, with abstention on weak evidence.
    let sw = w.min(128);
    let sh = h.min(128);
    let x0 = (w - sw) / 2;
    let y0 = (h - sh) / 2;
    let a: Vec<i32> = (y0..y0 + sh)
        .flat_map(|y| src[y * w + x0..y * w + x0 + sw].iter().map(|&x| x as i32))
        .collect();
    let mut best = None;
    let mut best_score = 0;
    for (i, &k) in KERNELS.iter().enumerate() {
        if !motion && i != 8 {
            continue;
        }
        if motion && i == 8 {
            continue;
        }
        let p = inverse(&a, sw, sh, k);
        let (err, gain, change) = stats(&a, &p, sw, sh, k);
        let score = gain - 2 * err - change / 2;
        if gain >= 80 && change <= 650 && err <= 450 && score > best_score {
            best = Some((i, k));
            best_score = score
        }
    }
    let Some((i, k)) = best else {
        return Ok((src.to_vec(), 0));
    };
    let full: Vec<_> = src.iter().map(|&x| x as i32).collect();
    let p = inverse(&full, w, h, k);
    let (err, gain, change) = stats(&full, &p, w, h, k);
    if gain < 40 || change > 650 || err > 450 {
        return Ok((src.to_vec(), 0));
    }
    Ok((p.into_iter().map(|x| x as u8).collect(), i as i64 + 1))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_abstain() {
        let f = vec![128; 64 * 64];
        assert_eq!(restore(&f, 64, 64, true).unwrap(), (f, 0));
    }
    #[test]
    fn known_psf() {
        let w = 64;
        let h = 64;
        let a: Vec<i32> = (0..w * h)
            .map(|i| if i % w < 32 { 80 } else { 160 })
            .collect();
        let b = blur(&a, w, h, (1, 0, 2));
        let p = inverse(&b, w, h, (1, 0, 2));
        let sse = |p: &[i32]| {
            p.iter()
                .zip(&a)
                .map(|(x, y)| (x - y).pow(2) as i64)
                .sum::<i64>()
        };
        assert!(sse(&p) < sse(&b));
        let b: Vec<u8> = b.into_iter().map(|x| x as u8).collect();
        let r = restore(&b, w, h, true).unwrap();
        assert_eq!(r, restore(&b, w, h, true).unwrap());
        assert!(r.0.iter().zip(&b).all(|(&x, &y)| x.abs_diff(y) <= 24));
    }
}
