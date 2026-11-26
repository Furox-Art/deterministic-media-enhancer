mod analysis;
mod benchmark;
mod blind;
mod bm3d;
mod deconv;
mod direction;
mod hdr;
mod media;
mod multiframe;
mod precision;
mod reference_bench;
mod video;
use media::*;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
const VERSION: &str = concat!("DME ", env!("CARGO_PKG_VERSION"));
const PRESETS: &[&str] = &[
    "auto",
    "clean",
    "detail",
    "resize",
    "precision",
    "hdr10",
    "restore",
    "denoise",
    "regional",
    "deblur",
    "wavelet",
    "patch",
    "bm3d",
    "bm3d-soft",
    "blind",
    "motion-deblur",
    "temporal",
    "stabilize",
    "multiframe",
    "multiframe-local",
    "multiframe-gpu",
];
fn absolute(p: &str) -> Result<String> {
    let p = PathBuf::from(p);
    let p = if p.is_absolute() {
        p
    } else {
        env::current_dir()?.join(p)
    };
    Ok(p.to_str().ok_or("Non-UTF8 path")?.into())
}
fn scale(s: &str) -> Result<usize> {
    let n = s.parse()?;
    if ![1, 2, 4, 8].contains(&n) {
        return err("Invalid scale");
    }
    Ok(n)
}
fn source(p: &str) -> Result<bool> {
    if !Path::new(p).is_file() {
        return err("Input must be a regular file");
    }
    formats(p)?;
    Ok(still(p))
}
fn main_job() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("{VERSION}");
        return Ok(());
    }
    if args.len() == 5 && args[0] == "--reference-benchmark" {
        let motion = match args[4].as_str() {
            "motion" => true,
            "noise" => false,
            _ => return err("Invalid reference benchmark mode"),
        };
        return reference_bench::run(&args[1], args[2].parse()?, args[3].parse()?, motion);
    }
    if args.len() == 2 && args[0] == "--synthetic-benchmark" {
        let count: usize = args[1].parse()?;
        if !(1..=10000).contains(&count) {
            return err("Invalid benchmark count");
        }
        return benchmark::run(count);
    }
    if args.len() == 1 && args[0] == "--kernel-fixture" {
        use std::io::Write;
        std::io::stdout().write_all(&bm3d::fixture()?)?;
        return Ok(());
    }
    if (2..=3).contains(&args.len()) && args[0] == "--analyze" {
        let input = absolute(&args[1])?;
        let s = scale(args.get(2).map(String::as_str).unwrap_or("2"))?;
        let image = source(&input)?;
        let m = analysis::analyze(&input, image)?;
        println!("{}", analysis::optimize(&input, image, &m, s)?.1);
        return Ok(());
    }
    if !(2..=5).contains(&args.len()) {
        return err("Usage: dme INPUT OUTPUT [1|2|4|8] [auto|clean|detail|resize|precision|hdr10|restore|denoise|regional|deblur|wavelet|patch|bm3d|bm3d-soft|blind|motion-deblur|temporal|stabilize|multiframe|multiframe-local|multiframe-gpu] [keep|30|60|120]");
    }
    let input = absolute(&args[0])?;
    let output = absolute(&args[1])?;
    let image = source(&input)?;
    if fs::symlink_metadata(&output).is_ok() {
        return err("Output already exists");
    }
    let s = scale(args.get(2).map(String::as_str).unwrap_or("2"))?;
    let preset = args.get(3).map(String::as_str).unwrap_or("auto");
    if !PRESETS.contains(&preset) {
        return err("Invalid preset");
    }
    let fps = match args.get(4).map(String::as_str).unwrap_or("keep") {
        "keep" => 0,
        "30" => 30,
        "60" => 60,
        "120" => 120,
        _ => return err("Invalid target FPS"),
    };
    let out_ext = ext(&output);
    if (image && out_ext != "png") || (!image && !matches!(out_ext.as_str(), "mkv" | "mp4")) {
        return err("Unsupported output extension");
    }
    if image
        && (fps > 0
            || matches!(
                preset,
                "temporal" | "stabilize" | "multiframe" | "multiframe-local" | "multiframe-gpu"
            ))
    {
        return err("Video-only operation requested for image");
    }
    if preset.starts_with("multiframe") && (s != 2 || fps != 0) {
        return err("Multiframe requires 2x scale and keep FPS");
    }
    if preset == "hdr10" && (image || fps != 0) {
        return err("HDR10 is video-only and requires original FPS");
    }
    if preset == "precision" && fps != 0 {
        return err("Precision mode requires original FPS");
    }
    let timeline = if image {
        None
    } else {
        Some(video::inspect(&input)?)
    };
    let (iw, ih) = if image {
        let f = full(&input)?;
        if f.w
            .checked_mul(f.h)
            .and_then(|n| n.checked_mul(s * s))
            .is_none_or(|n| n > MAX_PIXELS)
        {
            return err("Requested image output exceeds QA pixel limit");
        }
        (f.w, f.h)
    } else {
        let t = timeline.as_ref().unwrap();
        (t.width, t.height)
    };
    if matches!(preset, "patch" | "bm3d" | "bm3d-soft") && iw.min(ih) < 32 {
        return err("Patch/BM3D presets require at least 32x32 pixels");
    }
    let baseline = resize(s);
    let mut chain = match preset {
        "auto" => {
            let m = analysis::analyze(&input, image)?;
            let (f, r) = analysis::optimize(&input, image, &m, s)?;
            eprintln!("{r}");
            f
        }
        "bm3d" | "bm3d-soft" | "blind" | "motion-deblur" | "precision" | "hdr10" => {
            baseline.clone()
        }
        "temporal" | "stabilize" | "regional" | "deblur" | "wavelet" | "patch" => {
            let f = match preset {
                "temporal" => TEMPORAL,
                "stabilize" => STABILIZE,
                "regional" => REGIONAL,
                "deblur" => DEBLUR,
                "wavelet" => WAVELET,
                "patch" => PATCH,
                _ => unreachable!(),
            };
            format!("{f},{baseline}")
        }
        _ => {
            let prefix=match preset{"clean"=>"hqdn3d=1.5:1.5:0:0,","denoise"=>"nlmeans=s=2:p=5:r=9,","restore"=>"deblock=filter=weak:block=8:planes=7,deband=1thr=0.012:2thr=0.012:3thr=0.012:range=-8:direction=-1.570796,hqdn3d=1.5:1.5:0:0,",_=>""};
            let suffix = if matches!(preset, "resize" | "denoise") {
                ""
            } else {
                ",unsharp=5:5:0.35:5:5:0"
            };
            format!("{prefix}{baseline}{suffix}")
        }
    };
    if fps > 0 {
        chain = format!(
            "{},{}",
            video::interpolate(timeline.as_ref().unwrap(), fps)?,
            chain
        )
    }
    let stage = Stage::new(Path::new(&output))?;
    let mut candidate = stage.file(&format!("candidate.{out_ext}"))?;
    if preset == "hdr10" {
        hdr::encode(&input, &candidate, s)?;
    } else if preset == "precision" {
        precision::encode_precision(&input, &candidate, image, s)?;
    } else if preset.starts_with("multiframe") {
        multiframe::encode_multi(&input, &candidate, timeline.as_ref().unwrap(), preset)?
    } else if matches!(preset, "bm3d" | "bm3d-soft" | "blind" | "motion-deblur") {
        bm3d::encode_native(
            &input,
            &candidate,
            iw,
            ih,
            timeline.as_ref(),
            s,
            fps,
            preset,
        )?;
    } else {
        encode(&input, &candidate, &chain, image)?
    }
    if image {
        let result = full(&candidate)?;
        if (result.w, result.h) != (iw * s, ih * s) {
            return err("Image output dimensions mismatch");
        }
        if preset == "auto" && chain != baseline {
            let fallback = stage.file(&format!("baseline.{out_ext}"))?;
            encode(&input, &fallback, &baseline, true)?;
            let ref_image = full(&fallback)?;
            let (accepted, report) = analysis::full_guard(&ref_image, &result)?;
            eprintln!("{report}");
            if !accepted {
                candidate = fallback;
                eprintln!(r#"{{"fallback":"baseline","reason":"full_image_guard"}}"#)
            }
        }
        eprintln!(
            r#"{{"image_guard":"geometry","accepted":true,"input_width":{iw},"input_height":{ih},"output_width":{},"output_height":{}}}"#,
            result.w, result.h
        );
    } else {
        let mut c = ff();
        media::input(&mut c, &candidate)?;
        c.args([
            "-map", "0:v:0", "-map", "0:a?", "-threads", "1", "-f", "null", "-",
        ]);
        run(&mut c)?;
        let result = video::inspect(&candidate)?;
        let src = timeline.as_ref().unwrap();
        video::validate(src, &result, s, fps)?;
        eprintln!("{}", video::report(src, &result, fps));
    }
    fs::hard_link(&candidate, &output)?;
    Ok(())
}
fn main() {
    if let Err(e) = main_job() {
        eprintln!("Error: {e}");
        std::process::exit(2)
    }
}
