use crate::media::*;
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub frames: i64,
    pub noise: i64,
    pub block: i64,
    pub edge: i64,
    pub curvature: i64,
    pub flat: i64,
}
pub fn measure(f: &Frame) -> Metrics {
    let (
        mut residual,
        mut flat,
        mut total,
        mut boundary,
        mut inner,
        mut nb,
        mut ni,
        mut grad,
        mut curv,
        mut ne,
    ) = (0i64, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let w = f.w;
    let p = |i: usize| f.pixels[i] as i64;
    for y in 1..f.h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let (c, l, r, u, b) = (p(i), p(i - 1), p(i + 1), p(i - w), p(i + w));
            let g = (l - r).abs() + (u - b).abs();
            total += 1;
            if g <= 12 {
                residual += (4 * c - l - r - u - b).abs();
                flat += 1
            }
            if g >= 24 {
                grad += g;
                curv += (4 * c - l - r - u - b).abs();
                ne += 1
            }
            for (coord, delta) in [(x, (c - l).abs()), (y, (c - u).abs())] {
                if coord % 8 == 0 {
                    boundary += delta;
                    nb += 1
                } else {
                    inner += delta;
                    ni += 1
                }
            }
        }
    }
    Metrics {
        frames: 1,
        noise: if flat > 0 {
            residual * 100 / (flat * 4)
        } else {
            0
        },
        flat: if total > 0 { flat * 100 / total } else { 0 },
        block: if nb > 0 && ni > 0 {
            (boundary * ni + nb * ni) * 100 / (inner * nb + nb * ni)
        } else {
            100
        },
        edge: if ne > 0 { grad / ne } else { 0 },
        curvature: if grad > 0 { curv * 100 / grad } else { 0 },
    }
}
pub fn analyze(path: &str, still: bool) -> Result<Metrics> {
    let m: Vec<_> = samples(path, still, 512, 8, "")?
        .iter()
        .map(measure)
        .collect();
    let median = |field: fn(&Metrics) -> i64| {
        let mut v: Vec<_> = m.iter().map(field).collect();
        v.sort_unstable();
        v[v.len() / 2]
    };
    Ok(Metrics {
        frames: m.len() as i64,
        noise: median(|m| m.noise),
        block: median(|m| m.block),
        edge: median(|m| m.edge),
        curvature: median(|m| m.curvature),
        flat: median(|m| m.flat),
    })
}
#[derive(Default, Debug)]
pub struct Comparison {
    pub accepted: bool,
    pub gain: i64,
    pub mae: i64,
    pub edge_loss: i64,
    pub noise_delta: i64,
    pub block_delta: i64,
    pub sharp_gain: i64,
    pub clipping: i64,
}
pub fn compare(a: &Frame, b: &Frame, m: &Metrics) -> Result<Comparison> {
    if a.w != b.w || a.h != b.h || a.pixels.len() != b.pixels.len() || a.pixels.is_empty() {
        return err("Sample dimensions differ");
    }
    let (mut diff, mut fa, mut fb, mut nf, mut ea, mut eb, mut ne, mut clip) =
        (0i64, 0, 0, 0, 0, 0, 0, 0);
    for (&x, &y) in a.pixels.iter().zip(&b.pixels) {
        diff += (x as i64 - y as i64).abs();
        if x > 4 && x < 251 && (y <= 1 || y >= 254) {
            clip += 1
        }
    }
    let w = a.w;
    let p = |i: usize| a.pixels[i] as i64;
    let q = |i: usize| b.pixels[i] as i64;
    for y in 1..a.h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let g = (p(i - 1) - p(i + 1)).abs() + (p(i - w) - p(i + w)).abs();
            let h = (q(i - 1) - q(i + 1)).abs() + (q(i - w) - q(i + w)).abs();
            if g <= 12 {
                fa += (4 * p(i) - p(i - 1) - p(i + 1) - p(i - w) - p(i + w)).abs();
                fb += (4 * q(i) - q(i - 1) - q(i + 1) - q(i - w) - q(i + w)).abs();
                nf += 1
            }
            if g >= 24 {
                ea += g;
                eb += h;
                ne += 1
            }
        }
    }
    let mut c = Comparison {
        mae: diff * 100 / a.pixels.len() as i64,
        clipping: clip * 10000 / a.pixels.len() as i64,
        noise_delta: if nf > 0 {
            (fa - fb) * 100 / (4 * nf)
        } else {
            0
        },
        edge_loss: if ea > 0 { (ea - eb) * 100 / ea } else { 0 },
        sharp_gain: if ne > 0 { (eb - ea) * 100 / ne } else { 0 },
        block_delta: (measure(a).block - 100).max(0) - (measure(b).block - 100).max(0),
        ..Default::default()
    };
    c.accepted = c.mae <= 500
        && (-25..=12).contains(&c.edge_loss)
        && c.clipping <= 50
        && c.noise_delta >= -40
        && c.block_delta >= -25;
    c.gain = if m.noise >= 180 && m.flat >= 20 {
        2 * c.noise_delta
    } else {
        0
    } + if m.block >= 145 && m.edge >= 24 {
        3 * c.block_delta
    } else {
        0
    } + if m.edge >= 24 && m.curvature < 55 && m.noise < 350 {
        c.sharp_gain.clamp(0, 800) / 4
    } else {
        0
    } - c.mae / 2
        - 5 * c.edge_loss.max(0);
    Ok(c)
}
pub fn candidates(m: &Metrics) -> Vec<(&'static str, String)> {
    let noise = m.noise >= 180 && m.flat >= 20;
    let block = m.block >= 145 && m.edge >= 24;
    let soft = m.edge >= 24 && m.curvature < 55 && m.noise < 350;
    let mild = "hqdn3d=1.2:1:0:0";
    let db = "deblock=filter=weak:block=8:planes=7";
    let sh = "unsharp=5:5:0.25:5:5:0";
    let mut v = vec![("baseline", "".to_owned())];
    if noise {
        v.extend([
            ("denoise-mild", mild.into()),
            ("denoise-strong", "hqdn3d=2.5:2:0:0".into()),
            ("nonlocal", "nlmeans=s=2:p=5:r=9".into()),
            (
                "bilateral",
                "bilateral=sigmaS=1.5:sigmaR=0.08:planes=1".into(),
            ),
        ]);
        if m.noise < 1000 {
            v.push(("regional", REGIONAL.into()))
        }
    }
    if block {
        v.push(("deblock", db.into()));
        v.push(("deblock-low", format!("{db}:alpha=0.06:beta=0.03")))
    }
    if soft {
        v.extend([
            ("sharpen-mild", sh.into()),
            ("sharpen-medium", "unsharp=5:5:0.45:5:5:0".into()),
            ("deblur", DEBLUR.into()),
        ])
    }
    let combo = [(block, db), (noise, mild), (soft, sh)]
        .iter()
        .filter_map(|(on, s)| on.then_some(*s))
        .collect::<Vec<_>>()
        .join(",");
    if !combo.is_empty() && !v.iter().any(|(_, s)| *s == combo) {
        v.push(("combined", combo))
    }
    v
}
pub fn optimize(path: &str, still: bool, m: &Metrics, scale: usize) -> Result<(String, String)> {
    let base = samples(path, still, 384, 4, "")?;
    let mut best_score = 0;
    let mut best = ("baseline", String::new());
    let mut reports = Vec::new();
    for (name, chain) in candidates(m) {
        let (mut accepted, mut score) = (true, 0);
        if !chain.is_empty() {
            let result = samples(path, still, 384, 4, &chain)?;
            if result.len() != base.len() {
                return err("Optimization sample count differs");
            }
            for (a, b) in base.iter().zip(&result) {
                let c = compare(a, b, m)?;
                accepted &= c.accepted;
                score += c.gain
            }
            score /= base.len() as i64;
        }
        reports.push(format!(
            r#"{{"name":"{name}","chain":"{chain}","accepted":{accepted},"score":{score}}}"#
        ));
        if accepted && score > 15 && score > best_score {
            best_score = score;
            best = (name, chain)
        }
    }
    let filter = if best.1.is_empty() {
        resize(scale)
    } else {
        format!("{},{}", best.1, resize(scale))
    };
    let report = format!(
        r#"{{"frames":{},"noise_proxy_x100":{},"block_ratio_x100":{},"edge_gradient":{},"edge_curvature_x100":{},"flat_percent":{},"filter":"{}","selected":"{}","candidates":[{}]}}"#,
        m.frames,
        m.noise,
        m.block,
        m.edge,
        m.curvature,
        m.flat,
        filter,
        best.0,
        reports.join(",")
    );
    Ok((filter, report))
}
pub fn full_guard(a: &Frame, b: &Frame) -> Result<(bool, String)> {
    if a.w != b.w || a.h != b.h || a.pixels.len() != b.pixels.len() || a.pixels.is_empty() {
        return err("Full image dimensions differ");
    }
    let (mut tiles, mut rejected, mut mae, mut loss, mut clip) = (0, 0, 0, 0, 0);
    for y in (0..a.h).step_by(240) {
        for x in (0..a.w).step_by(240) {
            let w = 256.min(a.w - x);
            let h = 256.min(a.h - y);
            let tile = |f: &Frame| {
                let mut pixels = Vec::with_capacity(w * h);
                for row in y..y + h {
                    pixels.extend_from_slice(&f.pixels[row * f.w + x..row * f.w + x + w])
                }
                Frame { w, h, pixels }
            };
            let aa = tile(a);
            let bb = tile(b);
            let c = compare(&aa, &bb, &measure(&aa))?;
            tiles += 1;
            if !(c.mae <= 500
                && (-25..=12).contains(&c.edge_loss)
                && c.clipping <= 50
                && c.noise_delta >= -40)
            {
                rejected += 1
            }
            mae = mae.max(c.mae);
            loss = loss.max(c.edge_loss);
            clip = clip.max(c.clipping);
        }
    }
    let accepted = rejected == 0;
    Ok((
        accepted,
        format!(
            r#"{{"output_guard":"full_image_luma","accepted":{accepted},"tiles":{tiles},"rejected_tiles":{rejected},"max_mae_x100":{mae},"max_edge_loss_percent":{loss},"max_new_clipping_x100_percent":{clip}}}"#
        ),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> Frame {
        let mut pixels = vec![];
        for y in 0..h {
            for x in 0..w {
                pixels.push(f(x, y))
            }
        }
        Frame { w, h, pixels }
    }
    #[test]
    fn metrics() {
        let a = frame(128, 128, |_, _| 128);
        let m = measure(&a);
        assert_eq!((m.noise, m.edge, m.block), (0, 0, 100));
        let b = frame(128, 128, |x, y| if (x + y) % 2 == 0 { 124 } else { 132 });
        let m = measure(&b);
        assert!(m.noise >= 350);
        assert!(candidates(&m).iter().any(|(n, _)| *n == "denoise-strong"));
        let c = frame(
            128,
            128,
            |x, y| if (x / 8 + y / 8) % 2 == 0 { 96 } else { 160 },
        );
        let m = measure(&c);
        assert!(m.block >= 145 && m.edge >= 24);
        assert!(candidates(&m).iter().any(|(n, _)| *n == "deblock"));
        let soft = frame(128, 128, |x, _| {
            (128 + (x as i32 - 64) * 16).clamp(0, 255) as u8
        });
        let m = measure(&soft);
        assert!(m.edge >= 24 && m.curvature < 55);
        assert!(candidates(&m).iter().any(|(n, _)| *n == "sharpen-mild"));
        assert_eq!(candidates(&measure(&a)).len(), 1);
    }
    #[test]
    fn optimizer_guards() {
        let a = frame(128, 128, |_, _| 128);
        let c = compare(&a, &a, &measure(&a)).unwrap();
        assert!(c.accepted && c.gain == 0);
        let noisy = frame(128, 128, |x, y| if (x + y) % 2 == 0 { 124 } else { 132 });
        let c = compare(&noisy, &a, &measure(&noisy)).unwrap();
        assert!(c.accepted && c.gain > 15 && c.noise_delta > 0);
        let edge = frame(128, 128, |x, _| if x < 64 { 80 } else { 176 });
        let clip = frame(128, 128, |_, _| 255);
        let c = compare(&edge, &clip, &measure(&edge)).unwrap();
        assert!(!c.accepted && c.clipping > 50);
        assert!(candidates(&measure(&edge))[0].1.is_empty());
    }
    #[test]
    fn guard() {
        let a = frame(720, 520, |_, _| 128);
        assert!(full_guard(&a, &a).unwrap().0);
        let b = frame(
            720,
            520,
            |x, y| if x >= 650 && y >= 450 { 255 } else { 128 },
        );
        assert!(!full_guard(&a, &b).unwrap().0);
        let mut mismatch = b.clone();
        mismatch.w -= 1;
        assert!(full_guard(&a, &mismatch).is_err());
        let c = frame(128, 128, |x, _| if x < 64 { 80 } else { 176 });
        let d = frame(128, 128, |_, _| 128);
        assert!(!compare(&c, &d, &measure(&c)).unwrap().accepted);
    }
}
