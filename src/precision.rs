//! Explicit high-precision resize path; no restoration filter or tone mapping.
use crate::media::*;
use std::collections::BTreeMap;
type Tags = BTreeMap<String, String>;
const KEYS: &[&str] = &[
    "color_range",
    "color_space",
    "color_transfer",
    "color_primaries",
    "chroma_location",
];
fn metadata(path: &str) -> Result<Tags> {
    let mut c = probe(path)?;
    c.args(["-select_streams","v:0","-show_entries","stream=pix_fmt,color_range,color_space,color_transfer,color_primaries,chroma_location:stream_side_data=side_data_type","-of","default=noprint_wrappers=1",path]);
    let data = String::from_utf8(capture(&mut c, 1024 * 1024)?)?;
    let mut tags = Tags::new();
    for line in data.lines() {
        if let Some((key, val)) = line.split_once('=') {
            if key == "side_data_type" {
                return err("Precision mode does not support stream side-data metadata");
            }
            if key == "pix_fmt" || KEYS.contains(&key) {
                if tags.insert(key.into(), val.into()).is_some() {
                    return err("Ambiguous color metadata");
                }
            }
        }
    }
    if !tags.contains_key("pix_fmt") {
        return err("Missing source pixel format");
    }
    Ok(tags)
}
fn format(tags: &Tags, image: bool) -> Result<String> {
    let f = tags["pix_fmt"].as_str();
    if image {
        return Ok(match f {
            "rgba64be" | "rgba64le" => "rgba64be",
            "rgb48be" | "rgb48le" => "rgb48be",
            "gray16be" | "gray16le" => "gray16be",
            "ya16be" | "ya16le" => "ya16be",
            _ => return err("Precision image requires 16-bit RGB/RGBA/gray PNG-compatible source"),
        }
        .into());
    }
    if [
        "gbrp10le",
        "gbrp12le",
        "gbrp16le",
        "gbrap10le",
        "gbrap12le",
        "gbrap16le",
        "gray10le",
        "gray12le",
        "gray16le",
    ]
    .contains(&f)
    {
        return Ok(f.into());
    }
    for prefix in ["yuv420p", "yuv422p", "yuv444p", "yuva444p"] {
        for depth in ["10le", "12le", "16le"] {
            if f == format!("{prefix}{depth}") {
                return Ok(f.into());
            }
        }
    }
    err("Precision video requires supported planar 10/12/16-bit source")
}
fn frame_side_data(path: &str) -> Result<()> {
    let mut c = probe(path)?;
    c.args([
        "-select_streams",
        "v:0",
        "-show_entries",
        "frame=side_data_list:frame_side_data=side_data_type",
        "-of",
        "default=noprint_wrappers=1",
        path,
    ]);
    let data = String::from_utf8(capture(&mut c, 16 * 1024 * 1024)?)?;
    if data.lines().any(|line| !line.trim().is_empty()) {
        return err(
            "Precision mode rejects frame side data (including mastering/HDR dynamic metadata)",
        );
    }
    Ok(())
}
pub(crate) fn digest(path: &str, pix: &str) -> Result<Vec<u8>> {
    scaled_digest(path, pix, 1)
}
pub(crate) fn scaled_digest(path: &str, pix: &str, scale: usize) -> Result<Vec<u8>> {
    let mut c = ff();
    c.arg("-noautorotate");
    input(&mut c, path)?;
    if scale > 1 {
        c.args(["-vf", &resize(scale)]);
    }
    c.args([
        "-map",
        "0:v:0",
        "-an",
        "-filter_threads",
        "1",
        "-threads",
        "1",
        "-pix_fmt",
        pix,
        "-fps_mode",
        "passthrough",
        "-c:v",
        "rawvideo",
        "-f",
        "hash",
        "-hash",
        "sha256",
        "pipe:1",
    ]);
    capture(&mut c, 1024)
}
pub fn encode_precision(path: &str, out: &str, image: bool, scale: usize) -> Result<()> {
    if !image && ext(out) != "mkv" {
        return err("Precision video output must be MKV");
    }
    let before = metadata(path)?;
    let pix = format(&before, image)?;
    frame_side_data(path)?;
    let mut c = ff();
    c.args(["-n", "-noautorotate"]);
    input(&mut c, path)?;
    c.args([
        "-map",
        "0:v:0",
        "-map_metadata",
        "-1",
        "-map_chapters",
        "-1",
        "-filter_threads",
        "1",
        "-threads",
        "1",
        "-flags:v",
        "+bitexact",
    ]);
    if !image {
        c.args(["-map", "0:a?"]);
    }
    if scale > 1 {
        c.args(["-vf", &resize(scale)]);
    }
    c.args(["-pix_fmt", &pix]);
    if image {
        c.args(["-frames:v", "1", "-an", "-c:v", "png"]);
    } else {
        c.args([
            "-c:v",
            "ffv1",
            "-level",
            "3",
            "-c:a",
            "flac",
            "-flags:a",
            "+bitexact",
            "-fflags",
            "+bitexact",
            "-fps_mode",
            "passthrough",
        ]);
    }
    for (key, option) in [
        ("color_range", "-color_range"),
        ("color_space", "-colorspace"),
        ("color_transfer", "-color_trc"),
        ("color_primaries", "-color_primaries"),
        ("chroma_location", "-chroma_sample_location"),
    ] {
        if let Some(val) = before.get(key) {
            if val != "unknown" && val != "unspecified" && val != "N/A" {
                // FFprobe values only, no shell interpolation.
                if !val
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                {
                    return err("Invalid color metadata");
                }
                c.args([
                    option,
                    if key == "color_space" && val == "gbr" {
                        "rgb"
                    } else {
                        val
                    },
                ]);
            }
        }
    }
    c.arg(out);
    run(&mut c)?;
    let after = metadata(out)?;
    if after["pix_fmt"] != pix {
        return err("Precision pixel format changed");
    }
    for key in KEYS {
        if before.get(*key) != after.get(*key) {
            return err("Precision color metadata changed");
        }
    }
    frame_side_data(out)?;
    if scale == 1 && digest(path, &pix)? != digest(out, &pix)? {
        return err("Precision decoded sample mismatch");
    }
    eprintln!(
        r#"{{"precision":"native_depth_resize","pixel_format":"{pix}","color_tags_match":true,"sample_exact":{}}}"#,
        scale == 1
    );
    Ok(())
}
