//! Two-stage block matching and collaborative filtering using exact integer
//! Walsh-Hadamard transforms. A bounded BM3D variant, not the reference DCT implementation.
use crate::{
    media::*,
    video::{self, Timeline},
};
use std::{
    io::{Read, Write},
    process::Stdio,
};

const B: usize = 8;
const P: usize = B * B;
const GROUP: usize = 4;
const LIMIT: usize = 1920 * 1080;

fn round_div(x: i64, d: i64) -> i64 {
    if x < 0 {
        -((-x + d / 2) / d)
    } else {
        (x + d / 2) / d
    }
}

// Robust diagonal residual MAD, lower quartile over 32x32 tiles.
// For independent Gaussian noise the median absolute residual is ~1.349 sigma.
fn estimate(src: &[u8], w: usize, h: usize) -> i64 {
    let mut estimates = Vec::new();
    for y in (0..h).step_by(32) {
        for x in (0..w).step_by(32) {
            let mut hist = [0usize; 511];
            let mut n = 0;
            for j in (y..(y + 32).min(h).saturating_sub(1)).step_by(2) {
                for i in (x..(x + 32).min(w).saturating_sub(1)).step_by(2) {
                    let k = j * w + i;
                    let d = (src[k] as i32 - src[k + 1] as i32 - src[k + w] as i32
                        + src[k + w + 1] as i32)
                        .unsigned_abs() as usize;
                    hist[d] += 1;
                    n += 1;
                }
            }
            if n < 32 {
                continue;
            }
            let mut sum = 0;
            let mut med = 0;
            for (i, c) in hist.iter().enumerate() {
                sum += c;
                if sum * 2 >= n {
                    med = i;
                    break;
                }
            }
            estimates.push((med as i64 * 1000 + 674) / 1349);
        }
    }
    if estimates.is_empty() {
        return 0;
    }
    estimates.sort_unstable();
    estimates[estimates.len() / 4].clamp(0, 25)
}

fn hadamard(v: &mut [i64]) {
    let mut s = 1;
    while s < v.len() {
        for i in (0..v.len()).step_by(2 * s) {
            for j in 0..s {
                let a = v[i + j];
                let b = v[i + j + s];
                v[i + j] = a + b;
                v[i + j + s] = a - b;
            }
        }
        s *= 2;
    }
}

fn starts(n: usize) -> Vec<usize> {
    let mut v: Vec<_> = (0..=n - B).step_by(4).collect();
    if v.last() != Some(&(n - B)) {
        v.push(n - B)
    }
    v
}

