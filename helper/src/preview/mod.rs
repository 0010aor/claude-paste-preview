#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod x11_window;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(target_os = "linux")]
use x11_window as platform;

use crate::Result;

const MAX_WIDTH: u32 = 320;
const MAX_HEIGHT: u32 = 200;
const MAX_MONITOR_FRACTION: f64 = 0.2;
const MAX_STRIP_MONITOR_FRACTION: f64 = 0.6;
const TILE_GAP: u32 = 6;
pub(crate) const SCREEN_MARGIN: i32 = 24;
pub(crate) const BACKGROUND: [u8; 3] = [0x26, 0x26, 0x2e];
const CLOSE_MARK_SIZE: u32 = 14;
const CLOSE_MARK_INSET: u32 = 6;
const CLOSE_MARK_BADGE: [u8; 3] = [0x20, 0x20, 0x26];
const CLOSE_MARK_CROSS: [u8; 3] = [0xe6, 0xe6, 0xee];

pub(crate) struct Rgb {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Monitor {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

fn fitted_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = (f64::from(max_width) / f64::from(width))
        .min(f64::from(max_height) / f64::from(height))
        .min(1.0);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

struct AreaAverage {
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
    premultiplied: Vec<u64>,
    alpha: Vec<u64>,
    counts: Vec<u64>,
}

impl AreaAverage {
    fn new(source_width: u32, source_height: u32, width: u32, height: u32) -> Self {
        let bins = (width * height) as usize;
        Self {
            source_width,
            source_height,
            width,
            height,
            premultiplied: vec![0; bins * 3],
            alpha: vec![0; bins],
            counts: vec![0; bins],
        }
    }

    fn add_row(&mut self, y: u32, row: &[u8], channels: usize) {
        let row_bin = (u64::from(y) * u64::from(self.height) / u64::from(self.source_height))
            as u32
            * self.width;
        for (x, source) in row.chunks_exact(channels).enumerate() {
            let (color, alpha) = match source {
                [gray] => ([*gray; 3], 255),
                [gray, alpha] => ([*gray; 3], *alpha),
                [red, green, blue] => ([*red, *green, *blue], 255),
                [red, green, blue, alpha, ..] => ([*red, *green, *blue], *alpha),
                [] => continue,
            };
            let column = (x as u64 * u64::from(self.width) / u64::from(self.source_width)) as u32;
            let bin = (row_bin + column) as usize;
            for (sum, channel) in self.premultiplied[bin * 3..bin * 3 + 3]
                .iter_mut()
                .zip(color)
            {
                *sum += u64::from(channel) * u64::from(alpha);
            }
            self.alpha[bin] += u64::from(alpha);
            self.counts[bin] += 1;
        }
    }

