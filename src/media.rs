use std::{
    fs,
    io::{self, Read},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn err<T>(s: &str) -> Result<T> {
    Err(s.to_owned().into())
}
pub fn ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
pub fn still(path: &str) -> bool {
    matches!(
        ext(path).as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "webp"
    )
}
pub fn formats(path: &str) -> Result<&'static str> {
    Ok(match ext(path).as_str() {
        "png" => "png_pipe",
        "jpg" | "jpeg" => "jpeg_pipe",
        "bmp" => "bmp_pipe",
        "tif" | "tiff" => "tiff_pipe",
        "webp" => "webp_pipe",
        "mkv" | "webm" => "matroska,webm",
        "mp4" | "mov" => "mov,mp4,m4a,3gp,3g2,mj2",
        "avi" => "avi",
        _ => return err("Unsupported input demuxer"),
    })
}
pub fn ff() -> Command {
    let mut c = Command::new("ffmpeg");
    c.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-xerror",
        "-cpuflags",
        "0",
        "-threads",
        "1",
        "-fflags",
        "+bitexact",
    ])
    .stdin(Stdio::null());
    c
}
pub fn input(c: &mut Command, path: &str) -> Result<()> {
    c.args([
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        formats(path)?,
        "-i",
        path,
    ]);
    Ok(())
}
pub fn probe(path: &str) -> Result<Command> {
    let mut c = Command::new("ffprobe");
    c.args([
        "-v",
        "error",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        formats(path)?,
        "-threads",
        "1",
    ]);
    Ok(c)
}
pub fn run(c: &mut Command) -> Result<()> {
    if !c.status()?.success() {
        return err("Media subprocess failed");
    }
    Ok(())
}
pub fn capture(c: &mut Command, limit: usize) -> Result<Vec<u8>> {
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("Missing stdout")?
        .take(limit as u64 + 1)
        .read_to_end(&mut data);
    if read.is_err() || data.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        return err("Media capture limit or read failure");
    }
    if !child.wait()?.success() {
        return err("Analysis decode failed");
    }
    Ok(data)
}
pub struct ChildGuard(pub Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u8>,
}
pub const MAX_PIXELS: usize = 64 * 1024 * 1024;
fn token<'a>(data: &'a [u8], p: &mut usize) -> Result<&'a str> {
    while *p < data.len() && data[*p].is_ascii_whitespace() {
        *p += 1
    }
    let start = *p;
    while *p < data.len() && !data[*p].is_ascii_whitespace() {
        *p += 1
    }
    Ok(std::str::from_utf8(&data[start..*p])?)
}
pub fn pgm(data: &[u8], max_dim: usize, max_pixels: usize) -> Result<Vec<Frame>> {
    let mut frames = Vec::new();
    let mut p = 0;
    while p < data.len() {
        if token(data, &mut p)? != "P5" {
            return err("Invalid PGM frame");
        }
        let w: usize = token(data, &mut p)?.parse()?;
        let h: usize = token(data, &mut p)?.parse()?;
        if token(data, &mut p)? != "255"
            || w == 0
            || h == 0
            || w > max_dim
            || h > max_dim
            || data.get(p) != Some(&b'\n')
        {
            return err("Invalid PGM header");
        }
        p += 1;
        let n = w.checked_mul(h).ok_or("Pixel count overflow")?;
        if n > max_pixels || n > data.len() - p {
            return err("PGM exceeds limit or truncated");
        }
        frames.push(Frame {
            w,
            h,
            pixels: data[p..p + n].to_vec(),
        });
        p += n;
    }
    if frames.is_empty() {
        return err("No decoded frames");
    }
    Ok(frames)
}
pub fn samples(
    path: &str,
    still: bool,
    size: usize,
    count: usize,
    chain: &str,
) -> Result<Vec<Frame>> {
    let mut vf = if still {
        String::new()
    } else {
        r"select=not(mod(n\,15)),".into()
    };
    vf += &format!(
        r"crop=min(iw\,{size}):min(ih\,{size}):trunc((iw-ow)/16)*8:trunc((ih-oh)/16)*8:exact=1"
    );
    if !chain.is_empty() {
        vf += ",";
        vf += chain
    }
    vf += ",format=gray";
    let mut c = ff();
    c.args(["-t", "10"]);
    input(&mut c, path)?;
    c.args([
        "-map",
        "0:v:0",
        "-filter_threads",
        "1",
        "-vf",
        &vf,
        "-frames:v",
        &if still { 1 } else { count }.to_string(),
        "-fps_mode",
        "passthrough",
        "-threads",
        "1",
        "-c:v",
        "pgm",
        "-f",
        "image2pipe",
        "pipe:1",
    ]);
    pgm(&capture(&mut c, 4 * 1024 * 1024)?, size, size * size)
}
pub fn full(path: &str) -> Result<Frame> {
    let mut c = ff();
    c.args(["-max_pixels", "67108864"]);
    input(&mut c, path)?;
    c.args([
        "-map",
        "0:v:0",
        "-filter_threads",
        "1",
        "-vf",
        "format=gray",
        "-frames:v",
        "1",
        "-threads",
        "1",
        "-c:v",
        "pgm",
        "-f",
        "image2pipe",
        "pipe:1",
    ]);
    let mut f = pgm(&capture(&mut c, MAX_PIXELS + 1024)?, MAX_PIXELS, MAX_PIXELS)?;
    if f.len() != 1 {
        return err("Expected one image frame");
    }
    Ok(f.remove(0))
}
static SEQ: AtomicU64 = AtomicU64::new(0);
pub struct Stage(pub PathBuf);
impl Stage {
    pub fn new(output: &Path) -> Result<Self> {
        let parent = output.parent().ok_or("Missing output parent")?;
        for _ in 0..1024 {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let p = parent.join(format!(".dme-stage-{}-{seq}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&p) {
                Ok(()) => return Ok(Self(p)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        err("Cannot create private staging directory")
    }
    pub fn file(&self, name: &str) -> Result<String> {
        Ok(self.0.join(name).to_str().ok_or("Non-UTF8 path")?.into())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub fn resize(scale: usize) -> String {
    format!("scale=iw*{scale}:ih*{scale}:flags=lanczos+accurate_rnd+bitexact")
}
pub const REGIONAL: &str = "format=yuv444p,smartblur=lr=1.2:ls=0.65:lt=8:cr=0.8:cs=0.4:ct=6";
pub const DEBLUR:&str="format=yuv444p,convolution=0m='-1 -2 -1 -2 44 -2 -1 -2 -1':0rdiv=0.03125,unsharp=7:7:0.12:5:5:0";
pub const WAVELET:&str="format=yuv444p,vaguedenoiser=threshold=2:method=garrote:nsteps=3:percent=75:planes=1:type=universal";
pub const PATCH:&str="format=yuv444p,bm3d=sigma=48:block=8:bstep=4:group=1:range=9:mstep=2:thmse=0:estim=basic:planes=1";
pub const BM3D:&str="format=yuv444p,bm3d=sigma=64:block=8:bstep=4:group=4:range=9:mstep=2:thmse=0:estim=basic:planes=1";
pub const TEMPORAL: &str =
    "format=yuv444p,atadenoise=0a=0.015:0b=0.03:1a=0.012:1b=0.025:2a=0.012:2b=0.025:s=5:a=s";
pub const STABILIZE: &str =
    "format=yuv444p,deshake=rx=16:ry=16:edge=mirror:blocksize=8:contrast=80:search=exhaustive";
pub fn encoding(c: &mut Command, out: &str, image: bool) {
    c.args([
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
    if image {
        c.args([
            "-frames:v",
            "1",
            "-an",
            "-c:v",
            "png",
            "-pix_fmt",
            "rgba64be",
        ]);
    } else if ext(out) == "mkv" {
        c.args([
            "-c:v",
            "ffv1",
            "-level",
            "3",
            "-pix_fmt",
            "yuv444p16le",
            "-c:a",
            "flac",
            "-flags:a",
            "+bitexact",
            "-fflags",
            "+bitexact",
        ]);
    } else {
        c.args([
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "16",
            "-pix_fmt",
            "yuv444p",
            "-x264-params",
            "threads=1:lookahead-threads=1",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-flags:a",
            "+bitexact",
            "-fflags",
            "+bitexact",
        ]);
    }
    if !image {
        c.args(["-fps_mode", "passthrough"]);
    }
    c.arg(out);
}
pub fn encode(path: &str, out: &str, chain: &str, image: bool) -> Result<()> {
    let mut c = ff();
    c.arg("-n");
    input(&mut c, path)?;
    c.args(["-map", "0:v:0"]);
    if !image {
        c.args(["-map", "0:a?"]);
    }
    c.args(["-vf", chain]);
    encoding(&mut c, out, image);
    run(&mut c)
}
