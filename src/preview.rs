//! A screenshot job: the game renders one picture of the mod's map (`--shot`) in a child process; the UI polls it without blocking
//! and shows the picture, or the exit status and the end of the game's stderr when it failed.

use crate::launch::{Launch, Shot, shot_outputs};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::render::render_resource::TextureFormat;
use bevy_egui::egui;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

/// A game that has not finished its picture after this long is killed.
const TIMEOUT: Duration = Duration::from_secs(180);
/// How many lines of the game's stderr a failure shows.
const TAIL_LINES: usize = 12;

/// Decodes PNG bytes into an egui image (Bevy's own PNG support, already part of the build).
pub fn decode_png(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let image = Image::from_buffer(bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::default(), RenderAssetUsages::MAIN_WORLD)
        .map_err(|e| format!("cannot decode the picture: {e}"))?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let data = image.data.as_deref().ok_or("the picture has no pixel data")?;
    match image.texture_descriptor.format {
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm if data.len() == w * h * 4 => Ok(egui::ColorImage::from_rgba_unmultiplied([w, h], data)),
        other => Err(format!("unsupported picture format {other:?}")),
    }
}

/// The last `n` non-empty lines of the file at `path` (empty when it cannot be read).
pub fn tail(path: &Path, n: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// The scratch picture for `map` and `ship`: `<scratch>/shots/<map>-<ship>.png`.
pub fn shot_file(map: &str, ship: Option<&str>) -> PathBuf {
    let clean = |s: &str| s.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    crate::overlay::scratch("shots").join(format!("{}-{}.png", clean(map), clean(ship.unwrap_or("default"))))
}

struct Job {
    child: Child,
    started: Instant,
    log: PathBuf,
    picture: PathBuf,
}

/// The latest screenshot job and its result.
#[derive(Default)]
pub struct Preview {
    job: Option<Job>,
    image: Option<egui::ColorImage>,
    texture: Option<egui::TextureHandle>,
    /// A failure to show instead of a picture.
    pub error: Option<String>,
}

/// Where a preview job is: the panel's status line follows it (it used to keep saying "Rendering" after the picture came).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Nothing started yet.
    Idle,
    Rendering,
    /// The picture is in.
    Ready,
    /// The game failed or timed out; `Preview::error` says how.
    Failed,
}

impl Phase {
    /// The status line for this phase (empty when there is nothing to say; a failure is drawn from `Preview::error`).
    pub fn line(self) -> &'static str {
        match self {
            Phase::Idle | Phase::Failed => "",
            Phase::Rendering => "Rendering a preview...",
            Phase::Ready => "Preview ready.",
        }
    }
}

impl Preview {
    /// The state of the latest job. Derived from what is held, so it cannot go stale: a job running is `Rendering`, else an error
    /// is `Failed`, else a picture is `Ready`.
    pub fn phase(&self) -> Phase {
        if self.job.is_some() {
            Phase::Rendering
        } else if self.error.is_some() {
            Phase::Failed
        } else if self.image.is_some() || self.texture.is_some() {
            Phase::Ready
        } else {
            Phase::Idle
        }
    }

