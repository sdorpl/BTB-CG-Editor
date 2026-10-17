//! Akceleracja sprzętowa okna edytora: tryb (`--gpu auto|on|off`, `BTB_GPU`) i backend renderowania
//! (`--renderer glow|wgpu`, `BTB_RENDERER`). Flaga CLI ma pierwszeństwo przed zmienną środowiskową.
//! Domyślnie: auto + glow (OpenGL), czyli GPU, jeśli sterownik jest dostępny, inaczej programowo.

use eframe::{HardwareAcceleration, Renderer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuConfig {
    pub accel: HardwareAcceleration,
    pub renderer: Renderer,
}

impl Default for GpuConfig {
    fn default() -> Self { Self { accel: HardwareAcceleration::Preferred, renderer: Renderer::Glow } }
}

fn parse_accel(v: &str) -> Option<HardwareAcceleration> {
    match v.to_ascii_lowercase().as_str() {
        "auto" | "preferred" => Some(HardwareAcceleration::Preferred),
        "on" | "required" | "1" | "true" => Some(HardwareAcceleration::Required),
        "off" | "software" | "0" | "false" => Some(HardwareAcceleration::Off),
        _ => None,
    }
}

fn parse_renderer(v: &str) -> Option<Renderer> {
    match v.to_ascii_lowercase().as_str() {
        "glow" | "gl" | "opengl" => Some(Renderer::Glow),
        "wgpu" | "vulkan" | "gpu" => Some(Renderer::Wgpu),
        _ => None,
    }
}

impl GpuConfig {
    /// `args` = argumenty bez nazwy programu, `env` = (BTB_GPU, BTB_RENDERER). Nieznane wartości są pomijane z ostrzeżeniem.
    pub fn resolve(args: &[String], env: (Option<String>, Option<String>)) -> Self {
        let mut cfg = Self::default();
        let mut set = |name: &str, v: &str, accel: bool| {
            let ok = if accel { parse_accel(v).map(|a| cfg.accel = a).is_some() } else { parse_renderer(v).map(|r| cfg.renderer = r).is_some() };
            if !ok { eprintln!("{name}: nieznana wartość „{v}”, pomijam"); }
        };
        if let Some(v) = &env.0 { set("BTB_GPU", v, true); }
        if let Some(v) = &env.1 { set("BTB_RENDERER", v, false); }
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--gpu" => if let Some(v) = it.next() { set("--gpu", v, true) },
                "--renderer" => if let Some(v) = it.next() { set("--renderer", v, false) },
                _ => {}
            }
        }
        cfg
    }

    pub fn from_process() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::resolve(&args, (std::env::var("BTB_GPU").ok(), std::env::var("BTB_RENDERER").ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn defaults_to_preferred_glow() {
        assert_eq!(GpuConfig::resolve(&[], (None, None)), GpuConfig::default());
    }

    #[test]
    fn cli_overrides_env() {
        let c = GpuConfig::resolve(&s(&["--gpu", "off", "--renderer", "wgpu"]), (Some("on".into()), Some("glow".into())));
        assert_eq!(c.accel, HardwareAcceleration::Off);
        assert_eq!(c.renderer, Renderer::Wgpu);
    }

    #[test]
    fn env_and_invalid_values() {
        let c = GpuConfig::resolve(&s(&["--gpu", "bogus"]), (Some("on".into()), Some("nope".into())));
        assert_eq!(c.accel, HardwareAcceleration::Required);
        assert_eq!(c.renderer, Renderer::Glow);
    }
}
