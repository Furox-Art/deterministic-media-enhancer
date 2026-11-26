//! Strict static HDR10 metadata preservation for a limited lossless HEVC path.
use crate::media::*;
use std::collections::BTreeMap;
type Fields = BTreeMap<String, String>;
#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticHdr {
    coords: [u32; 8],
    luminance: [u32; 2],
    light: [u32; 2],
}
fn scaled(s: &str, scale: u64, max: u64) -> Result<u32> {
    let (a, b) = s.split_once('/').ok_or("Invalid HDR rational")?;
    let a: u64 = a.parse()?;
    let b: u64 = b.parse()?;
    let p = a.checked_mul(scale).ok_or("HDR rational overflow")?;
    if b == 0 || p % b != 0 || p / b > max {
        return err("HDR value outside exact representable range");
    }
    Ok((p / b) as u32)
}
fn value<'a>(m: &'a Fields, key: &str) -> Result<&'a str> {
    m.get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing HDR field {key}").into())
}
fn parse_side(
    m: &Fields,
    master: &mut Option<([u32; 8], [u32; 2])>,
    light: &mut Option<[u32; 2]>,
) -> Result<()> {
    match value(m, "side_data_type")? {
        "H.26[45] User Data Unregistered SEI message" => {} // non-HDR vendor SEI explicitly not preserved
        "Mastering display metadata" => {
            let mut c = [0; 8];
            for (i, key) in [
                "green_x",
                "green_y",
                "blue_x",
                "blue_y",
                "red_x",
                "red_y",
                "white_point_x",
                "white_point_y",
            ]
            .iter()
            .enumerate()
            {
                c[i] = scaled(value(m, key)?, 50000, 50000)?
            }
            let l = [
                scaled(value(m, "max_luminance")?, 10000, u32::MAX as u64)?,
                scaled(value(m, "min_luminance")?, 10000, u32::MAX as u64)?,
            ];
            if l[0] == 0
                || l[1] >= l[0]
                || m.len() != 11
                || c.chunks_exact(2).any(|v| v[0] + v[1] > 50000)
                || master.replace((c, l)).is_some()
            {
                return err("Invalid/duplicate mastering display metadata");
            }
        }
        "Content light level metadata" => {
            let l = [
                value(m, "max_content")?.parse::<u16>()? as u32,
                value(m, "max_average")?.parse::<u16>()? as u32,
            ];
            if m.len() != 3 || l[1] > l[0] || light.replace(l).is_some() {
                return err("Invalid/duplicate content light metadata");
            }
        }
        _ => return err("Unsupported HDR side data; refusing metadata loss"),
    }
    Ok(())
}
fn parse_frames(data: &str) -> Result<(StaticHdr, usize)> {
    let (mut frame, mut side) = (false, false);
    let mut fields = Fields::new();
    let (mut master, mut light) = (None, None);
    let (mut first, mut count) = (None, 0usize);
    for line in data.lines() {
        match line {
            "[FRAME]" if !frame && !side => {
                frame = true;
                master = None;
                light = None;
            }
            "[SIDE_DATA]" if frame && !side => {
                side = true;
                fields.clear();
            }
            "[/SIDE_DATA]" if frame && side => {
                parse_side(&fields, &mut master, &mut light)?;
                side = false;
            }
            "[/FRAME]" if frame && !side => {
                let (c, l) = master.ok_or("Missing per-frame mastering metadata")?;
                let h = StaticHdr {
                    coords: c,
                    luminance: l,
                    light: light.ok_or("Missing per-frame light metadata")?,
                };
                if first.as_ref().is_some_and(|f| f != &h) {
                    return err("Changing HDR metadata is unsupported");
                }
                first = Some(h);
                count += 1;
                frame = false;
            }
            "" => {}
            _ if side => {
                let (k, v) = line.split_once('=').ok_or("Malformed HDR field")?;
                if fields.insert(k.into(), v.into()).is_some() {
                    return err("Duplicate HDR field");
                }
            }
            _ => return err("Malformed HDR frame sections"),
        }
    }
    if frame || side {
        return err("Incomplete HDR metadata");
    }
    Ok((first.ok_or("No HDR frames")?, count))
}
fn read(path: &str) -> Result<(Fields, StaticHdr, usize)> {
    let mut p = probe(path)?;
    p.args(["-select_streams","v:0","-show_entries","stream=pix_fmt,color_range,color_space,color_transfer,color_primaries,chroma_location:stream_side_data=side_data_type","-of","default=noprint_wrappers=1",path]);
    let mut tags = Fields::new();
    for l in String::from_utf8(capture(&mut p, 1024 * 1024)?)?.lines() {
        let (k, v) = l.split_once('=').ok_or("Malformed color field")?;
        if k == "side_data_type" || tags.insert(k.into(), v.into()).is_some() {
            return err("Unsupported stream side data or duplicate color field");
        }
    }
    for (k, v) in [
        ("color_range", "tv"),
        ("color_space", "bt2020nc"),
        ("color_transfer", "smpte2084"),
        ("color_primaries", "bt2020"),
    ] {
        if value(&tags, k)? != v {
            return err("HDR10 requires limited-range BT.2020 PQ");
        }
    }
    if !["yuv420p10le", "yuv422p10le", "yuv444p10le"].contains(&value(&tags, "pix_fmt")?) {
        return err("HDR10 requires planar 10-bit YUV");
    }
    let mut p = probe(path)?;
    p.args([
        "-select_streams",
        "v:0",
        "-show_entries",
        "frame=side_data_list:frame_side_data",
        "-of",
        "default",
        path,
    ]);
    let (hdr, count) = parse_frames(&String::from_utf8(capture(&mut p, 16 * 1024 * 1024)?)?)?;
    Ok((tags, hdr, count))
}
pub fn encode(path: &str, out: &str, scale: usize) -> Result<()> {
    if ext(out) != "mkv" {
        return err("HDR10 output must be MKV");
    }
    let (tags, hdr, count) = read(path)?;
    let pix = value(&tags, "pix_fmt")?;
    let [g1, g2, b1, b2, r1, r2, w1, w2] = hdr.coords;
    let [max, min] = hdr.luminance;
    let [cll, fall] = hdr.light;
    let params=format!("lossless=1:pools=none:frame-threads=1:log-level=error:asm=0:hdr10=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G({g1},{g2})B({b1},{b2})R({r1},{r2})WP({w1},{w2})L({max},{min}):max-cll={cll},{fall}");
    let mut c = ff();
    c.args(["-n", "-noautorotate"]);
    input(&mut c, path)?;
    c.args([
        "-map",
        "0:v:0",
        "-map",
        "0:a?",
        "-map_metadata",
        "-1",
        "-map_chapters",
        "-1",
        "-threads",
        "1",
        "-filter_threads",
        "1",
        "-c:v",
        "libx265",
        "-pix_fmt",
        pix,
        "-x265-params",
        &params,
        "-color_range",
        "tv",
        "-colorspace",
        "bt2020nc",
        "-color_trc",
        "smpte2084",
        "-color_primaries",
        "bt2020",
        "-c:a",
        "flac",
        "-flags:a",
        "+bitexact",
        "-fflags",
        "+bitexact",
        "-fps_mode",
        "passthrough",
    ]);
    if let Some(ch) = tags.get("chroma_location") {
        if ["left", "center", "topleft", "top", "bottomleft", "bottom"].contains(&ch.as_str()) {
            c.args(["-chroma_sample_location", ch]);
        }
    }
    if scale > 1 {
        c.args(["-vf", &resize(scale)]);
    }
    c.arg(out);
    run(&mut c)?;
    if read(out)? != (tags.clone(), hdr, count) {
        return err("HDR10 output metadata mismatch");
    }
    if crate::precision::scaled_digest(path, pix, scale)? != crate::precision::digest(out, pix)? {
        return err("HDR10 decoded sample mismatch");
    }
    eprintln!(
        r#"{{"hdr10":"static_metadata","frames":{count},"sample_exact":{},"metadata_exact":true,"scaled_sample_exact":true}}"#,
        scale == 1
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rational() {
        assert_eq!(scaled("17/25", 50000, 50000).unwrap(), 34000);
        assert!(scaled("1/3", 50000, 50000).is_err());
        assert!(scaled("1/0", 50000, 50000).is_err());
        assert!(parse_frames("[FRAME]\n[/FRAME]").is_err());
    }
}