    /// Whether the game is still rendering.
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// Starts the game for `launch`, which must carry a [`Shot`]; the previous picture and error are dropped.
    pub fn start(&mut self, launch: &Launch) -> Result<(), String> {
        let Some(shot) = &launch.shot else { return Err("no screenshot requested".into()) };
        let picture = shot_outputs(shot).pop().ok_or("no frame requested")?;
        let log = shot.file.with_extension("log");
        for old in shot_outputs(shot) {
            let _ = crate::project::guard::remove_file(&old, None);
        }
        if let Some(parent) = shot.file.parent() {
            crate::project::guard::create_dir_all(parent, None)?;
        }
        let child = launch.spawn_logged(&log).map_err(|e| format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", crate::launch::game_bin().display()))?;
        self.image = None;
        self.texture = None;
        self.error = None;
        self.job = Some(Job { child, started: Instant::now(), log, picture });
        Ok(())
    }

    /// Looks at the child without waiting; takes in the picture or the failure when it has finished.
    pub fn poll(&mut self) {
        let Some(job) = &mut self.job else { return };
        let status = match job.child.try_wait() {
            Ok(None) if job.started.elapsed() > TIMEOUT => {
                let _ = job.child.kill();
                let _ = job.child.wait();
                self.error = Some(format!("the game did not finish a picture within {} s and was stopped", TIMEOUT.as_secs()));
                self.job = None;
                return;
            }
            Ok(None) => return,
            Ok(Some(s)) => s,
            Err(e) => {
                self.error = Some(format!("cannot wait for the game: {e}"));
                self.job = None;
                return;
            }
        };
        let job = self.job.take().expect("checked above");
        let decoded = match (status.success(), std::fs::read(&job.picture)) {
            (true, Ok(bytes)) => decode_png(&bytes),
            (true, Err(e)) => Err(format!("the game exited without writing {}: {e}", job.picture.display())),
            (false, _) => Err(format!("the game stopped with {status}\n{}", tail(&job.log, TAIL_LINES))),
        };
        match decoded {
            Ok(img) => self.image = Some(img),
            Err(e) => self.error = Some(e),
        }
    }

    /// Draws the picture (or the failure); call every frame, it polls first.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        if self.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("The game is rendering the picture...");
            });
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if let Some(img) = self.image.take() {
            self.texture = Some(ui.ctx().load_texture("playtest-preview", img, egui::TextureOptions::LINEAR));
        }
        if let Some(t) = &self.texture {
            let size = fit(t.size_vec2(), ui.available_width(), MAX_PICTURE_HEIGHT);
            ui.add(egui::Image::new(egui::load::SizedTexture::new(t.id(), size)));
        }
    }
}

/// The tallest the preview picture is drawn, so the controls and Export below it stay on screen.
pub const MAX_PICTURE_HEIGHT: f32 = 300.0;

/// `size` scaled down (never up) to fit `max_w` by `max_h`, keeping the aspect ratio.
pub fn fit(size: egui::Vec2, max_w: f32, max_h: f32) -> egui::Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    size * (max_w.max(1.0) / size.x).min(max_h.max(1.0) / size.y).min(1.0)
}

