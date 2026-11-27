//! Implementações da trait `talk_core::ditado::Plataforma`, uma por sistema.
//! Cada módulo expõe `nova(dir)` (a plataforma) e `iniciar_gancho(ditado, dir)` (o gancho de teclado).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{iniciar_gancho, nova};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{iniciar_gancho, nova};

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("RenanTalk roda só em Linux (X11) e Windows");
