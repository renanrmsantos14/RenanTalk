//! RenanTalk desktop: casca Tauri sobre o `talk-core`. Sobe sem janela e roda o ditado.
//! Luz, bandeja, atualização e a tela entram nas próximas tarefas.

mod plataforma;

use std::sync::Arc;
use std::time::Duration;
use talk_core::groq::{Groq, MODELO_VOZ};
use talk_core::{escrita, historico, pasta};
use talk_core::ditado::Ditado;

fn main() {
    let dir = pasta::padrao();
    let versao = env!("CARGO_PKG_VERSION");
    #[cfg(windows)]
    if !plataforma::instancia_unica() { // mutex "RenanTalk-Rust": o Python usa "RenanTalk" e pode estar rodando junto
        pasta::log(&dir, "outra instância do RenanTalk já está rodando: saindo");
        return;
    }
    pasta::log(&dir, &format!("iniciado v{versao} ({})", std::env::consts::OS));
    let groq = Arc::new(Groq::novo(&dir, versao));
    let ditado = Ditado::novo(dir.clone(), plataforma::nova(&dir), groq.clone(), groq.clone());

    let g = groq.clone();
    std::thread::spawn(move || g.conferir_modelos(&[MODELO_VOZ, escrita::MODELO]));
    let d = dir.clone();
    std::thread::spawn(move || loop { // apaga o que passou da retenção ao iniciar e 1x por dia
        historico::limpar_antigos(&d, historico::RETENCAO_DIAS);
        std::thread::sleep(Duration::from_secs(24 * 3600));
    });
    plataforma::iniciar_gancho(ditado.clone(), dir);

    let app = tauri::Builder::default()
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Tauri");
    app.run(move |_app, evento| match evento {
        // Sem janela aberta o app continua residente: só `exit` explícito encerra.
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        tauri::RunEvent::Exit => ditado.encerrar(), // sair no meio do ditado não deixa os outros apps baixos
        _ => {}
    });
}
