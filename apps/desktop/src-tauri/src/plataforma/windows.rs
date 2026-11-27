//! Windows: a tarefa 4 implementa (SetWindowsHookEx, WASAPI, SendInput, área de transferência em todos os formatos,
//! volume por sessão). Até lá só compila: nada aqui funciona.

use std::path::Path;
use std::sync::Arc;
use talk_core::ditado::{Ditado, Plataforma};

pub struct Windows;

pub fn nova(_dir: &Path) -> Arc<dyn Plataforma> {
    Arc::new(Windows)
}

/// Gancho de teclado para `atalho_ditar`, `atalho_traduzir` e `atalho_colar`, com `talk_core::casador::Casador`.
pub fn iniciar_gancho(_ditado: Arc<Ditado>, _dir: std::path::PathBuf) {
    todo!("tarefa 4: SetWindowsHookEx")
}

impl Plataforma for Windows {
    fn colar(&self, _texto: &str) -> Result<(), String> { todo!("tarefa 4: SendInput + área de transferência") }
    fn app_ativo(&self) -> String { todo!("tarefa 4: nome do .exe da janela ativa") }
    fn abaixar_outros(&self, _fator: f32) { todo!("tarefa 4: volume por sessão") }
    fn restaurar_outros(&self) { todo!("tarefa 4: volume por sessão") }
    fn bipe(&self, _freq: u32, _ms: u32, _volume: u8) { todo!("tarefa 4") }
    fn iniciar_captura(&self, _microfone: &str) -> Result<(), String> { todo!("tarefa 4: WASAPI") }
    fn parar_captura(&self) -> Vec<i16> { todo!("tarefa 4: WASAPI") }
}
