use std::{error::Error, fs};

use glam::DVec2;
use peniko::Color;
use rasm_engine_ink::{BrushConfig, StrokeSample, tessellate_stroke};
use rasm_renderer_tileink::canvas_from_dabs;
use tileink::WgpuRenderer;

fn main() -> Result<(), Box<dyn Error>> {
    const WIDTH: u32 = 960;
    const HEIGHT: u32 = 540;

    let mut samples = Vec::new();
    for index in 0..=80_u64 {
        let t = index as f64 / 80.0;
        let x = 80.0 + t * 800.0;
        let y = 270.0 + (t * std::f64::consts::TAU * 1.5).sin() * 120.0;
        let pressure = (0.15 + 0.85 * t) as f32;

        samples.push(StrokeSample::new(
            DVec2::new(x, y),
            Some(pressure),
            index * 8_333_333,
        )?);
    }

    let dabs = tessellate_stroke(&samples, BrushConfig::default())?;
    let canvas = canvas_from_dabs(WIDTH, HEIGHT, 1.0, &dabs, Color::from_rgb8(35, 175, 245))?;

    let mut renderer =
        WgpuRenderer::new_default_device(WIDTH, HEIGHT, Color::from_rgb8(18, 18, 18));
    renderer.render(&canvas);

    fs::create_dir_all("target")?;
    renderer.image().save("target/renderer-spike.png")?;

    println!(
        "Rendered {} pressure-aware dabs to target/renderer-spike.png",
        dabs.len()
    );
    Ok(())
}
