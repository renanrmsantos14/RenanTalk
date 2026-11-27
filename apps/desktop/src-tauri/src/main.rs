//! RenanTalk desktop: casca Tauri sobre o `talk-core`. Sobe sem janela, com bandeja, e roda o ditado.
//! A tela (webview, React em `apps/desktop/src`) é criada sob demanda e destruída ao fechar; a atualização está em `atualizar.rs`.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod atualizar;
mod casca;
mod plataforma;
mod tela;

use std::sync::Arc;
use tauri::Manager;
use std::time::Duration;
use talk_core::groq::{Groq, MODELO_VOZ};
use talk_core::ditado::Ditado;
use talk_core::{escrita, historico, pasta};

fn main() {
    let dir = pasta::padrao();
    let versao = env!("CARGO_PKG_VERSION");

    let app = tauri::Builder::default()
        // Primeiro plugin: a 2ª execução avisa a que já roda e sai aqui, antes de subir o ditado (sem 2º gancho de teclado).
        // Cobre o Windows (mutex nomeado pelo identifier, `br.com.renan.renantalk`, distinto do "RenanTalk" do Python,
        // então os dois coexistem) e o Linux (D-Bus).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| casca::abrir_janela(app, casca::aba_dos_args(&args))))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            tela::estado, tela::ajustes, tela::salvar_ajustes, tela::historico_listar, tela::historico_editar, tela::historico_excluir,
            tela::historico_apagar_tudo, tela::salvar_vocab, tela::inicio, tela::sugestao_resolver, tela::substituicoes_ler,
            tela::substituicoes_salvar, tela::salvar_chave, tela::abrir, tela::mostrar, tela::fechar, tela::procurar_atualizacao,
        ])
        .on_window_event(|janela, evento| {
            if let tauri::WindowEvent::Destroyed = evento {
                casca::devolver_memoria(janela.app_handle().clone());
            }
        })
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Tauri");
    // Não usa o `setup` do Tauri: sem janela o laço de eventos do GTK só o despacha quando algo o acorda
    // (visto no Xvfb: minutos sem rodar), e o ditado precisa subir sozinho.
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
    let h = app.handle();
    casca::autostart_primeira_vez(h, &dir); // antes do gancho: o marcador da ponte diz se o autostart ficou ligado
    let (a, d) = (h.clone(), dir.clone());
    plataforma::iniciar_gancho(ditado.clone(), dir.clone(), move || casca::marcar_subiu(&a, &d, versao));

    casca::bandeja(h, ditado.clone(), dir);
    app.manage(ditado);
    atualizar::ao_iniciar(h);
    atualizar::agendar(h.clone());
    app.run(|app, evento| match evento {
        // Sem janela aberta o app continua residente: só `exit` explícito (menu Sair) encerra.
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        tauri::RunEvent::Exit => {
            if let Some(d) = app.try_state::<Arc<Ditado>>() {
                d.encerrar(); // sair no meio do ditado não deixa os outros apps baixos
            }
        }
        _ => {}
    });
}
