//! Renderer selection with fallbacks, so the launcher opens on as many
//! machines as possible (including VMs without OpenGL drivers).
//!
//! Two renderers: OpenGL via glow, and wgpu (Direct3D 12 on Windows → Vulkan
//! → OpenGL, choosing a real GPU first and Windows' built-in software
//! rasterizer "WARP" last). If the chosen one fails to start, the other is
//! tried in a fresh process (winit allows only one event loop per process).
//! If everything fails the user gets an error box instead of a window that
//! silently never appears.
//!
//! Default on Windows is OpenGL: Direct3D 12 flip-model presentation can be
//! promoted to a hardware overlay plane (Multi-Plane Overlay), and on some
//! NVIDIA/Windows 10 systems that makes the window flash washed-out frames
//! and the whole desktop stutter while the launcher redraws or moves.
//! OpenGL windows are composed normally by the desktop and are not affected.
//!
//! Choice order: `--renderer` (one launch only) → the Settings choice
//! (`LauncherConfig::renderer`) → the renderer that worked after the default
//! failed (`<root>/renderer-fallback.txt`) → the platform default.

use std::path::Path;
use std::sync::Arc;

use eframe::wgpu;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gfx {
    Wgpu,
    Glow,
}

impl Gfx {
    pub fn id(self) -> &'static str {
        match self {
            Gfx::Wgpu => "wgpu",
            Gfx::Glow => "glow",
        }
    }

    pub fn parse(s: &str) -> Option<Gfx> {
        match s.trim() {
            "wgpu" | "d3d" | "dx12" => Some(Gfx::Wgpu),
            "glow" | "opengl" | "gl" => Some(Gfx::Glow),
            _ => None,
        }
    }

    pub fn other(self) -> Gfx {
        match self {
            Gfx::Wgpu => Gfx::Glow,
            Gfx::Glow => Gfx::Wgpu,
        }
    }
}

/// Written only when the preferred renderer failed and the other one worked.
/// (The old `renderer.txt` recorded every success, so it is no longer read.)
const FALLBACK_FILE: &str = "renderer-fallback.txt";

/// Renderer used when nothing else decides.
pub fn platform_default() -> Gfx {
    if cfg!(windows) { Gfx::Glow } else { Gfx::Wgpu }
}

/// Parse the Settings choice: `auto`, `opengl` or `direct3d`.
pub fn preference(setting: &str) -> Option<Gfx> {
    match setting {
        "opengl" => Some(Gfx::Glow),
        "direct3d" => Some(Gfx::Wgpu),
        _ => None, // "auto" or anything unknown
    }
}

/// Pick the renderer for this start (see the module docs for the order).
pub fn choose(args: &[String], root: &Path, setting: &str) -> Gfx {
    if let Some(i) = args.iter().position(|a| a == "--renderer")
        && let Some(g) = args.get(i + 1).and_then(|s| Gfx::parse(s))
    {
        return g;
    }
    if let Some(g) = preference(setting) {
        return g;
    }
    std::fs::read_to_string(root.join(FALLBACK_FILE))
        .ok()
        .and_then(|s| Gfx::parse(&s))
        .unwrap_or_else(platform_default)
}

/// Record that `gfx` worked after the preferred renderer failed.
pub fn remember_fallback(root: &Path, gfx: Gfx) {
    let _ = std::fs::create_dir_all(root);
    let _ = std::fs::write(root.join(FALLBACK_FILE), gfx.id());
}

static ACTIVE: std::sync::OnceLock<Gfx> = std::sync::OnceLock::new();

/// The renderer this process is using (for the Settings page).
pub fn set_active(gfx: Gfx) {
    let _ = ACTIVE.set(gfx);
}

pub fn active() -> Option<Gfx> {
    ACTIVE.get().copied()
}

impl Gfx {
    pub fn label(self) -> &'static str {
        match self {
            Gfx::Glow => "OpenGL",
            Gfx::Wgpu => {
                if cfg!(windows) {
                    "Direct3D 12"
                } else {
                    "wgpu (Vulkan/OpenGL)"
                }
            }
        }
    }
}