/// A one-frame shot request for the preview (`frame` as typed in the UI).
pub fn shot(map: &str, ship: Option<&str>, frame: u32) -> Shot {
    Shot { file: shot_file(map, ship), frames: vec![frame], dist: None, yaw: None, pitch: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        let mut crc = 0xFFFF_FFFFu32;
        for b in &crc_input {
            crc ^= *b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&crc_input);
        out.extend_from_slice(&(!crc).to_be_bytes());
        out
    }

    /// A hand-made 2x2 RGBA PNG (stored, uncompressed deflate blocks): red, green / blue, translucent white.
    fn tiny_png() -> Vec<u8> {
        let raw: Vec<u8> = [
            &[0u8][..],
            &[255, 0, 0, 255],
            &[0, 255, 0, 255], //
            &[0u8][..],
            &[0, 0, 255, 255],
            &[255, 255, 255, 128],
        ]
        .concat();
        let (mut a, mut b) = (1u32, 0u32);
        for x in &raw {
            a = (a + *x as u32) % 65521;
            b = (b + a) % 65521;
        }
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
        z.extend_from_slice(&raw);
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut ihdr = 2u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &z));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn a_tiny_png_decodes_to_its_pixels() {
        let img = decode_png(&tiny_png()).unwrap();
        assert_eq!(img.size, [2, 2]);
        assert_eq!(img.pixels[0], egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255));
        assert_eq!(img.pixels[2], egui::Color32::from_rgba_unmultiplied(0, 0, 255, 255));
        assert_eq!(img.pixels[3].a(), 128);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode_png(b"not a png").is_err());
        assert!(decode_png(&tiny_png()[..20]).is_err());
    }

    #[test]
    fn tail_keeps_the_last_non_empty_lines() {
        let f = std::env::temp_dir().join(format!("procyon-editor-tail-{}.log", std::process::id()));
        std::fs::write(&f, "a\nb\n\nc\nd\n").unwrap();
        assert_eq!(tail(&f, 2), "c\nd");
        assert_eq!(tail(&f, 10), "a\nb\nc\nd");
        let _ = std::fs::remove_file(&f);
        assert_eq!(tail(&f, 2), "");
    }

    #[test]
    fn the_shot_file_is_named_after_map_and_ship_in_the_scratch_folder() {
        let f = shot_file("gate/ambush", Some("Ship X"));
        assert!(f.starts_with(crate::overlay::scratch("shots")));
        assert_eq!(f.file_name().unwrap(), "gate_ambush-Ship_X.png");
        assert_eq!(shot_file("m", None).file_name().unwrap(), "m-default.png");
    }

    fn run(script: &str, name: &str) -> (Preview, PathBuf) {
        let dir = std::env::temp_dir().join(format!("procyon-editor-preview-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("game.log");
        let child = std::process::Command::new("sh").args(["-c", script]).stderr(std::fs::File::create(&log).unwrap()).spawn().unwrap();
        let job = Job { child, started: Instant::now(), log, picture: dir.join("shot.png") };
        (Preview { job: Some(job), ..Default::default() }, dir)
    }

    fn finish(p: &mut Preview) {
        for _ in 0..200 {
            p.poll();
            if !p.running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("child did not finish");
    }

    #[test]
    fn a_failed_game_shows_its_status_and_the_end_of_its_stderr() {
        let (mut p, dir) = run("echo first >&2; echo boom >&2; exit 3", "fail");
        finish(&mut p);
        let e = p.error.clone().unwrap();
        assert!(e.contains("boom") && e.contains('3'), "{e}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_successful_game_without_a_picture_and_one_with_a_picture() {
        let (mut p, dir) = run("exit 0", "nopic");
        finish(&mut p);
        assert!(p.error.as_deref().unwrap().contains("without writing"));
        let (mut p, dir2) = run("exit 0", "pic");
        std::fs::write(dir2.join("shot.png"), tiny_png()).unwrap();
        finish(&mut p);
        assert!(p.error.is_none() && p.image.is_some());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[test]
    fn the_phase_follows_the_job_to_ready_or_failed() {
        let (mut p, dir) = run("sleep 0.2; exit 0", "phase-ok");
        std::fs::write(dir.join("shot.png"), tiny_png()).unwrap();
        assert_eq!((p.phase(), p.phase().line()), (Phase::Rendering, "Rendering a preview..."));
        finish(&mut p);
        assert_eq!((p.phase(), p.phase().line()), (Phase::Ready, "Preview ready."));
        let (mut f, dir2) = run("exit 3", "phase-bad");
        finish(&mut f);
        assert_eq!((f.phase(), f.phase().line()), (Phase::Failed, ""));
        assert_eq!(Preview::default().phase(), Phase::Idle);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[test]
    fn a_picture_is_scaled_down_to_the_width_and_the_height_but_never_up() {
        let v = egui::vec2;
        assert_eq!(fit(v(1280.0, 720.0), 640.0, 300.0), v(1280.0 * 300.0 / 720.0, 300.0));
        assert_eq!(fit(v(1280.0, 720.0), 400.0, 300.0), v(400.0, 225.0));
        assert_eq!(fit(v(100.0, 50.0), 400.0, 300.0), v(100.0, 50.0));
        assert_eq!(fit(v(0.0, 50.0), 400.0, 300.0), egui::Vec2::ZERO);
    }

    #[test]
    fn a_launch_without_a_shot_is_refused() {
        // Refused before any process is spawned.
        let mut p = Preview::default();
        assert!(p.start(&Launch::new(Path::new("."), "m")).is_err());
        assert!(!p.running());
    }
}
