//! Raw grayscale reference benchmark; no model fitting or codec overhead.
use crate::{bm3d, direction, media::*};
use std::{fs::File, io::Read};
fn sse(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (x as i32 - y as i32).pow(2) as u64)
        .sum()
}
pub fn run(path: &str, size: usize, count: usize, motion: bool) -> Result<()> {
    if !(32..=256).contains(&size) || !(1..=10000).contains(&count) {
        return err("Invalid reference sweep size/count");
    }
    let n = size * size;
    let mut f = File::open(path)?;
    if f.metadata()?.len() != (n * count) as u64 {
        return err("Reference stream length mismatch");
    }
    let mut a = vec![0u8; n];
    for i in 0..count {
        f.read_exact(&mut a)?;
        let mut seed = (i as u32).wrapping_add(0x789ab123);
        let b: Vec<_> = a
            .iter()
            .map(|&v| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (v as i32 + ((seed >> 16) % 29) as i32 - 14).clamp(0, 255) as u8
            })
            .collect();
        let (out, sigma) = bm3d::restore(&b, size, size)?;
        let (clean, _) = bm3d::restore(&a, size, size)?;
        print!(
            r#"{{"index":{i},"noise_baseline_sse":{},"noise_candidate_sse":{},"clean_sse":{},"soft_candidate_sse":{},"soft_clean_sse":{},"sigma":{sigma}"#,
            sse(&a, &b),
            sse(&a, &out),
            sse(&a, &clean),
            sse(&a, &bm3d::soften(&b, &out)),
            sse(&a, &bm3d::soften(&a, &clean))
        );
        if motion {
            let (dx, dy) = match i % 4 {
                0 => (1, 0),
                1 => (0, 1),
                2 => (1, 1),
                _ => (1, -1),
            };
            let b: Vec<u8> = (0..n)
                .map(|i| {
                    let x = (i % size) as i32;
                    let y = (i / size) as i32;
                    let s: i32 = (-2..=2)
                        .map(|t| {
                            a[(y + t * dy).clamp(0, size as i32 - 1) as usize * size
                                + (x + t * dx).clamp(0, size as i32 - 1) as usize]
                                as i32
                        })
                        .sum();
                    ((s + 2) / 5) as u8
                })
                .collect();
            let (out, id) = direction::restore(&b, size, size)?;
            let (_, clean_id) = direction::restore(&a, size, size)?;
            print!(
                r#","motion_baseline_sse":{},"motion_candidate_sse":{},"motion_selected_id":{id},"motion_clean_id":{clean_id}"#,
                sse(&a, &b),
                sse(&a, &out)
            );
        }
        println!("}}");
    }
    Ok(())
}
