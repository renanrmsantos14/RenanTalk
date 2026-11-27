//! Atualização assinada pelo `latest.json` do release (como o `atualizador.py`: 60 s depois de subir e a cada 6 h,
//! ou pelo botão da aba Sobre e pelo menu da bandeja). O plugin confere a assinatura com a `pubkey` do
//! `tauri.conf.json` antes de instalar; assinatura que não bate não instala nada.
//!
//! `RENANTALK_UPDATE_URL` troca o endereço do `latest.json`, só para teste (num build de release o plugin recusa
//! `http://` a menos que o build tenha `dangerousInsecureTransportProtocol`).
//!
//! Ao instalar, grava `.atualizando` na pasta de dados com a versão de origem e a aba aberta (se a janela estava
//! aberta). O app novo lê o marcador ao subir: registra a troca de versão no log e reabre a janela na mesma aba (#14).

use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use talk_core::ditado::Ditado;
use talk_core::pasta;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const MARCA: &str = ".atualizando";
const PRIMEIRA: Duration = Duration::from_secs(60);
const INTERVALO: Duration = Duration::from_secs(6 * 3600);
static OCUPADO: AtomicBool = AtomicBool::new(false);

/// Verificação automática: 60 s depois de subir e a cada 6 h.
pub fn agendar(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(PRIMEIRA);
        loop {
            let r = tauri::async_runtime::block_on(procurar(&app, false));
            // adiada pelo ditado em curso: tenta de novo em 1 min, não em 6 h
            let adiada = r.get("situacao").and_then(Value::as_str) == Some("adiada");
            std::thread::sleep(if adiada { PRIMEIRA } else { INTERVALO });
        }
    });
}

fn atualizador(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let r = match std::env::var("RENANTALK_UPDATE_URL").ok().filter(|u| !u.is_empty()) {
        Some(u) => {
            let url = u.parse().map_err(|e| format!("RENANTALK_UPDATE_URL inválida ({e})"))?;
            app.updater_builder().endpoints(vec![url]).and_then(|b| b.build())
        }
        None => app.updater(),
    };
    r.map_err(|e| e.to_string())
}

/// Procura, baixa e confere a assinatura; se houver versão nova, instala e reinicia logo depois de responder.
/// A tela recebe `{ok, situacao: "em_dia" | "instalando" | "adiada", versao}` ou `{ok: false, erro}`.
pub async fn procurar(app: &AppHandle, manual: bool) -> Value {
    if OCUPADO.swap(true, Ordering::SeqCst) {
        return json!({ "ok": false, "erro": "Já estou procurando atualização." });
    }
    let dir = pasta::padrao();
    let atual = app.package_info().version.to_string();
    let r = baixar(app, manual).await;
    let resposta = match r {
        Ok(None) => {
            pasta::log(&dir, &format!("atualização: em_dia (v{atual})"));
            json!({ "ok": true, "situacao": "em_dia", "versao": atual })
        }
        Ok(Some((u, None))) => {
            pasta::log(&dir, &format!("atualização: adiada (v{} disponível; ditando agora)", u.version));
            json!({ "ok": true, "situacao": "adiada", "versao": u.version })
        }
        Ok(Some((u, Some(bytes)))) => {
            let versao = u.version.clone();
            let a = app.clone();
            // Responde à tela primeiro; instala e reinicia em seguida.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(800));
                instalar(&a, &u, &bytes);
            });
            return json!({ "ok": true, "situacao": "instalando", "versao": versao }); // OCUPADO fica preso até reiniciar
        }
        Err(e) => {
            pasta::log(&dir, &format!("atualização: erro ({e})"));
            json!({ "ok": false, "erro": e })
        }
    };
    OCUPADO.store(false, Ordering::SeqCst);
    resposta
}

/// `None`: em dia. `Some((u, None))`: há versão nova, mas o ditado está em curso (só na verificação automática).
async fn baixar(app: &AppHandle, manual: bool) -> Result<Option<(Update, Option<Vec<u8>>)>, String> {
    let Some(u) = atualizador(app)?.check().await.map_err(|e| e.to_string())? else { return Ok(None) };
    let ditando = app.try_state::<Arc<Ditado>>().is_some_and(|d| d.gravando());
    if ditando && !manual {
        return Ok(Some((u, None)));
    }
    let dir = pasta::padrao();
    pasta::log(&dir, &format!("atualização: nova v{} (baixando de v{})", u.version, app.package_info().version));
    // `download` já confere a assinatura; um pacote adulterado para aqui.
    let bytes = u.download(|_, _| {}, || {}).await.map_err(|e| match e {
        tauri_plugin_updater::Error::Minisign(_) | tauri_plugin_updater::Error::SignatureUtf8(_) | tauri_plugin_updater::Error::Base64(_) => {
            format!("a assinatura de v{} não confere; nada foi instalado ({e})", u.version)
        }
        _ => format!("download de v{} falhou ({e})", u.version),
    })?;
    Ok(Some((u, Some(bytes))))
}

fn instalar(app: &AppHandle, u: &Update, bytes: &[u8]) {
    let dir = pasta::padrao();
    let aba = app.get_webview_window("main").and_then(|w| w.url().ok()).map(|url| {
        url.query_pairs().find(|(k, _)| k == "aba").map(|(_, v)| v.into_owned()).unwrap_or_else(|| "inicio".into())
    });
    let marca = json!({ "de": app.package_info().version.to_string(), "para": u.version, "aba": aba });
    let _ = pasta::gravar_atomico(&dir.join(MARCA), &marca.to_string());
    pasta::log(&dir, &format!("atualização: instalando v{} (janela {})", u.version, aba.as_deref().unwrap_or("fechada")));
    // Antes do install: no Windows ele encerra o processo (exit) e o volume dos outros apps ficaria baixo.
    if let Some(d) = app.try_state::<Arc<Ditado>>() {
        d.encerrar();
    }
    // Windows: o instalador NSIS assume daqui, encerra este processo e abre o app novo.
    if let Err(e) = u.install(bytes) {
        let _ = std::fs::remove_file(dir.join(MARCA));
        pasta::log(&dir, &format!("atualização: erro (instalar: {e})"));
        OCUPADO.store(false, Ordering::SeqCst);
        return;
    }
    app.restart(); // Linux: o AppImage já foi trocado no lugar
}

/// Ao subir: lê o marcador da atualização, registra a troca de versão e reabre a janela na aba em que estava.
pub fn ao_iniciar(app: &AppHandle) {
    let dir = pasta::padrao();
    let arq = dir.join(MARCA);
    let Ok(txt) = std::fs::read_to_string(&arq) else { return };
    let _ = std::fs::remove_file(&arq);
    let m: Value = serde_json::from_str(&txt).unwrap_or_default();
    let (de, para) = (m["de"].as_str().unwrap_or("?"), m["para"].as_str().unwrap_or("?"));
    let atual = app.package_info().version.to_string();
    if para == atual {
        pasta::log(&dir, &format!("atualização: instalada (v{de} → v{atual})"));
    } else {
        pasta::log(&dir, &format!("atualização: erro (pedi v{para}, subiu v{atual})"));
    }
    if let Some(aba) = m["aba"].as_str().and_then(|a| crate::casca::aba_dos_args(&[format!("--aba={a}")])) {
        crate::casca::abrir_janela(app, Some(aba));
    }
}
