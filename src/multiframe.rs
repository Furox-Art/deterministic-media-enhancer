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
pub fn encode_multi(path: &str, out: &str, t: &Timeline) -> Result<()> {
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
    let mut current = Rgb::new(t.width as i32, t.height as i32);
    let mut next = Rgb::new(current.w, current.h);
    let mut previous = None;
    if !current.read(&mut r)? {
        return err("No source frames");
    }
    let mut stats = Stats::default();
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