    fn over_background(self) -> Rgb {
        let mut pixels = Vec::with_capacity(self.counts.len() * 3);
        for (bin, &count) in self.counts.iter().enumerate() {
            let full = count.max(1) * 255;
            let uncovered = full - self.alpha[bin].min(full);
            for (covered, background) in self.premultiplied[bin * 3..bin * 3 + 3]
                .iter()
                .zip(BACKGROUND)
            {
                pixels.push(((covered + u64::from(background) * uncovered) / full) as u8);
            }
        }
        Rgb {
            width: self.width,
            height: self.height,
            pixels,
        }
    }
}

fn decode_scaled(path: &str, max_width: u32, max_height: u32) -> Result<Rgb> {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let (source_width, source_height) = (reader.info().width, reader.info().height);
    let channels = reader.output_color_type().0.samples();
    let (width, height) = fitted_size(source_width, source_height, max_width, max_height);
    let mut average = AreaAverage::new(source_width, source_height, width, height);
    let mut y = 0;
    while let Some(row) = reader.next_row()? {
        average.add_row(y, row.data(), channels);
        y += 1;
    }
    Ok(average.over_background())
}

fn tile_box(count: u32, monitor: &Monitor) -> (u32, u32) {
    let fraction_of = |side: i32, fraction: f64| (f64::from(side) * fraction) as u32;
    let strip_width = fraction_of(monitor.width, MAX_STRIP_MONITOR_FRACTION);
    let shared_width = strip_width.saturating_sub(TILE_GAP * (count + 1)) / count.max(1);
    let width = fraction_of(monitor.width, MAX_MONITOR_FRACTION)
        .min(MAX_WIDTH)
        .min(shared_width);
    let height = fraction_of(monitor.height, MAX_MONITOR_FRACTION).min(MAX_HEIGHT);
    (width.max(1), height.max(1))
}

fn side_by_side(tiles: &[Rgb]) -> Rgb {
    let width = tiles.iter().map(|tile| tile.width + TILE_GAP).sum::<u32>() + TILE_GAP;
    let height = tiles.iter().map(|tile| tile.height).max().unwrap_or(1) + TILE_GAP * 2;
    let mut strip = Rgb {
        width,
        height,
        pixels: BACKGROUND.repeat((width * height) as usize),
    };
    let mut left = TILE_GAP;
    for tile in tiles {
        let row_bytes = (tile.width * 3) as usize;
        for y in 0..tile.height {
            let target = (((y + TILE_GAP) * width + left) * 3) as usize;
            let source = (y * tile.width * 3) as usize;
            strip.pixels[target..target + row_bytes]
                .copy_from_slice(&tile.pixels[source..source + row_bytes]);
        }
        left += tile.width + TILE_GAP;
    }
    strip
}

fn paint(picture: &mut Rgb, x: u32, y: u32, color: [u8; 3]) {
    if x < picture.width && y < picture.height {
        let offset = ((y * picture.width + x) * 3) as usize;
        picture.pixels[offset..offset + 3].copy_from_slice(&color);
    }
}

fn draw_close_mark(picture: &mut Rgb) {
    if picture.width < CLOSE_MARK_SIZE * 3 || picture.height < CLOSE_MARK_SIZE * 3 {
        return;
    }
    let left = picture.width - CLOSE_MARK_SIZE - CLOSE_MARK_INSET;
    let top = CLOSE_MARK_INSET;
    for dy in 0..CLOSE_MARK_SIZE {
        for dx in 0..CLOSE_MARK_SIZE {
            paint(picture, left + dx, top + dy, CLOSE_MARK_BADGE);
        }
    }
    for step in 3..CLOSE_MARK_SIZE - 3 {
        for thickness in 0..2 {
            paint(
                picture,
                left + step + thickness,
                top + step,
                CLOSE_MARK_CROSS,
            );
            paint(
                picture,
                left + CLOSE_MARK_SIZE - 1 - step - thickness,
                top + step,
                CLOSE_MARK_CROSS,
            );
        }
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn pick_monitor(monitors: &[(Monitor, bool)]) -> Option<&Monitor> {
    monitors
        .iter()
        .find(|(_, is_primary)| *is_primary)
        .or_else(|| monitors.first())
        .map(|(monitor, _)| monitor)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn bottom_right_origin(monitor: &Monitor, width: u32, height: u32) -> (i32, i32) {
    (
        monitor.x + monitor.width - width as i32 - SCREEN_MARGIN,
        monitor.y + monitor.height - height as i32 - SCREEN_MARGIN,
    )
}

fn compose(paths: &[String], monitor: &Monitor) -> Result<Rgb> {
    let (tile_width, tile_height) = tile_box(paths.len() as u32, monitor);
    let tiles = paths
        .iter()
        .map(|path| decode_scaled(path, tile_width, tile_height))
        .collect::<Result<Vec<_>>>()?;
    let mut picture = side_by_side(&tiles);
    draw_close_mark(&mut picture);
    Ok(picture)
}

#[cfg(unix)]
fn exit_when_parent_exits() {
    const PARENT_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    let parent = std::os::unix::process::parent_id();
    std::thread::spawn(move || loop {
        std::thread::sleep(PARENT_CHECK_INTERVAL);
        if std::os::unix::process::parent_id() != parent {
            std::process::exit(0);
        }
    });
}

#[cfg(windows)]
use platform::exit_when_parent_exits;

pub fn show(paths: &[String]) -> Result<()> {
    exit_when_parent_exits();
    platform::show(|monitor| compose(paths, monitor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_inside_the_box_and_never_upscales() {
        assert_eq!(fitted_size(1600, 900, 640, 400), (640, 360));
        assert_eq!(fitted_size(900, 1600, 640, 400), (225, 400));
        assert_eq!(fitted_size(100, 50, 640, 400), (100, 50));
        assert_eq!(fitted_size(10_000, 1, 640, 400), (640, 1));
    }

    #[test]
    fn averages_rgba_down_and_blends_transparency_over_the_background() {
        let mut average = AreaAverage::new(2, 1, 1, 1);
        average.add_row(0, &[255, 0, 0, 255, 0, 0, 255, 255], 4);
        assert_eq!(average.over_background().pixels, vec![127, 0, 127]);

        let mut transparent = AreaAverage::new(1, 1, 1, 1);
        transparent.add_row(0, &[255, 255, 255, 0], 4);
        assert_eq!(transparent.over_background().pixels, BACKGROUND.to_vec());
    }

    #[test]
    fn reads_gray_and_gray_alpha_rows() {
        let mut gray = AreaAverage::new(1, 1, 1, 1);
        gray.add_row(0, &[200], 1);
        assert_eq!(gray.over_background().pixels, vec![200, 200, 200]);

        let mut gray_alpha = AreaAverage::new(1, 1, 1, 1);
        gray_alpha.add_row(0, &[200, 255], 2);
        assert_eq!(gray_alpha.over_background().pixels, vec![200, 200, 200]);
    }

    #[test]
    fn marks_the_top_right_corner_as_the_close_control_on_large_enough_previews() {
        let mut picture = Rgb {
            width: 100,
            height: 60,
            pixels: vec![255; 100 * 60 * 3],
        };
        draw_close_mark(&mut picture);
        let at = |x: u32, y: u32| {
            &picture.pixels[((y * 100 + x) * 3) as usize..((y * 100 + x) * 3 + 3) as usize]
        };
        assert_eq!(
            at(100 - CLOSE_MARK_INSET - 1, CLOSE_MARK_INSET),
            CLOSE_MARK_BADGE
        );
        assert_eq!(at(0, 0), [255, 255, 255]);

        let mut tiny = Rgb {
            width: 20,
            height: 20,
            pixels: vec![255; 20 * 20 * 3],
        };
        draw_close_mark(&mut tiny);
        assert!(tiny.pixels.iter().all(|&byte| byte == 255));
    }

    #[test]
    fn shares_the_strip_between_images_but_never_grows_a_tile() {
        let monitor = Monitor {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        };
        assert_eq!(tile_box(1, &monitor), (MAX_WIDTH, MAX_HEIGHT));
        assert_eq!(tile_box(4, &monitor), (MAX_WIDTH, MAX_HEIGHT));
        let (width, _) = tile_box(8, &monitor);
        assert!(width < MAX_WIDTH && 8 * (width + TILE_GAP) + TILE_GAP <= 1536);
    }

    #[test]
    fn lays_images_out_side_by_side_inside_a_frame() {
        let red = Rgb {
            width: 2,
            height: 1,
            pixels: [255, 0, 0].repeat(2),
        };
        let blue = Rgb {
            width: 1,
            height: 3,
            pixels: [0, 0, 255].repeat(3),
        };
        let strip = side_by_side(&[red, blue]);
        assert_eq!(
            (strip.width, strip.height),
            (2 + 1 + TILE_GAP * 3, 3 + TILE_GAP * 2)
        );
        let at = |x: u32, y: u32| {
            strip.pixels
                [((y * strip.width + x) * 3) as usize..((y * strip.width + x) * 3 + 3) as usize]
                .to_vec()
        };
        assert_eq!(at(TILE_GAP, TILE_GAP), vec![255, 0, 0]);
        assert_eq!(at(TILE_GAP * 2 + 2, TILE_GAP + 2), vec![0, 0, 255]);
        assert_eq!(at(0, 0), BACKGROUND.to_vec());
    }

    #[test]
    fn prefers_the_primary_monitor_then_the_first() {
        let left = Monitor {
            x: 0,
            y: 595,
            width: 2560,
            height: 1440,
        };
        let right = Monitor {
            x: 2560,
            y: 0,
            width: 1440,
            height: 2560,
        };
        assert_eq!(pick_monitor(&[(right, false), (left, true)]), Some(&left));
        assert_eq!(pick_monitor(&[(right, false)]), Some(&right));
        assert_eq!(pick_monitor(&[]), None);
    }
}
