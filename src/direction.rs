//! Conservative projected step-edge model selection for finite 2D motion PSFs.
use crate::media::*;
const NORMALS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
const MODELS: [(i32, i32, i32); 13] = [
    (0, 0, 0),
    (1, 0, 1),
    (0, 1, 1),
    (1, 1, 1),
    (1, -1, 1),
    (1, 0, 2),
    (0, 1, 2),
    (1, 1, 2),
    (1, -1, 2),
    (2, 1, 1),
    (2, -1, 1),
    (1, 2, 1),
    (1, -2, 1),
];
const R: i32 = 6;
fn predict(q: i32, step: i32, r: i32) -> i32 {
    let n = (-r..=r).filter(|&i| q + i * step >= 0).count() as i32;
    (n * 256 + r) / (2 * r + 1)
}
fn estimate(a: &[u8], w: usize, h: usize) -> Option<usize> {
    if w < 32 || h < 32 || w.checked_mul(h) != Some(a.len()) {
        return None;
    }
    let mut totals = [0i64; 13];
    let mut count = 0;
    let mut orientations = [0usize; 4];
    let mut coverage = [0usize; 16];
    // Precompute normalized step responses, including integer subpixel phase alternatives.
    let mut templates = vec![vec![vec![[0i32; 169]; 5]; 13]; 4];
    for (ni, &(nx, ny)) in NORMALS.iter().enumerate() {
        for (ki, &(dx, dy, r)) in MODELS.iter().enumerate() {
            for (pi, t) in (-2..=2).enumerate() {
                for j in -R..=R {
                    for i in -R..=R {
                        templates[ni][ki][pi][((j + R) * 13 + i + R) as usize] =
                            predict(nx * i + ny * j + t, nx * dx + ny * dy, r);
                    }
                }
            }
        }
    }
    let step = (w.max(h) / 128).max(2);
    for y in (6..h - 6).step_by(step) {
        for x in (6..w - 6).step_by(step) {
            let cell = (y * 4 / h) * 4 + x * 4 / w;
            if coverage[cell] >= 32 {
                continue;
            }
            let idx = y * w + x;
            let gx = a[idx + 1] as i32 - a[idx - 1] as i32;
            let gy = a[idx + w] as i32 - a[idx - w] as i32;
            if gx.abs() + gy.abs() < 12 {
                continue;
            }
            let ni = if gy.abs() * 2 < gx.abs() {
                0
            } else if gx.abs() * 2 < gy.abs() {
                1
            } else if gx.signum() == gy.signum() {
                2
            } else {
                3
            };
            let (nx, ny) = NORMALS[ni];
            let sign = (nx * gx + ny * gy).signum();
            let mut patch = [0i32; 169];
            let mut hist = [0usize; 256];
            for j in -R..=R {
                for i in -R..=R {
                    let v = a[(y as i32 + j) as usize * w + (x as i32 + i) as usize] as i32;
                    patch[((j + R) * 13 + i + R) as usize] = v;
                    hist[v as usize] += 1;
                }
            }
            let percentile = |n| {
                let mut c = 0;
                for (i, &v) in hist.iter().enumerate() {
                    c += v;
                    if c >= n {
                        return i as i32;
                    }
                }
                255
            };
            let lo = percentile(17);
            let hi = percentile(153);
            let contrast = hi - lo;
            if contrast < 40 || lo < 6 || hi > 249 {
                continue;
            }
            let mut norm = [0i32; 169];
            for (v, &p) in norm.iter_mut().zip(&patch) {
                *v = ((p - lo) * 256 / contrast).clamp(-32, 288);
                if sign < 0 {
                    *v = 256 - *v
                }
            }
            let mut costs = [0i64; 13];
            for ki in 0..MODELS.len() {
                costs[ki] = templates[ni][ki]
                    .iter()
                    .map(|t| {
                        norm.iter()
                            .zip(t)
                            .map(|(&a, &b)| (a - b).abs() as i64)
                            .sum::<i64>()
                    })
                    .min()
                    .unwrap();
            }
            let best = *costs.iter().min().unwrap();
            if best > 169 * 12 {
                continue;
            }
            for (t, c) in totals.iter_mut().zip(costs) {
                *t += c
            }
            count += 1;
            orientations[ni] += 1;
            coverage[cell] += 1;
            if count >= 512 {
                break;
            }
        }
        if count >= 512 {
            break;
        }
    }
    if count < 20
        || orientations.iter().filter(|&&n| n >= 8).count() < 2
        || coverage.iter().filter(|&&x| x > 0).count() < 3
    {
        return None;
    }
    let mut rank: Vec<_> = (0..MODELS.len()).collect();
    rank.sort_by_key(|&k| (totals[k], k));
    let b = rank[0];
    let runner = rank[1];
    if b == 0
        || totals[b] * 100 > totals[runner] * 80
        || totals[b] * 100 > totals[0] * 70
        || totals[runner] - totals[b] < count as i64 * 169 * 2
    {
        return None;
    }
    Some(b)
}
pub fn restore(a: &[u8], w: usize, h: usize) -> Result<(Vec<u8>, i64)> {
    if w < 32 || h < 32 || w.checked_mul(h) != Some(a.len()) || a.len() > 2073600 {
        return err("Motion PSF size unsupported");
    }
    let Some(k) = estimate(a, w, h) else {
        return Ok((a.to_vec(), 0));
    };
    let src: Vec<_> = a.iter().map(|&v| v as i32).collect();
    let mut out = crate::deconv::inverse(&src, w, h, MODELS[k]);
    // Prevent overshoot beyond local observed extrema.
    for y in 0..h {
        for x in 0..w {
            let mut lo = a[y * w + x];
            let mut hi = lo;
            for yy in y.saturating_sub(3)..=(y + 3).min(h - 1) {
                for xx in x.saturating_sub(3)..=(x + 3).min(w - 1) {
                    lo = lo.min(a[yy * w + xx]);
                    hi = hi.max(a[yy * w + xx]);
                }
            }
            out[y * w + x] = out[y * w + x].clamp(lo as i32, hi as i32);
        }
    }
    Ok((out.into_iter().map(|v| v as u8).collect(), k as i64))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let w = 160;
        let h = 160;
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as i32, (i / w) as i32);
                let q = match (x / 80, y / 80) {
                    (0, 0) => x,
                    (1, 0) => y,
                    (0, 1) => x + y,
                    _ => x - y,
                };
                if q.rem_euclid(40) < 20 {
                    48
                } else {
                    208
                }
            })
            .collect()
    }
    fn synth(a: &[u8], w: usize, h: usize, k: (i32, i32, i32)) -> Vec<u8> {
        let (dx, dy, r) = k;
        (0..a.len())
            .map(|i| {
                let (x, y) = ((i % w) as i32, (i / w) as i32);
                let s: i32 = (-r..=r)
                    .map(|t| {
                        a[(y + t * dy).clamp(0, h as i32 - 1) as usize * w
                            + (x + t * dx).clamp(0, w as i32 - 1) as usize]
                            as i32
                    })
                    .sum();
                ((s + r) / (2 * r + 1)) as u8
            })
            .collect()
    }
    #[test]
    fn directional_models() {
        let a = fixture();
        let w = 160;
        let h = 160;
        assert_eq!(estimate(&a, w, h), None);
        assert_eq!(estimate(&vec![128; w * h], w, h), None);
        for k in [5, 7, 8, 9, 10, 11, 12] {
            let b = synth(&a, w, h, MODELS[k]);
            let guess = estimate(&b, w, h);
            println!("PSF {k}: {guess:?}");
            assert_eq!(guess, Some(k));
            let (c, id) = restore(&b, w, h).unwrap();
            assert_eq!(id, k as i64);
            let sse = |x: &[u8]| {
                x.iter()
                    .zip(&a)
                    .map(|(&u, &v)| (u as i32 - v as i32).pow(2) as u64)
                    .sum::<u64>()
            };
            println!("{} -> {}", sse(&b), sse(&c));
            assert!(sse(&c) < sse(&b));
            assert_eq!(c, restore(&b, w, h).unwrap().0);
        }
    }
}
