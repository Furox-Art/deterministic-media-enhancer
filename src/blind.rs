const K: [&[i32]; 6] = [
    &[1],
    &[1, 1, 1],
    &[1, 1, 1, 1, 1],
    &[1, 1, 1, 1, 1, 1, 1],
    &[1, 2, 1],
    &[1, 4, 6, 4, 1],
];
fn rd(x: i32, d: i32) -> i32 {
    if x < 0 {
        -((-x + d / 2) / d)
    } else {
        (x + d / 2) / d
    }
}
fn refl(i: isize, n: usize) -> usize {
    let n = n as isize;
    let j = i.rem_euclid(2 * n);
    if j < n {
        j as usize
    } else {
        (2 * n - 1 - j) as usize
    }
}
fn axis(a: &[i32], w: usize, h: usize, k: usize, hor: bool) -> Vec<i32> {
    let q = K[k];
    if k == 0 {
        return a.to_vec();
    }
    let r = (q.len() / 2) as isize;
    let den = q.iter().sum();
    let mut out = vec![0; a.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0;
            for (j, &v) in q.iter().enumerate() {
                let o = j as isize - r;
                let i = if hor {
                    y * w + refl(x as isize + o, w)
                } else {
                    refl(y as isize + o, h) * w + x
                };
                s += a[i] * v;
            }
            out[y * w + x] = rd(s, den);
        }
    }
    out
}
fn blur(a: &[i32], w: usize, h: usize, kx: usize, ky: usize) -> Vec<i32> {
    axis(&axis(a, w, h, kx, true), w, h, ky, false)
}
fn score(p: &[i32; 13], lo: i32, hi: i32, k: usize) -> i32 {
    let q = K[k];
    let den = q.iter().sum::<i32>();
    let r = (q.len() / 2) as isize;
    (0..13)
        .map(|i| {
            let cum = q
                .iter()
                .enumerate()
                .filter(|(j, _)| i as isize + *j as isize - r >= 6)
                .map(|(_, v)| v)
                .sum::<i32>();
            let pred = lo + rd((hi - lo) * cum, den);
            rd((p[i] - pred).abs() * 256, (hi - lo).abs())
        })
        .sum()
}
fn est_axis(a: &[u8], w: usize, h: usize, hor: bool) -> Option<(usize, u64, u64, usize)> {
    let (n, m) = if hor { (w, h) } else { (h, w) };
    if n < 13 || m < 8 {
        return None;
    }
    let mut totals = [0u64; 6];
    let mut wins = [0usize; 6];
    let mut count = 0;
    let mut coverage = [false; 16];
    for j in (0..m).step_by((m / 128).max(1)) {
        for i in (6..n - 6).step_by((n / 512).max(1)) {
            let p: [i32; 13] = std::array::from_fn(|z| {
                let t = i + z - 6;
                a[if hor { j * w + t } else { t * w + j }] as i32
            });
            let l = &p[..3];
            let r = &p[10..];
            if l.iter().max()? - l.iter().min()? > 6 || r.iter().max()? - r.iter().min()? > 6 {
                continue;
            }
            let lo = rd(l.iter().sum(), 3);
            let hi = rd(r.iter().sum(), 3);
            if (hi - lo).abs() < 40 || lo.min(hi) < 8 || lo.max(hi) > 247 {
                continue;
            }
            let sign = (hi - lo).signum();
            let mid = lo + hi;
            if !(sign * (2 * p[5] - mid) < 0 && sign * (2 * p[6] - mid) >= 0) {
                continue;
            }
            if p.windows(2).any(|v| sign * (v[1] - v[0]) < -4) {
                continue;
            }
            let sc: [i32; 6] = std::array::from_fn(|k| score(&p, lo, hi, k));
            let b = (0..6).min_by_key(|&k| (sc[k], k))?;
            if sc[b] > 55 {
                continue;
            }
            for k in 0..6 {
                totals[k] += sc[k] as u64;
            }
            wins[b] += 1;
            count += 1;
            coverage[(j * 4 / m) * 4 + i * 4 / n] = true;
        }
    }
    if count < 16 || coverage.iter().filter(|&&b| b).count() < 3 {
        return None;
    }
    let mut rank = [0, 1, 2, 3, 4, 5];
    rank.sort_by_key(|&k| (totals[k], k));
    let b = rank[0];
    let r = rank[1];
    if wins[b] * 100 < count * 75
        || totals[b] > count as u64 * 40
        || totals[b] * 100 > totals[r] * 75
        || totals[r] - totals[b] < (count as u64) * 5
    {
        return None;
    }
    Some((b, totals[b], totals[r], count))
}
fn estimate(a: &[u8], w: usize, h: usize) -> Option<(usize, usize)> {
    let (x, _, _, _) = est_axis(a, w, h, true)?;
    let (y, _, _, _) = est_axis(a, w, h, false)?;
    if x == 0 && y <= 3 || y == 0 && x <= 3 || x == y && x >= 4 {
        Some((x, y))
    } else {
        None
    }
}
fn restore(a: &[u8], w: usize, h: usize, kx: usize, ky: usize) -> Vec<u8> {
    assert!(w > 0 && h > 0 && w.checked_mul(h) == Some(a.len()) && a.len() <= 1920 * 1080);
    if kx == 0 && ky == 0 {
        return a.to_vec();
    }
    let y: Vec<i32> = a.iter().map(|&p| p as i32 * 256).collect();
    let mut x = y.clone();
    for _ in 0..8 {
        let b = blur(&x, w, h, kx, ky);
        let e: Vec<i32> = y.iter().zip(&b).map(|(a, b)| a - b).collect();
        let g = axis(&axis(&e, w, h, ky, false), w, h, kx, true);
        for i in 0..x.len() {
            let step = rd(g[i] - rd(x[i] - y[i], 8), 2);
            x[i] = (x[i] + step).clamp((y[i] - 16 * 256).max(0), (y[i] + 16 * 256).min(255 * 256));
        }
    }
    let mut out = a.to_vec();
    for j in 0..h {
        for i in 0..w {
            let pos = j * w + i;
            let (mut lo, mut hi) = (a[pos], a[pos]);
            for dj in -3..=3 {
                for di in -3..=3 {
                    let z = a[refl(j as isize + dj, h) * w + refl(i as isize + di, w)];
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
            }
            if hi - lo >= 12 {
                out[pos] = rd(x[pos], 256).clamp(lo as i32, hi as i32) as u8;
            }
        }
    }
    out
}
#[cfg(test)]
fn synth(a: &[u8], w: usize, h: usize, kx: usize, ky: usize) -> Vec<u8> {
    blur(
        &a.iter().map(|&p| p as i32 * 256).collect::<Vec<_>>(),
        w,
        h,
        kx,
        ky,
    )
    .into_iter()
    .map(|p| rd(p, 256) as u8)
    .collect()
}
#[cfg(test)]
fn sse(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (x as i64 - y as i64).pow(2) as u64)
        .sum()
}
#[test]
fn step_edge_regression() {
    let (w, h) = (128, 128);
    let sharp: Vec<u8> = (0..w * h)
        .map(|i| {
            let x = i % w;
            let y = i / w;
            if ((x / 16) + (y / 16)) % 2 == 0 {
                48
            } else {
                208
            }
        })
        .collect();
    for (kx, ky) in [
        (0, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (0, 2),
        (4, 4),
        (5, 5),
        (2, 2),
    ] {
        let obs = synth(&sharp, w, h, kx, ky);
        let guess = estimate(&obs, w, h);
        let out = match guess {
            Some((x, y)) => restore(&obs, w, h, x, y),
            None => obs.clone(),
        };
        println!(
            "psf=({kx},{ky}) estimate={guess:?} x={:?} y={:?} sse={} -> {} max_delta={}",
            est_axis(&obs, w, h, true),
            est_axis(&obs, w, h, false),
            sse(&obs, &sharp),
            sse(&out, &sharp),
            obs.iter()
                .zip(&out)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap()
        );
        assert!(out.iter().zip(&obs).all(|(&a, &b)| a.abs_diff(b) <= 16));
        assert_eq!(
            out,
            match guess {
                Some((x, y)) => restore(&obs, w, h, x, y),
                None => obs.clone(),
            }
        );
        if [(1, 0), (2, 0), (3, 0), (0, 2), (4, 4), (5, 5)].contains(&(kx, ky)) {
            assert_eq!(guess, Some((kx, ky)));
            assert!(sse(&out, &sharp) < sse(&obs, &sharp));
        }
        if (kx, ky) == (0, 0) {
            assert_eq!(out, obs)
        }
        if (kx, ky) == (2, 2) {
            assert_eq!(guess, None)
        }
    }
    let flat = vec![123; w * h];
    assert_eq!(estimate(&flat, w, h), None);
    assert_eq!(restore(&flat, w, h, 2, 0), flat);
    let ramp: Vec<u8> = (0..w * h).map(|i| (32 + i % w) as u8).collect();
    assert_eq!(estimate(&ramp, w, h), None);
    let mut seed = 9u32;
    let noisy: Vec<u8> = flat
        .iter()
        .map(|&p| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (p as i32 + ((seed >> 24) % 41) as i32 - 20) as u8
        })
        .collect();
    assert_eq!(estimate(&noisy, w, h), None);
    println!("PASS flat/ramp/noise abstention; determinism, max correction 16");
}

pub fn blind(a: &[u8], w: usize, h: usize) -> crate::media::Result<(Vec<u8>, i64)> {
    if w < 32 || h < 32 || w.checked_mul(h) != Some(a.len()) || a.len() > 2073600 {
        return crate::media::err("Blind restoration size unsupported");
    }
    let Some((x, y)) = estimate(a, w, h) else {
        return Ok((a.to_vec(), 0));
    };
    if x == 0 && y == 0 {
        return Ok((a.to_vec(), 0));
    }
    let id = if y == 0 {
        x
    } else if x == 0 {
        3 + y
    } else {
        3 + x
    };
    Ok((restore(a, w, h, x, y), id as i64))
}
