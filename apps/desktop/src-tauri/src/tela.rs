//! Comandos da tela (webview): finos, só passam a pasta de dados ao `talk-core` e devolvem o JSON que a tela espera
//! (`{ok, ...}`), o mesmo da janela antiga. Nada aqui lê a chave Groq inteira de volta para a tela.

use serde_json::{json, Map, Value};
use std::path::PathBuf;
use talk_core::substituicoes::Regra;
use talk_core::{comandos, groq, pasta};
use tauri::{AppHandle, Manager, WebviewWindow};

const REPO_URL: &str = "https://github.com/renanrmsantos14/RenanTalk";

fn dir() -> PathBuf {
    pasta::padrao()
}

/// Memória própria que o Gerenciador de Tarefas chama de "privada" (Windows) ou o RSS anônimo (Linux), em MB.
#[cfg(windows)]
fn memoria_mb() -> Option<u64> {
    use windows::Win32::System::{ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX}, Threading::GetCurrentProcess};
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        ok.as_bool().then(|| (c.PrivateUsage / 1_048_576) as u64)
    }
}

#[cfg(not(windows))]
fn memoria_mb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = s.lines().find(|l| l.starts_with("RssAnon:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some((kb as f64 / 1024.0).round() as u64)
}

/// Microfones de entrada: `{nome, padrao}`. O Python guardava isso em `dispositivos.json`; aqui vem direto do cpal.
fn dispositivos() -> Value {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let padrao = host.default_input_device().and_then(|d| d.name().ok());
    let mut nomes: Vec<String> = host.input_devices().into_iter().flatten().filter_map(|d| d.name().ok()).collect();
    nomes.dedup();
    Value::Array(nomes.into_iter().map(|n| json!({ "padrao": Some(&n) == padrao.as_ref(), "nome": n })).collect())
}

#[tauri::command]
pub fn estado() -> Value {
    comandos::estado(&dir(), memoria_mb())
}

#[tauri::command]
pub fn ajustes() -> Value {
    comandos::ajustes_da_tela(&dir(), dispositivos())
}

#[tauri::command]
pub fn salvar_ajustes(valores: Map<String, Value>) -> Value {
    comandos::salvar_ajustes(&dir(), &valores)
}

#[tauri::command]
pub fn historico_listar(busca: String) -> Value {
    comandos::historico_listar(&dir(), &busca)
}

#[tauri::command]
pub fn historico_editar(id: i64, texto: String) -> Value {
    comandos::historico_editar(&dir(), id, &texto)
}

#[tauri::command]
pub fn historico_excluir(id: i64) -> Value {
    comandos::historico_excluir(&dir(), id)
}

#[tauri::command]
pub fn historico_apagar_tudo() -> Value {
    comandos::historico_apagar_tudo(&dir())
}

#[tauri::command]
pub fn salvar_vocab(termos: Vec<String>) -> Value {
    comandos::salvar_vocab(&dir(), &termos)
}

#[tauri::command]
pub fn inicio() -> Value {
    comandos::inicio(&dir())
}

#[tauri::command]
pub fn sugestao_resolver(termo: String, acao: String) -> Value {
    comandos::sugestao_resolver(&dir(), &termo, &acao)
}

#[tauri::command]
pub fn substituicoes_ler() -> Value {
    comandos::substituicoes_ler(&dir())
}

#[tauri::command]
pub fn substituicoes_salvar(regras: Vec<Regra>) -> Value {
    comandos::substituicoes_salvar(&dir(), &regras)
}

/// Valida a chave na Groq e grava. Assíncrono: a chamada de rede não trava a fila da janela.
#[tauri::command]
pub async fn salvar_chave(chave: String) -> Value {
    let base = std::env::var("RENANTALK_GROQ_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| groq::BASE.into());
    comandos::salvar_chave(&dir(), &base, &chave)
}

#[tauri::command]
pub fn abrir(alvo: String) -> Value {
    let destino = match alvo.as_str() {
        "log" => dir().join(pasta::ARQ_LOG).to_string_lossy().into_owned(),
        "repo" => REPO_URL.into(),
        "groq" => "https://console.groq.com/keys".into(),
        _ => return json!({ "ok": false }),
    };
    json!({ "ok": crate::casca::abrir_externo(&destino).is_ok() })
}

/// A janela nasce escondida e a tela pede para aparecer já desenhada (sem piscar branco); a casca tem um prazo de segurança.
#[tauri::command]
pub fn mostrar(janela: WebviewWindow) {
    let _ = janela.show();
    let _ = janela.set_focus();
}

/// Esc: fecha só a janela. O app segue residente (ditado e bandeja).
#[tauri::command]
pub fn fechar(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}
