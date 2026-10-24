//! Renderer selection with fallbacks, so the launcher opens on as many
//! machines as possible (including VMs without OpenGL drivers).
//!
//! Order: wgpu (Direct3D 12 on Windows → Vulkan → OpenGL, choosing a real GPU
//! first and Windows' built-in software rasterizer "WARP" last) and, if that
//! fails to start, OpenGL via glow in a fresh process (winit allows only one
//! event loop per process). If everything fails the user gets an error box
//! instead of a window that silently never appears.
//!
//! The renderer that worked is remembered in `<root>/renderer.txt`.

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

const REMEMBER_FILE: &str = "renderer.txt";

/// `--renderer <x>` overrides; otherwise what worked last time; else wgpu.
pub fn choose(args: &[String], root: &Path) -> Gfx {
    if let Some(i) = args.iter().position(|a| a == "--renderer")
        && let Some(g) = args.get(i + 1).and_then(|s| Gfx::parse(s))
    {
        return g;
    }
    std::fs::read_to_string(root.join(REMEMBER_FILE))
        .ok()
        .and_then(|s| Gfx::parse(&s))
        .unwrap_or(Gfx::Wgpu)
}

pub fn remember(root: &Path, gfx: Gfx) {
    let _ = std::fs::create_dir_all(root);
    let _ = std::fs::write(root.join(REMEMBER_FILE), gfx.id());
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
    fn choose_and_remember() {
        let d = tempfile_dir();
        assert_eq!(choose(&[], &d), Gfx::Wgpu);
        remember(&d, Gfx::Glow);
        assert_eq!(choose(&[], &d), Gfx::Glow);
        let args = vec!["kestrel".to_string(), "--renderer".into(), "dx12".into()];
        assert_eq!(choose(&args, &d), Gfx::Wgpu);
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
