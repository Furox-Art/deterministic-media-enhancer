//! Deterministic small synthetic fixture sweep; not an independent real-media corpus.
use crate::{bm3d, media::Result, multiframe};
pub fn run(count: usize) -> Result<()> {
    let (mut before, mut after, mut better, mut equal, mut worse) = (0u64, 0u64, 0, 0, 0);
    for n in 0..count {
        let w = 32;
        let h = 32;
        let clean: Vec<u8> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (65 + ((x + n) % 16) * 2 + ((y + n / 3) % 8) * 2 + ((x / 8 + y / 8 + n) % 2) * 48)
                    as u8
            })
            .collect();
        let mut seed = (n + 1) as u32;
        let amplitude = 3 + (n % 19) as i32;
        let degraded: Vec<u8> = clean
            .iter()
            .map(|&v| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (v as i32 + ((seed >> 16) % (2 * amplitude + 1) as u32) as i32 - amplitude)
                    .clamp(0, 255) as u8
            })
            .collect();
        let (out, _) = bm3d::restore(&degraded, w, h)?;
        let sse = |a: &[u8]| {
            a.iter()
                .zip(&clean)
                .map(|(&x, &y)| (x as i32 - y as i32).pow(2) as u64)
                .sum::<u64>()
        };
        let a = sse(&degraded);
        let b = sse(&out);
        before += a;
        after += b;
        if b < a {
            better += 1
        } else if b == a {
            equal += 1
        } else {
            worse += 1
        }
    }
    eprintln!("Completed {count} synthetic image kernels");
    let (v0, v1, vi, ve, vw, vf) = multiframe::synthetic_benchmark(count);
    println!(
        r#"{{"scope":"synthetic kernel variants; not independent real images or encoded videos","image_cases":{count},"image_sse_before":{before},"image_sse_after":{after},"image_improved":{better},"image_unchanged":{equal},"image_worse":{worse},"sequence_cases":{count},"frames_per_sequence":4,"evaluated_center_frames":{count},"video_sse_before":{v0},"video_sse_after":{v1},"video_improved":{vi},"video_unchanged":{ve},"video_worse":{vw},"video_fused_samples":{vf}}}"#
    );
    Ok(())
}
