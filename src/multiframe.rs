use crate::{
    media::*,
    video::{self, Timeline},
};
use std::io::{Read, Write};
use std::process::Stdio;
#[derive(Clone)]
struct Rgb {
    w: i32,
    h: i32,
    pixels: Vec<u8>,
}
impl Rgb {
    fn new(w: i32, h: i32) -> Self {
        Self {
            w,
            h,
            pixels: vec![0; w as usize * h as usize * 3],
        }
    }
    fn at(&self, x: i32, y: i32, c: usize) -> i32 {
        self.pixels[(y.clamp(0, self.h - 1) as usize * self.w as usize
            + x.clamp(0, self.w - 1) as usize)
            * 3
            + c] as i32
    }
    fn luma(&self, x: i32, y: i32) -> i32 {
        (77 * self.at(x, y, 0) + 150 * self.at(x, y, 1) + 29 * self.at(x, y, 2) + 128) / 256
    }
    fn read(&mut self, r: &mut impl Read) -> Result<bool> {
        loop {
            match r.read(&mut self.pixels[..1]) {
                Ok(0) => return Ok(false),
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        r.read_exact(&mut self.pixels[1..])?;
        Ok(true)
    }
}
fn luma_half(f: &Rgb, hx: i32, hy: i32) -> i32 {
    let x = hx.div_euclid(2);
    let y = hy.div_euclid(2);
    let u = hx.rem_euclid(2);
    let v = hy.rem_euclid(2);
    ((2 - u) * (2 - v) * f.luma(x, y)
        + u * (2 - v) * f.luma(x + 1, y)
        + (2 - u) * v * f.luma(x, y + 1)
        + u * v * f.luma(x + 1, y + 1)
        + 2)
        / 4
}
#[derive(Default, Clone, Copy, Debug)]
struct Shift {
    dx: i32,
    dy: i32,
    cost: i32,
    zero: i32,
    accepted: bool,
}
fn register(a: &Rgb, b: &Rgb) -> Shift {
    if a.w != b.w || a.h != b.h || a.w < 32 || a.h < 32 {
        return Shift::default();
    }
    let step = 2.max(a.w.max(a.h) / 160) as usize;
    let cost = |dx, dy| {
        let (mut sum, mut n) = (0i64, 0i64);
        for y in (7..a.h - 7).step_by(step) {
            for x in (7..a.w - 7).step_by(step) {
                sum += (b.luma(x, y) - luma_half(a, 2 * x + dx, 2 * y + dy)).abs() as i64;
                n += 1
            }
        }
        (sum * 100 / n.max(1)) as i32
    };
    let zero = cost(0, 0);
    let mut s = Shift {
        cost: zero,
        zero,
        ..Default::default()
    };
    let attempt = |s: &mut Shift, x, y| {
        let c = cost(x, y);
        if c < s.cost {
            s.dx = x;
            s.dy = y;
            s.cost = c
        }
    };
    for y in (-8..=8).step_by(2) {
        for x in (-8..=8).step_by(2) {
            attempt(&mut s, x, y)
        }
    }
    let (bx, by) = (s.dx, s.dy);
    for y in by - 1..=by + 1 {
        for x in bx - 1..=bx + 1 {
            attempt(&mut s, x, y)
        }
    }
    s.accepted = s.cost <= 600 && s.zero - s.cost >= 25 && (s.dx % 2 != 0 || s.dy % 2 != 0);
    s
}
fn cubic(f: &Rgb, hx: i32, hy: i32, c: usize) -> u8 {
    let half = [-1, 9, 9, -1];
    let exact = [0, 16, 0, 0];
    let wx = if hx % 2 != 0 { half } else { exact };
    let wy = if hy % 2 != 0 { half } else { exact };
    let (x, y) = (hx / 2, hy / 2);
    let mut sum = 0;
    for (j, &v) in wy.iter().enumerate() {
        for (i, &u) in wx.iter().enumerate() {
            if u != 0 && v != 0 {
                sum += u * v * f.at(x + i as i32 - 1, y + j as i32 - 1, c)
            }
        }
    }
    ((sum + 128) / 256).clamp(0, 255) as u8
}
#[derive(Default)]
struct Stats {
    neighbors: u64,
    samples: u64,
    frames: u64,
}
fn fuse(center: &Rgb, prev: Option<&Rgb>, next: Option<&Rgb>, stats: &mut Stats) -> Rgb {
    let mut out = Rgb::new(center.w * 2, center.h * 2);
    for y in 0..out.h {
        for x in 0..out.w {
            for c in 0..3 {
                out.pixels[(y as usize * out.w as usize + x as usize) * 3 + c] =
                    cubic(center, x, y, c)
            }
        }
    }
    let neighbors = [prev, next];
    let mut shifts = [Shift::default(); 2];
    for (i, n) in neighbors.iter().enumerate() {
        if let Some(n) = n {
            let mut s = register(center, n);
            if s.accepted {
                let r = register(n, center);
                if !r.accepted || (s.dx + r.dx).abs() > 1 || (s.dy + r.dy).abs() > 1 {
                    s.accepted = false
                }
            }
            shifts[i] = s;
            if s.accepted {
                stats.neighbors += 1
            }
        }
    }
    for y in 0..out.h {
        for x in 0..out.w {
            if x % 2 == 0 && y % 2 == 0 {
                continue;
            }
            let index = (y as usize * out.w as usize + x as usize) * 3;
            let (mut sum, mut count) = ([0i32; 3], 0);
            for (i, n) in neighbors.iter().enumerate() {
                let s = shifts[i];
                if !s.accepted {
                    continue;
                }
                let Some(n) = n else { continue };
                let (hx, hy) = (x - s.dx, y - s.dy);
                if hx % 2 != 0
                    || hy % 2 != 0
                    || hx < 0
                    || hy < 0
                    || hx >= 2 * center.w
                    || hy >= 2 * center.h
                {
                    continue;
                }
                let (nx, ny) = (hx / 2, hy / 2);
                if (0..3).any(|c| (n.at(nx, ny, c) - out.pixels[index + c] as i32).abs() > 18) {
                    continue;
                }
                for (c, v) in sum.iter_mut().enumerate() {
                    *v += n.at(nx, ny, c)
                }
                count += 1;
            }
            if count > 0 {
                for (c, &v) in sum.iter().enumerate() {
                    out.pixels[index + c] =
                        ((out.pixels[index + c] as i32 + 2 * v + (1 + 2 * count) / 2)
                            / (1 + 2 * count)) as u8
                }
                stats.samples += 1
            }
        }
    }
    stats.frames += 1;
    out
}
const TILE: i32 = 16;
fn reg_tile(a: &Rgb, b: &Rgb, tx: i32, ty: i32) -> Shift {
    let x0 = (tx - 4).max(5);
    let y0 = (ty - 4).max(5);
    let x1 = (tx + TILE + 4).min(a.w - 5);
    let y1 = (ty + TILE + 4).min(a.h - 5);
    if x1 - x0 < 8 || y1 - y0 < 8 {
        return Shift::default();
    }
    let cost = |dx, dy| {
        let (mut sum, mut n) = (0i64, 0i64);
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                sum += (b.luma(x, y) - luma_half(a, 2 * x + dx, 2 * y + dy)).abs() as i64;
                n += 1;
            }
        }
        (100 * sum / n) as i32
    };
    let zero = cost(0, 0);
    let mut s = Shift {
        zero,
        cost: zero,
        ..Default::default()
    };
    let trial = |x, y, s: &mut Shift| {
        let c = cost(x, y);
        if c < s.cost {
            s.dx = x;
            s.dy = y;
            s.cost = c;
        }
    };
    for y in (-8..=8).step_by(2) {
        for x in (-8..=8).step_by(2) {
            trial(x, y, &mut s)
        }
    }
    let (bx, by) = (s.dx, s.dy);
    for y in by - 1..=by + 1 {
        for x in bx - 1..=bx + 1 {
            trial(x, y, &mut s)
        }
    }
    let mut alt = i32::MAX;
    for y in (-8..=8).step_by(2) {
        for x in (-8..=8).step_by(2) {
            if (x - s.dx).abs().max((y - s.dy).abs()) >= 2 {
                alt = alt.min(cost(x, y));
            }
        }
    }
    s.accepted = s.cost <= 600
        && s.zero - s.cost >= 25
        && alt - s.cost >= 25
        && (s.dx % 2 != 0 || s.dy % 2 != 0);
    s
}
fn patch_ok(a: &Rgb, b: &Rgb, nx: i32, ny: i32, s: Shift) -> bool {
    if nx < 2 || ny < 2 || nx >= b.w - 2 || ny >= b.h - 2 {
        return false;
    }
    let mut bias = [0i32; 3];
    let mut abs = [0i32; 3];
    let mut n = 0;
    for j in -2..=2 {
        for i in -2..=2 {
            if i == 0 && j == 0 {
                continue;
            }
            let (hx, hy) = (2 * (nx + i) + s.dx, 2 * (ny + j) + s.dy);
            if hx < 0 || hy < 0 || hx >= 2 * a.w || hy >= 2 * a.h {
                return false;
            }
            for c in 0..3 {
                let d = b.at(nx + i, ny + j, c) - cubic(a, hx, hy, c) as i32;
                bias[c] += d;
                abs[c] += d.abs();
            }
            n += 1;
        }
    }
    (0..3).all(|c| bias[c].abs() <= 5 * n && abs[c] <= 6 * n)
}
fn local_fuse(a: &Rgb, ns: &[&Rgb], patch: bool) -> (Rgb, [u64; 4]) {
    let out = fuse(a, None, None, &mut Stats::default());
    local_with_base(a, ns, patch, out)
}
fn local_with_base(a: &Rgb, ns: &[&Rgb], patch: bool, mut out: Rgb) -> (Rgb, [u64; 4]) {
    let mut phases = [0u64; 4];
    for ty in (0..a.h).step_by(TILE as usize) {
        for tx in (0..a.w).step_by(TILE as usize) {
            let shifts: Vec<_> = ns
                .iter()
                .map(|b| {
                    let mut s = reg_tile(a, b, tx, ty);
                    if s.accepted {
                        let r = reg_tile(b, a, tx, ty);
                        s.accepted =
                            r.accepted && (s.dx + r.dx).abs() <= 1 && (s.dy + r.dy).abs() <= 1;
                    }
                    s
                })
                .collect();
            for y in 2 * ty..2 * (ty + TILE).min(a.h) {
                for x in 2 * tx..2 * (tx + TILE).min(a.w) {
                    let phase = (x % 2 + 2 * (y % 2)) as usize;
                    if phase == 0 {
                        continue;
                    }
                    let k = (y * out.w + x) as usize * 3;
                    let mut sum = [0i32; 3];
                    let mut count = 0;
                    for (b, s) in ns.iter().zip(shifts.iter().copied()) {
                        if !s.accepted {
                            continue;
                        }
                        let (hx, hy) = (x - s.dx, y - s.dy);
                        if hx < 0
                            || hy < 0
                            || hx >= 2 * a.w
                            || hy >= 2 * a.h
                            || hx % 2 != 0
                            || hy % 2 != 0
                        {
                            continue;
                        }
                        let (nx, ny) = (hx / 2, hy / 2);
                        if (0..3).any(|c| (b.at(nx, ny, c) - out.pixels[k + c] as i32).abs() > 18) {
                            continue;
                        }
                        if patch && !patch_ok(a, b, nx, ny, s) {
                            continue;
                        }
                        for c in 0..3 {
                            sum[c] += b.at(nx, ny, c);
                        }
                        count += 1;
                    }
                    if count > 0 {
                        for c in 0..3 {
                            out.pixels[k + c] =
                                ((out.pixels[k + c] as i32 + 2 * sum[c] + (1 + 2 * count) / 2)
                                    / (1 + 2 * count)) as u8;
                        }
                        phases[phase] += 1;
                    }
                }
            }
        }
    }
    (out, phases)
}