fn matching(
    guide: &[u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    sigma: i64,
    final_pass: bool,
) -> Vec<(usize, usize)> {
    let mut ranked = Vec::with_capacity(25);
    let limit = P as i64 * sigma * sigma * if final_pass { 3 } else { 5 };
    // Fixed grid and lexicographic tie breaks. Always retain the reference block.
    for dy in [-8isize, -4, 0, 4, 8] {
        for dx in [-8isize, -4, 0, 4, 8] {
            let xx = (x as isize + dx).clamp(0, (w - B) as isize) as usize;
            let yy = (y as isize + dy).clamp(0, (h - B) as isize) as usize;
            if (xx, yy) == (x, y) || ranked.iter().any(|&(_, a, b)| a == xx && b == yy) {
                continue;
            }
            let mut d = 0i64;
            for j in 0..B {
                for i in 0..B {
                    let z = guide[(y + j) * w + x + i] as i64 - guide[(yy + j) * w + xx + i] as i64;
                    d += z * z;
                }
            }
            if d <= limit {
                ranked.push((d, xx, yy))
            }
        }
    }
    ranked.sort_unstable();
    let mut group = vec![(x, y)];
    let n = (ranked.len() + 1).min(GROUP);
    let n = if n >= 4 {
        4
    } else if n >= 2 {
        2
    } else {
        1
    };
    group.extend(ranked.into_iter().take(n - 1).map(|(_, x, y)| (x, y)));
    group
}

fn transform(src: &[u8], w: usize, group: &[(usize, usize)]) -> Vec<i64> {
    let mut v = Vec::with_capacity(group.len() * P);
    for &(x, y) in group {
        for j in 0..B {
            for i in 0..B {
                v.push(src[(y + j) * w + x + i] as i64)
            }
        }
    }
    // Flattened separable spatial and group transform, self-inverse up to N.
    hadamard(&mut v);
    v
}

fn pass(src: &[u8], guide: &[u8], w: usize, h: usize, sigma: i64, final_pass: bool) -> Vec<u8> {
    let mut accum = vec![0i64; w * h];
    let mut weights = vec![0i64; w * h];
    let xs = starts(w);
    let ys = starts(h);
    for &y in &ys {
        for &x in &xs {
            let group = matching(guide, w, h, x, y, sigma, final_pass);
            let n = (group.len() * P) as i64;
            let var = n * sigma * sigma;
            let mut v = transform(src, w, &group);
            let weight = if final_pass {
                let pilot = transform(guide, w, &group);
                let mut gain2 = 0i64;
                for (z, b) in v.iter_mut().zip(pilot) {
                    let power = b * b;
                    let gain = (power * 1024 + (power + var) / 2) / (power + var);
                    *z = round_div(*z * gain, 1024);
                    gain2 += gain * gain;
                }
                (n * 1024 * 1024 * 16 / gain2.max(1)).clamp(1, 1024)
            } else {
                let mut kept = 0;
                for z in &mut v {
                    if *z * *z * 100 < 729 * var {
                        *z = 0
                    } else {
                        kept += 1
                    }
                }
                (n * 16 / kept.max(1)).clamp(1, 1024)
            };
            hadamard(&mut v);
            for (k, &(xx, yy)) in group.iter().enumerate() {
                for j in 0..B {
                    for i in 0..B {
                        let idx = (yy + j) * w + xx + i;
                        // Separable integer taper avoids visible patch boundaries.
                        let taper = (i + 1).min(B - i) * (j + 1).min(B - j);
                        let a = weight * taper as i64;
                        accum[idx] += round_div(v[k * P + j * B + i], n).clamp(0, 255) * a;
                        weights[idx] += a;
                    }
                }
            }
        }
    }
    accum
        .iter()
        .zip(weights)
        .zip(src)
        .map(|((&a, b), &s)| {
            if b == 0 {
                s
            } else {
                round_div(a, b).clamp(0, 255) as u8
            }
        })
        .collect()
}
pub fn restore(src: &[u8], w: usize, h: usize) -> Result<(Vec<u8>, i64)> {
    if w < 32 || h < 32 || w.checked_mul(h) != Some(src.len()) || src.len() > LIMIT {
        return err("Native BM3D requires 32x32 minimum and at most 2073600 source pixels");
    }
    let sigma = estimate(src, w, h);
    if sigma < 2 {
        return Ok((src.to_vec(), sigma));
    }
    let basic = pass(src, src, w, h, sigma, false);
    Ok((pass(src, &basic, w, h, sigma, true), sigma))
}

pub fn encode_native(
    path: &str,
    out: &str,
    w: usize,
    h: usize,
    t: Option<&Timeline>,
    scale: usize,
    fps: i64,
    preset: &str,
) -> Result<()> {
    let n = w.checked_mul(h).ok_or("Pixel count overflow")?;
    if w < 32 || h < 32 || n > LIMIT {
        return err("Native BM3D requires 32x32 minimum and at most 2073600 source pixels");
    }
    let rate = if let Some(t) = t {
        if !video::cfr(t) || t.pts[0].abs() > 2000 {
            return err("Native BM3D video requires near-zero-start CFR");
        }
        video::native_rate(path, t)?
    } else {
        "1".into()
    };
    let mut enc = ff();
    enc.arg("-n").args([
        "-f",
        "rawvideo",
        "-pixel_format",
        "yuv444p",
        "-video_size",
        &format!("{w}x{h}"),
        "-framerate",
        &rate,
        "-i",
        "pipe:0",
    ]);
    if t.is_some() {
        enc.args(["-threads", "1"]);
        input(&mut enc, path)?
    }
    enc.args(["-map", "0:v:0"]);
    if t.is_some() {
        enc.args(["-map", "1:a?"]);
    }
    let mut chain = resize(scale);
    if fps > 0 {
        chain = format!(
            "{},{}",
            video::interpolate(t.ok_or("Video required")?, fps)?,
            chain
        )
    }
    enc.args(["-vf", &chain]);
    encoding(&mut enc, out, t.is_none());
    let mut writer = ChildGuard(enc.stdin(Stdio::piped()).spawn()?);
    let mut dst = writer.0.stdin.take().ok_or("Missing encoder stdin")?;
    let mut dec = ff();
    if t.is_some() {
        dec.arg("-noautorotate");
    }
    input(&mut dec, path)?;
    dec.args([
        "-map",
        "0:v:0",
        "-an",
        "-filter_threads",
        "1",
        "-threads",
        "1",
        "-pix_fmt",
        "yuv444p",
        "-fps_mode",
        "passthrough",
    ]);
    if t.is_none() {
        dec.args(["-frames:v", "1"]);
    }
    dec.args(["-f", "rawvideo", "pipe:1"]);
    let mut reader = ChildGuard(dec.stdout(Stdio::piped()).spawn()?);
    let mut src = reader.0.stdout.take().ok_or("Missing decoder stdout")?;
    let mut buffer = vec![0; n * 3];
    let mut frames = 0u64;
    let mut processed = 0u64;
    let mut sigma_sum = 0i64;
    loop {
        match src.read(&mut buffer[..1]) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
        src.read_exact(&mut buffer[1..])?;
        let (restored, sigma) = if preset == "bm3d" {
            restore(&buffer[..n], w, h)?
        } else {
            if preset == "blind" {
                crate::blind::blind(&buffer[..n], w, h)?
            } else {
                crate::direction::restore(&buffer[..n], w, h)?
            }
        };
        buffer[..n].copy_from_slice(&restored);
        dst.write_all(&buffer)?;
        frames += 1;
        sigma_sum += sigma;
        if sigma >= if preset == "bm3d" { 2 } else { 1 } {
            processed += 1
        }
    }
    drop(src);
    drop(dst);
    if !reader.0.wait()?.success()
        || !writer.0.wait()?.success()
        || frames != t.map_or(1, |v| v.pts.len() as u64)
    {
        return err("Native BM3D encode failed or frame count differs");
    }
    if preset == "bm3d" {
        eprintln!(
            r#"{{"bm3d":"integer_wht_two_stage","frames":{frames},"processed_frames":{processed},"sigma_sum":{sigma_sum}}}"#
        );
    } else {
        eprintln!(
            r#"{{"deconvolution":"{preset}","frames":{frames},"processed_frames":{processed},"kernel_id_sum":{sigma_sum}}}"#
        );
    }
    Ok(())
}

// Portable integer-only fixture: no floating point, codecs or random library.
pub fn fixture() -> Result<Vec<u8>> {
    let mut seed = 9u32;
    let mut pixels = Vec::with_capacity(64 * 64);
    for y in 0..64 {
        for x in 0..64 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = 64 + x * 2 + (y / 16) * 8 + ((seed >> 24) % 25) as i32 - 12;
            pixels.push(v.clamp(0, 255) as u8);
        }
    }
    Ok(restore(&pixels, 64, 64)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transform_is_exact() {
        let mut v: Vec<i64> = (0..256).map(|x| x - 128).collect();
        let original = v.clone();
        hadamard(&mut v);
        hadamard(&mut v);
        assert_eq!(v, original.iter().map(|x| x * 256).collect::<Vec<_>>());
    }
    #[test]
    fn flat_identity() {
        let f = vec![123; 64 * 64];
        let (out, sigma) = restore(&f, 64, 64).unwrap();
        assert_eq!(out, f);
        assert_eq!(sigma, 0)
    }
    #[test]
    fn integer_noise_reduction() {
        let w = 64;
        let h = 64;
        let refp: Vec<_> = (0..w * h)
            .map(|i| if i % w < 32 { 80u8 } else { 160u8 })
            .collect();
        let mut seed = 9u32;
        let noisy: Vec<_> = refp
            .iter()
            .map(|&x| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (x as i32 + ((seed >> 24) % 25) as i32 - 12).clamp(0, 255) as u8
            })
            .collect();
        let (out, sigma) = restore(&noisy, w, h).unwrap();
        assert!(sigma >= 2);
        let sse = |p: &[u8]| {
            p.iter()
                .zip(&refp)
                .map(|(&a, &b)| (a as i64 - b as i64).pow(2))
                .sum::<i64>()
        };
        assert!(sse(&out) < sse(&noisy));
        assert_eq!(out, restore(&noisy, w, h).unwrap().0);
    }
}