/// Sort key for an adapter; lower is better. Device type first (discrete >
/// integrated > virtual > other > software such as WARP / llvmpipe), then
/// backend: Direct3D 12 / Metal (the platform's native API) > Vulkan > GL.
fn adapter_rank(device: wgpu::DeviceType, backend: wgpu::Backend) -> (u8, u8) {
    let d = match device {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    let b = match backend {
        wgpu::Backend::Dx12 | wgpu::Backend::Metal => 0,
        wgpu::Backend::Vulkan => 1,
        _ => 2,
    };
    (d, b)
}

/// Pick the best adapter that can present to the window.
fn select_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    let mut usable: Vec<&wgpu::Adapter> = adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .collect();
    usable.sort_by_key(|a| {
        let i = a.get_info();
        adapter_rank(i.device_type, i.backend)
    });
    for a in adapters {
        let i = a.get_info();
        log::info!(
            "graphics adapter: {} ({:?}, {:?})",
            i.name,
            i.device_type,
            i.backend
        );
    }
    match usable.first() {
        Some(a) => {
            let i = a.get_info();
            log::info!(
                "using graphics adapter: {} ({:?}, {:?})",
                i.name,
                i.device_type,
                i.backend
            );
            Ok((*a).clone())
        }
        None => Err("no graphics adapter can draw to this window".into()),
    }
}

/// Apply the renderer choice to the native options.
pub fn configure(options: &mut eframe::NativeOptions, gfx: Gfx) {
    match gfx {
        Gfx::Glow => options.renderer = eframe::Renderer::Glow,
        Gfx::Wgpu => {
            options.renderer = eframe::Renderer::Wgpu;
            let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
            setup.native_adapter_selector = Some(Arc::new(select_adapter));
            options.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
        }
    }
}

/// Start the same executable again with the other renderer. Returns true if
/// the new process was started.
pub fn relaunch_with(gfx: Gfx, args: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--renderer" {
            it.next();
            continue;
        }
        if a == "--no-fallback" {
            continue;
        }
        rest.push(a.clone());
    }
    rest.extend(["--renderer".into(), gfx.id().into(), "--no-fallback".into()]);
    match std::process::Command::new(exe).args(&rest).spawn() {
        Ok(_) => true,
        Err(e) => {
            log::error!("could not restart with the {} renderer: {e}", gfx.id());
            false
        }
    }
}

/// Last resort: tell the user why no window appeared.
pub fn show_fatal(message: &str) {
    log::error!("{message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let wide = |s: &str| {
            s.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        };
        let text = wide(message);
        let title = wide("Kestrel could not start");
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive
        // the call; a null owner window is allowed.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_order() {
        let d = tempfile_dir();
        // Nothing decided: platform default (OpenGL on Windows).
        assert_eq!(choose(&[], &d, "auto"), platform_default());
        if cfg!(windows) {
            assert_eq!(platform_default(), Gfx::Glow);
        }
        // The old success file no longer pins Direct3D 12 for existing users.
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("renderer.txt"), "wgpu").unwrap();
        assert_eq!(choose(&[], &d, "auto"), platform_default());
        // A real fallback is remembered (only in Automatic mode).
        remember_fallback(&d, platform_default().other());
        assert_eq!(choose(&[], &d, "auto"), platform_default().other());
        // The Settings choice wins over the fallback file.
        assert_eq!(choose(&[], &d, "opengl"), Gfx::Glow);
        assert_eq!(choose(&[], &d, "direct3d"), Gfx::Wgpu);
        assert_eq!(choose(&[], &d, "bogus"), platform_default().other());
        // --renderer wins over everything, for that launch.
        let args = vec!["kestrel".to_string(), "--renderer".into(), "dx12".into()];
        assert_eq!(choose(&args, &d, "opengl"), Gfx::Wgpu);
        assert_eq!(Gfx::Wgpu.other(), Gfx::Glow);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn adapter_ranking() {
        use wgpu::{Backend as B, DeviceType as D};
        // Same GPU: Direct3D 12 beats Vulkan beats GL.
        assert!(adapter_rank(D::DiscreteGpu, B::Dx12) < adapter_rank(D::DiscreteGpu, B::Vulkan));
        assert!(adapter_rank(D::DiscreteGpu, B::Vulkan) < adapter_rank(D::Other, B::Gl));
        // A real GPU on any backend beats the software rasterizer.
        assert!(adapter_rank(D::IntegratedGpu, B::Gl) < adapter_rank(D::Cpu, B::Dx12));
        assert!(adapter_rank(D::VirtualGpu, B::Vulkan) < adapter_rank(D::Cpu, B::Dx12));
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kestrel-gfx-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
}