#[cfg(test)]
mod local_tests {
    use super::*;
    fn signal(x: i32, y: i32) -> u8 {
        (128.0
            + 43.0 * (x as f64 * 0.24).sin()
            + 30.0 * (y as f64 * 0.19).cos()
            + 17.0 * ((x + y) as f64 * 0.45).sin())
        .round() as u8
    }
    fn synth(shift: impl Fn(i32, i32) -> (i32, i32), occ: bool) -> Rgb {
        let mut f = Rgb::new(96, 80);
        for y in 0..f.h {
            for x in 0..f.w {
                let (dx, dy) = shift(x, y);
                let mut v = signal(2 * x + dx, 2 * y + dy);
                if occ && (28..44).contains(&x) && (24..40).contains(&y) {
                    v = v.saturating_add(12);
                }
                let k = (y * f.w + x) as usize * 3;
                f.pixels[k..k + 3].fill(v);
            }
        }
        f
    }
    fn mse(f: &Rgb, x0: i32, y0: i32, x1: i32, y1: i32) -> f64 {
        let (mut sum, mut n) = (0i64, 0i64);
        for y in y0..y1 {
            for x in x0..x1 {
                sum += (f.at(x, y, 0) - signal(x, y) as i32).pow(2) as i64;
                n += 1;
            }
        }
        sum as f64 / n as f64
    }
    #[test]
    fn compare() {
        let a = synth(|_, _| (0, 0), false);
        let base = fuse(&a, None, None, &mut Stats::default());
        let b = synth(|x, _| if x < 48 { (1, 0) } else { (0, 1) }, false);
        let c = synth(|x, _| if x < 48 { (0, 1) } else { (1, 0) }, false);
        let d = synth(|_, _| (1, 1), false);
        let mut st = Stats::default();
        let old = fuse(&a, Some(&b), Some(&c), &mut st);
        let (l2, p2) = local_fuse(&a, &[&b, &c], true);
        let (l3, p3) = local_fuse(&a, &[&b, &c, &d], true);
        println!(
            "mixed global shifts {:?} {:?}, accepted {}, fused {}",
            register(&a, &b),
            register(&a, &c),
            st.neighbors,
            st.samples
        );
        for (name, f) in [
            ("cubic", &base),
            ("old", &old),
            ("local2", &l2),
            ("local3", &l3),
        ] {
            println!(
                "{name} MSE all {:.6}, left {:.6}, right {:.6}",
                mse(f, 8, 8, 184, 152),
                mse(f, 8, 8, 88, 152),
                mse(f, 104, 8, 184, 152)
            );
        }
        println!("local2 phases {:?}, local3 phases {:?}", p2, p3);
        assert!(mse(&l3, 8, 8, 184, 152) < mse(&l2, 8, 8, 184, 152));
        let occ = synth(|_, _| (1, 0), true);
        let clean = synth(|_, _| (1, 0), false);
        let old_occ = fuse(&a, Some(&occ), None, &mut Stats::default());
        let (raw, _) = local_fuse(&a, &[&occ], false);
        let (guard, pg) = local_fuse(&a, &[&occ], true);
        let (clean_out, pc) = local_fuse(&a, &[&clean], true);
        println!("occlusion global {:?}", register(&a, &occ));
        for (name, f) in [
            ("cubic", &base),
            ("old", &old_occ),
            ("local unguarded", &raw),
            ("local guarded", &guard),
            ("local clean", &clean_out),
        ] {
            println!(
                "{name} MSE core {:.6}, all {:.6}",
                mse(f, 62, 54, 82, 74),
                mse(f, 8, 8, 184, 152)
            );
        }
        println!("guard phases {:?}, clean {:?}", pg, pc);
    }
    #[test]
    fn invariants() {
        let a = synth(|_, _| (0, 0), false);
        let base = fuse(&a, None, None, &mut Stats::default());
        let b = synth(|x, _| if x < 48 { (1, 0) } else { (0, 1) }, false);
        let c = synth(|x, _| if x < 48 { (0, 1) } else { (1, 0) }, false);
        let d = synth(|_, _| (1, 1), false);
        let (out, p) = local_fuse(&a, &[&b, &c, &d], true);
        assert!(p[1] > 0 && p[2] > 0 && p[3] > 0);
        assert!(mse(&out, 8, 8, 184, 152) < mse(&base, 8, 8, 184, 152));
        assert_eq!(out.pixels, local_fuse(&a, &[&b, &c, &d], true).0.pixels);
        for y in 0..a.h {
            for x in 0..a.w {
                for c in 0..3 {
                    assert_eq!(out.at(2 * x, 2 * y, c), a.at(x, y, c));
                }
            }
        }
        let occ = synth(|_, _| (1, 0), true);
        let (raw, _) = local_fuse(&a, &[&occ], false);
        let (guard, _) = local_fuse(&a, &[&occ], true);
        assert!(mse(&guard, 62, 54, 82, 74) < mse(&raw, 62, 54, 82, 74));
        let mut cut = Rgb::new(a.w, a.h);
        cut.pixels.fill(255);
        assert_eq!(base.pixels, local_fuse(&a, &[&cut], true).0.pixels);
        assert_eq!(base.pixels, local_fuse(&a, &[&a], true).0.pixels);
        let integer = synth(|_, _| (2, 0), false);
        assert_eq!(base.pixels, local_fuse(&a, &[&integer], true).0.pixels);
        println!("phase, RGB center, determinism, occlusion, cut, identical and integer phase tests passed");
    }
}
struct GpuCubic {
    child: ChildGuard,
    input: Option<std::process::ChildStdin>,
    output: std::process::ChildStdout,
    checked: bool,
}
impl GpuCubic {
    fn new(w: usize, h: usize) -> Result<Self> {
        let helper = std::env::current_exe()?.with_file_name("dme-cuda");
        if !helper.is_file() {
            return err("CUDA helper absent; run scripts/build-gpu.sh");
        }
        let mut child = ChildGuard(
            std::process::Command::new(helper)
                .args([w.to_string(), h.to_string()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let input = child.0.stdin.take().ok_or("Missing CUDA stdin")?;
        let output = child.0.stdout.take().ok_or("Missing CUDA stdout")?;
        Ok(Self {
            child,
            input: Some(input),
            output,
            checked: false,
        })
    }
    fn frame(&mut self, center: &Rgb) -> Result<Rgb> {
        self.input
            .as_mut()
            .ok_or("Closed CUDA stdin")?
            .write_all(&center.pixels)?;
        let mut out = Rgb::new(center.w * 2, center.h * 2);
        self.output.read_exact(&mut out.pixels)?;
        if !self.checked {
            let cpu = fuse(center, None, None, &mut Stats::default());
            if cpu.pixels != out.pixels {
                return err("CUDA first-frame CPU comparison mismatch");
            }
            self.checked = true;
        }
        Ok(out)
    }
    fn finish(mut self) -> Result<()> {
        self.input.take();
        let mut extra = [0];
        if self.output.read(&mut extra)? != 0 {
            return err("Extra CUDA bytes");
        }
        if !self.child.0.wait()?.success() {
            return err("CUDA helper failed");
        }
        Ok(())
    }
}
pub fn encode_multi(path: &str, out: &str, t: &Timeline, mode: &str) -> Result<()> {
    if !video::cfr(t)
        || t.pts[0].abs() > 2000
        || t.width < 32
        || t.height < 32
        || t.width
            .checked_mul(t.height)
            .is_none_or(|n| n > 1920 * 1080)
    {
        return err("Multiframe requires near-zero-start CFR, minimum 32x32 and maximum 2073600 source pixels");
    }
    let rate = video::native_rate(path, t)?;
    let mut enc = ff();
    enc.arg("-n").args([
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgb24",
        "-video_size",
        &format!("{}x{}", t.width * 2, t.height * 2),
        "-framerate",
        &rate,
        "-i",
        "pipe:0",
        "-threads",
        "1",
    ]);
    input(&mut enc, path)?;
    enc.args(["-map", "0:v:0", "-map", "1:a?"]);
    encoding(&mut enc, out, false);
    let mut writer = ChildGuard(enc.stdin(Stdio::piped()).spawn()?);
    let mut w = writer.0.stdin.take().ok_or("Missing encoder input")?;
    let mut dec = ff();
    dec.arg("-noautorotate");
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
        "rgb24",
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let mut reader = ChildGuard(dec.stdout(Stdio::piped()).spawn()?);
    let mut r = reader.0.stdout.take().ok_or("Missing decoder output")?;
    let mut stats = Stats::default();
    let mut phase_counts = [0u64; 4];
    if mode == "multiframe" {
        let mut current = Rgb::new(t.width as i32, t.height as i32);
        let mut next = Rgb::new(current.w, current.h);
        let mut previous = None;
        if !current.read(&mut r)? {
            return err("No source frames");
        }

        loop {
            let more = next.read(&mut r)?;
            let enhanced = fuse(
                &current,
                previous.as_ref(),
                more.then_some(&next),
                &mut stats,
            );
            w.write_all(&enhanced.pixels)?;
            if !more {
                break;
            }
            let old = previous
                .take()
                .unwrap_or_else(|| Rgb::new(current.w, current.h));
            previous = Some(current);
            current = next;
            next = old;
        }
    } else {
        use std::collections::VecDeque;
        let mut queue = VecDeque::<Rgb>::new();
        let mut previous = VecDeque::<Rgb>::new();
        let mut eof = false;
        let mut gpu = if mode == "multiframe-gpu" {
            Some(GpuCubic::new(t.width, t.height)?)
        } else {
            None
        };
        loop {
            while queue.len() < 3 && !eof {
                let mut f = Rgb::new(t.width as i32, t.height as i32);
                if f.read(&mut r)? {
                    queue.push_back(f)
                } else {
                    eof = true
                }
            }
            let Some(center) = queue.pop_front() else {
                break;
            };
            let ns: Vec<_> = previous.iter().rev().chain(queue.iter()).collect();
            let (enhanced, phases) = if let Some(g) = gpu.as_mut() {
                let base = g.frame(&center)?;
                local_with_base(&center, &ns, true, base)
            } else {
                local_fuse(&center, &ns, true)
            };
            for i in 0..4 {
                phase_counts[i] += phases[i]
            }
            stats.samples += phases.iter().sum::<u64>();
            stats.frames += 1;
            w.write_all(&enhanced.pixels)?;
            previous.push_back(center);
            if previous.len() > 2 {
                previous.pop_front();
            }
        }
        if let Some(g) = gpu {
            g.finish()?
        }
    }
    drop(r);
    drop(w);
    if !reader.0.wait()?.success()
        || !writer.0.wait()?.success()
        || stats.frames != t.pts.len() as u64
    {
        return err("Multiframe encode failed or frame count differs");
    }
    eprintln!(
        r#"{{"multiframe":true,"frames":{},"accepted_neighbors":{},"fused_pixels":{}}}"#,
        stats.frames, stats.neighbors, stats.samples
    );
    if mode != "multiframe" {
        eprintln!(
            r#"{{"local_fusion":true,"backend":"{}","phases":[{},{},{},{}],"frames":{}}}"#,
            if mode == "multiframe-gpu" {
                "cuda-cubic"
            } else {
                "cpu"
            },
            phase_counts[0],
            phase_counts[1],
            phase_counts[2],
            phase_counts[3],
            stats.frames
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn truth(x: i32, y: i32) -> i32 {
        (128.0
            + 43.0 * (x as f64 * 0.24).sin()
            + 30.0 * (y as f64 * 0.19).cos()
            + 17.0 * ((x + y) as f64 * 0.45).sin())
        .round() as i32
    }
    fn sample(dx: i32, dy: i32) -> Rgb {
        let mut f = Rgb::new(96, 80);
        for y in 0..f.h {
            for x in 0..f.w {
                let i = (y * f.w + x) as usize * 3;
                f.pixels[i..i + 3].fill(truth(2 * x + dx, 2 * y + dy) as u8)
            }
        }
        f
    }
    #[test]
    fn fusion() {
        let c = sample(0, 0);
        let a = sample(1, 0);
        let b = sample(0, 1);
        let s = register(&c, &a);
        assert!(s.accepted);
        assert_eq!((s.dx, s.dy), (1, 0));
        let base = fuse(&c, None, None, &mut Stats::default());
        let mut stats = Stats::default();
        let out = fuse(&c, Some(&a), Some(&b), &mut stats);
        assert_eq!(stats.neighbors, 2);
        assert!(stats.samples > 0);
        let (mut before, mut after) = (0i64, 0i64);
        for y in 8..out.h - 8 {
            for x in 8..out.w - 8 {
                before += (base.at(x, y, 0) - truth(x, y)).pow(2) as i64;
                after += (out.at(x, y, 0) - truth(x, y)).pow(2) as i64;
                if x % 2 == 0 && y % 2 == 0 {
                    assert_eq!(out.at(x, y, 0), c.at(x / 2, y / 2, 0))
                }
            }
        }
        assert!(after < before);
        let mut cut = Rgb::new(c.w, c.h);
        cut.pixels.fill(255);
        assert_eq!(
            fuse(&c, Some(&cut), None, &mut Stats::default()).pixels,
            base.pixels
        );
        assert_eq!(
            fuse(&c, Some(&a), Some(&b), &mut Stats::default()).pixels,
            out.pixels
        );
    }
}

pub fn synthetic_benchmark(count: usize) -> (u64, u64, usize, usize, usize, u64) {
    let (mut before, mut after, mut improved, mut equal, mut worse, mut fused) =
        (0u64, 0u64, 0, 0, 0, 0u64);
    for n in 0..count {
        let signal = |x: i32, y: i32| -> u8 {
            let tri = |z: i32, k: i32| {
                let t = z.rem_euclid(2 * k);
                t.min(2 * k - t)
            };
            (50 + tri(x + 3 * n as i32, 17 + (n % 9) as i32) * 3
                + tri(y + n as i32, 13 + (n % 7) as i32) * 3
                + tri(x + y + n as i32, 9) * 2)
                .clamp(0, 255) as u8
        };
        let sample = |phase: i32| {
            let mut a = Rgb::new(32, 32);
            for y in 0..32 {
                for x in 0..32 {
                    let (mut dx, mut dy) = (phase % 2, phase / 2);
                    if n % 3 == 1 && x >= 16 {
                        std::mem::swap(&mut dx, &mut dy)
                    }
                    let mut v = signal(2 * x + dx, 2 * y + dy);
                    if n % 3 == 2 && phase != 0 && (10..18).contains(&x) && (10..18).contains(&y) {
                        v = v.saturating_add(12)
                    }
                    let i = (y * 32 + x) as usize * 3;
                    a.pixels[i..i + 3].fill(v);
                }
            }
            a
        };
        let a = sample(0);
        let b = sample(1);
        let c = sample(2);
        let d = sample(3);
        let base = fuse(&a, None, None, &mut Stats::default());
        let (restored, phases) = local_fuse(&a, &[&b, &c, &d], true);
        let sse = |v: &Rgb| {
            let mut s = 0u64;
            for y in 4..60 {
                for x in 4..60 {
                    s += (v.at(x, y, 0) - signal(x, y) as i32).pow(2) as u64
                }
            }
            s
        };
        let x = sse(&base);
        let y = sse(&restored);
        before += x;
        after += y;
        fused += phases.iter().sum::<u64>();
        if y < x {
            improved += 1
        } else if y == x {
            equal += 1
        } else {
            worse += 1
        }
    }
    (before, after, improved, equal, worse, fused)
}
