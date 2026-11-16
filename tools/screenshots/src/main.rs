//! Renders the widgets' sample screens to PNG for the READMEs.
//!
//! Each widget's `preview::write_shots` test writes its sample screens as the
//! encoded trees the Deck would receive (`<crate>--<shot>--<w>x<h>.tree`).
//! This tool draws them with the Deck's own renderer (`bmc-render`, femtovg
//! on OpenGL) and saves `<crate>-<shot>.png`. macOS only: it uses a CGL
//! context with no window.
//!
//! Usage: `deck-screenshots <trees dir> <png dir>`. Run it with
//! `tools/screenshots.sh`, which also gets the fonts.

use std::ffi::{CString, c_void};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use bmc_render::FrameTimings;
use bmc_render::gpu::FemtoVgRenderer;
use bmc_render::renderer::prelude::*;
use bmc_render::tree::WalkState;
use glow::HasContext as _;
use glutin::config::ConfigTemplateBuilder;
use glutin::api::cgl::display::Display;
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, Version};
use glutin::display::GlDisplay as _;
use raw_window_handle::{AppKitDisplayHandle, RawDisplayHandle};

/// A fixed clock, so nothing the renderer dates or animates depends on the run.
const NOW_UNIX_SECS: i64 = 1_781_088_120; // 2026-06-10 10:42 UTC

/// Frames drawn per shot, a second apart, so that eased values settle.
const FRAMES: u32 = 5;

struct Shot {
    file: PathBuf,
    png: String,
    width: u32,
    height: u32,
}

/// `<crate>--<shot>--<w>x<h>.tree` → the PNG name and the size.
fn parse_name(file: &Path) -> Option<Shot> {
    let stem = file.file_stem()?.to_str()?;
    let mut parts = stem.split("--");
    let (krate, shot, size) = (parts.next()?, parts.next()?, parts.next()?);
    let (width, height) = size.split_once('x')?;
    Some(Shot {
        file: file.to_path_buf(),
        png: format!("{krate}-{shot}.png"),
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    })
}

fn shots(dir: &Path) -> Result<Vec<Shot>> {
    let mut shots = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "tree") {
            shots.push(parse_name(&path).with_context(|| format!("bad shot name: {}", path.display()))?);
        }
    }
    shots.sort_by(|a, b| a.png.cmp(&b.png));
    if shots.is_empty() {
        bail!("no .tree files in {}", dir.display());
    }
    Ok(shots)
}

/// The pixels of a render-target texture, top row first, RGBA.
fn read_texture(gl: &glow::Context, texture: u32, width: u32, height: u32) -> Result<Vec<u8>> {
    let texture = glow::NativeTexture(NonZeroU32::new(texture).context("GL texture name is zero")?);
    let (w, h) = (i32::try_from(width)?, i32::try_from(height)?);
    let mut pixels = vec![0_u8; usize::try_from(width * height * 4)?];
    // SAFETY: the context is current on this thread for the whole run.
    unsafe {
        let fbo = gl.create_framebuffer().map_err(anyhow::Error::msg)?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        gl.read_pixels(0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.delete_framebuffer(fbo);
    }
    // GL reads bottom row first.
    let row = usize::try_from(width * 4)?;
    let flipped = pixels.chunks_exact(row).rev().flatten().copied().collect();
    Ok(flipped)
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (Some(trees), Some(out)) = (args.next(), args.next()) else {
        bail!("usage: deck-screenshots <trees dir> <png dir>");
    };
    let (trees, out) = (PathBuf::from(trees), PathBuf::from(out));
    std::fs::create_dir_all(&out)?;

    // SAFETY: CGL takes no real display; AppKit is only the kind it accepts.
    let display = unsafe { Display::new(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())) }?;
    let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_stencil_size(8).build();
    // SAFETY: the template is valid; the display lives for the whole run.
    let config = unsafe { display.find_configs(template) }?.next().context("no OpenGL config")?;
    let attributes = ContextAttributesBuilder::new()
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 2))))
        .build(None);
    // SAFETY: the config came from this display.
    let _context = unsafe { display.create_context(&config, &attributes) }?.make_current_surfaceless()?;

    let address = |name: &str| -> *const c_void {
        let name = CString::new(name).expect("BUG: GL function names have no null byte");
        display.get_proc_address(&name)
    };
    // SAFETY: the loader resolves against the context made current above.
    let mut renderer = unsafe { FemtoVgRenderer::new(address, 1280, 480, 0, 0) }?;
    // SAFETY: same context.
    let gl = unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) };
    let intl = bmc_intl::shared_formatter(bmc_intl::Language::default());

    for shot in shots(&trees)? {
        let node = bmc_render::deserialize_tree(&std::fs::read(&shot.file)?)?;
        let (image, texture) = renderer.create_render_target(shot.width, shot.height);
        let mut walk = WalkState::default();
        for frame in 0..FRAMES {
            renderer.begin_frame_to_image(image, shot.width, shot.height, 1.0);
            walk.interaction.begin_frame();
            let delta_ms = if frame == 0 { 0 } else { 1_000 };
            let mut ctx = walk.context(delta_ms, NOW_UNIX_SECS, &intl);
            let mut timings = FrameTimings::default();
            #[expect(clippy::cast_precision_loss, reason = "a screen size in pixels")]
            let (w, h) = (shot.width as f32, shot.height as f32);
            bmc_render::layout_and_render(&node, w, h, &mut renderer, &mut timings, &mut ctx)?;
            renderer.flush();
        }
        let pixels = read_texture(&gl, texture, shot.width, shot.height)?;
        renderer.delete_image(image);
        let path = out.join(&shot.png);
        image::save_buffer(&path, &pixels, shot.width, shot.height, image::ExtendedColorType::Rgba8)?;
        println!("{} ({}x{})", path.display(), shot.width, shot.height);
    }
    Ok(())
}
