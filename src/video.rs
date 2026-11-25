use crate::media::*;
use std::collections::BTreeMap;
#[derive(Default, Debug)]
pub struct Timeline {
    pub width: usize,
    pub height: usize,
    pub pts: Vec<i64>,
    pub tail: i64,
}
impl Timeline {
    pub fn duration(&self) -> i64 {
        self.pts[self.pts.len() - 1] - self.pts[0] + self.tail
    }
}
pub fn timestamp(s: &str) -> Result<i64> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let sec: i64 = whole.parse()?;
    if !(-1_000_000_000..=1_000_000_000).contains(&sec) || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return err("Timestamp out of range");
    }
    let frac = format!("{:0<6}", &frac[..frac.len().min(6)]).parse::<i64>()?;
    Ok(sec * 1_000_000 + if s.starts_with('-') { -frac } else { frac })
}
pub fn inspect(path: &str) -> Result<Timeline> {
    let mut c = probe(path)?;
    c.args([
        "-select_streams",
        "v:0",
        "-show_entries",
        "frame=best_effort_timestamp_time,pkt_duration_time,width,height",
        "-of",
        "compact=p=0:nk=0",
        path,
    ]);
    let data = capture(&mut c, 64 * 1024 * 1024)?;
    let raw = std::str::from_utf8(&data)?;
    let mut t = Timeline::default();
    for line in raw.lines() {
        let fields: BTreeMap<_, _> = line.split('|').filter_map(|s| s.split_once('=')).collect();
        let Some(pts) = fields.get("best_effort_timestamp_time") else {
            continue;
        };
        let pts = timestamp(pts)?;
        let w: usize = fields.get("width").ok_or("Missing width")?.parse()?;
        let h: usize = fields.get("height").ok_or("Missing height")?.parse()?;
        if w == 0 || h == 0 {
            return err("Invalid video dimensions");
        }
        if t.pts.is_empty() {
            t.width = w;
            t.height = h
        }
        if w != t.width || h != t.height {
            return err("Changing video dimensions unsupported");
        }
        if t.pts.last().is_some_and(|p| pts <= *p) {
            return err("Non-increasing video timestamps");
        }
        t.pts.push(pts);
        t.tail = match fields.get("pkt_duration_time") {
            Some(s) if *s != "N/A" => timestamp(s)?,
            _ => 0,
        };
    }
    if t.pts.is_empty() {
        return err("No decoded video frames");
    }
    if t.tail <= 0 && t.pts.len() > 1 {
        t.tail = t.pts[t.pts.len() - 1] - t.pts[t.pts.len() - 2]
    }
    if t.tail <= 0 {
        return err("Unknown final frame duration");
    }
    Ok(t)
}
pub fn cfr(t: &Timeline) -> bool {
    if t.pts.len() < 3 {
        return false;
    }
    let span = t.pts[t.pts.len() - 1] - t.pts[0];
    if span <= 0 {
        return false;
    }
    let step = span / (t.pts.len() - 1) as i64;
    (t.tail - step).abs() <= 2000 && t.pts.windows(2).all(|v| (v[1] - v[0] - step).abs() <= 2000)
}
pub fn target_frames(t: &Timeline, fps: i64) -> i64 {
    ((t.duration() as i128 * fps as i128 + 500_000) / 1_000_000) as i64
}
pub fn interpolate(t: &Timeline, fps: i64) -> Result<String> {
    if !cfr(t) || t.pts[0].abs() > 2000 || t.width < 32 || t.height < 32 {
        return err("Motion interpolation requires near-zero-start CFR video, at least 3 frames and 32x32 pixels");
    }
    let count = target_frames(t, fps);
    if count < 1 || count < t.pts.len() as i64 {
        return err("Target FPS is lower than source FPS or invalid");
    }
    Ok(format!("format=yuv444p,tpad=stop_mode=clone:stop=3,minterpolate=fps={fps}:mi_mode=mci:mc_mode=aobmc:me_mode=bilat:me=epzs:vsbmc=1:scd=fdiff:scd_threshold=10,trim=end_frame={count}"))
}
pub fn validate(a: &Timeline, b: &Timeline, scale: usize, fps: i64) -> Result<()> {
    if a.width.checked_mul(scale) != Some(b.width) || a.height.checked_mul(scale) != Some(b.height)
    {
        return err("Video output dimensions mismatch");
    }
    let want = if fps > 0 {
        target_frames(a, fps)
    } else {
        a.pts.len() as i64
    };
    if b.pts.len() as i64 != want {
        return err("Video output frame count mismatch");
    }
    if (a.duration() - b.duration()).abs() > a.tail.max(b.tail) + 2000 {
        return err("Video output duration mismatch");
    }
    for (i, &p) in b.pts.iter().enumerate() {
        let expected = if fps > 0 {
            i as i64 * 1_000_000 / fps
        } else {
            a.pts[i] - a.pts[0]
        };
        if (p - b.pts[0] - expected).abs() > 2000 {
            return err("Video output timestamp cadence mismatch");
        }
    }
    Ok(())
}
pub fn report(a: &Timeline, b: &Timeline, fps: i64) -> String {
    format!(
        r#"{{"video_guard":"timeline","accepted":true,"input_frames":{},"output_frames":{},"input_duration_us":{},"output_duration_us":{},"target_fps":{fps}}}"#,
        a.pts.len(),
        b.pts.len(),
        a.duration(),
        b.duration()
    )
}
pub fn native_rate(path: &str, t: &Timeline) -> Result<String> {
    let mut c = probe(path)?;
    c.args([
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=avg_frame_rate",
        "-of",
        "csv=p=0",
        path,
    ]);
    let raw = String::from_utf8(capture(&mut c, 4096)?)?;
    let rate = raw.trim();
    let (n, d) = rate.split_once('/').ok_or("Invalid native rate")?;
    let n: i64 = n.parse()?;
    let d: i64 = d.parse()?;
    if n <= 0 || d <= 0 || n > 1_000_000 || d > 1_000_000 || n > 120 * d {
        return err("Unsupported native rate");
    }
    for (i, p) in t.pts.iter().enumerate() {
        let expect = (i as i128 * 1_000_000 * d as i128 / n as i128) as i64;
        if (p - t.pts[0] - expect).abs() > 2000 {
            return err("Native rate does not match timeline");
        }
    }
    Ok(rate.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeline() {
        let mut a = Timeline {
            width: 160,
            height: 120,
            pts: vec![0, 41667, 83333, 125000],
            tail: 41667,
        };
        assert!(cfr(&a));
        assert_eq!(target_frames(&a, 60), 10);
        let mut b = Timeline {
            width: 320,
            height: 240,
            pts: (0..10).map(|i| i * 1_000_000 / 60).collect(),
            tail: 16667,
        };
        validate(&a, &b, 2, 60).unwrap();
        let last = b.pts.pop().unwrap();
        assert!(validate(&a, &b, 2, 60).is_err());
        b.pts.push(last);
        b.pts[4] += 9000;
        assert!(validate(&a, &b, 2, 60).is_err());
        a.pts[2] += 10000;
        assert!(!cfr(&a));
        assert!(interpolate(&a, 60).is_err());
        assert_eq!(timestamp("-0.125000").unwrap(), -125000);
    }
}
