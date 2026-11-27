//! Casca residente: bandeja, janela sob demanda e início com o sistema.
//! O app roda sem janela; o webview só existe enquanto a janela está aberta.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use talk_core::ditado::Ditado;
use talk_core::pasta;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

const ABA_PADRAO: &str = "inicio";
/// Marca que o início com o sistema já foi decidido (ligado na 1ª execução): desligar depois vale.
const MARCA_AUTOSTART: &str = ".autostart-feito";

/// Aba pedida na linha de comando (`--aba=vocabulario` ou `--aba vocabulario`); só `[a-z0-9_-]`.
pub fn aba_dos_args(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let valor = match a.strip_prefix("--aba=") {
            Some(v) => Some(v.to_string()),
            None if a == "--aba" => it.next().cloned(),
            None => None,
        };
        if let Some(v) = valor {
            let v = v.to_lowercase();
            if !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Some(v);
            }
        }
    }
    None
}

/// Traz a janela `main`, criando-a se não existir. `aba` (ou "inicio" na criação) chega à tela
/// pela query string `?aba=` ao criar e pelo evento `aba` quando a janela já estava aberta.
pub fn abrir_janela(app: &AppHandle, aba: Option<String>) {
    let a = app.clone();
    // Criar janela fora da thread principal trava no Windows: sempre pela fila dela.
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = a.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            if let Some(aba) = aba {
                let _ = a.emit_to("main", "aba", aba);
            }
            return;
        }
        let url = format!("index.html?aba={}", aba.as_deref().unwrap_or(ABA_PADRAO));
        let r = WebviewWindowBuilder::new(&a, "main", WebviewUrl::App(url.into()))
            .title("RenanTalk")
            .inner_size(800.0, 580.0)
            .min_inner_size(640.0, 480.0)
            .center()
            .visible(false) // a tela pede `mostrar` quando já desenhou
            .build();
        match r {
            Ok(w) => {
                // rede de segurança: se a página não pedir para aparecer, mostra assim mesmo
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(2000));
                    let _ = w.show();
                });
            }
            Err(e) => pasta::log(&pasta::padrao(), &format!("janela: falhou ao abrir ({e})")),
        }
    });
}

/// Depois que a janela fecha, devolve ao sistema a memória que o webview deixou: `malloc_trim` no Linux (o heap da
/// glibc não encolhe sozinho, e o processo de rede do WebKit é encerrado) e `EmptyWorkingSet` no Windows (é o número que o Gerenciador de Tarefas mostra).
pub fn devolver_memoria(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2)); // o WebKit/WebView2 ainda está se desmontando
        #[cfg(target_os = "linux")]
        {
            unsafe { libc::malloc_trim(0) };
            if app.get_webview_window("main").is_none() { // reabriu nesse meio tempo: deixa quieto
                encerrar_rede_webkit();
            }
        }
        #[cfg(windows)]
        unsafe {
            let _ = &app;
            use windows::Win32::System::{ProcessStatus::K32EmptyWorkingSet, Threading::GetCurrentProcess};
            let _ = K32EmptyWorkingSet(GetCurrentProcess());
        }
    });
}

/// Linux: o WebKit deixa o `WebKitNetworkProcess` (~47 MB) vivo depois que a última janela fecha. Encerra-o; o WebKit
/// sobe outro na próxima janela. Só mexe em filhos diretos deste processo.
#[cfg(target_os = "linux")]
fn encerrar_rede_webkit() {
    let eu = std::process::id().to_string();
    let Ok(dir) = std::fs::read_dir("/proc") else { return };
    for e in dir.flatten() {
        let pid = e.file_name().to_string_lossy().to_string();
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let ppid = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)).unwrap_or("");
        let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        if ppid == eu && String::from_utf8_lossy(&cmd).contains("WebKitNetworkProcess") {
            if let Ok(n) = pid.parse::<i32>() {
                unsafe { libc::kill(n, libc::SIGTERM) };
            }
        }
    }
}

/// Liga o início com o sistema na 1ª execução (como o instalador do Python) e nunca mais mexe sozinho.
pub fn autostart_primeira_vez(app: &AppHandle, dir: &Path) {
    let marca = dir.join(MARCA_AUTOSTART);
    if marca.exists() {
        return;
    }
    match app.autolaunch().enable() {
        Ok(()) => {
            let _ = pasta::gravar_atomico(&marca, "1");
            pasta::log(dir, "iniciar com a sessão: ligado na primeira execução");
        }
        Err(e) => pasta::log(dir, &format!("iniciar com a sessão: não foi possível ligar ({e})")),
    }
}

/// Abre um arquivo ou endereço no programa padrão do sistema.
pub fn abrir_externo(destino: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer").arg(destino).creation_flags(0x0800_0000).spawn()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(destino).spawn();
    r.map(|_| ())
}

fn abrir_log(dir: &Path) {
    let arq: PathBuf = dir.join(pasta::ARQ_LOG);
    if !arq.exists() {
        return;
    }
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&arq).creation_flags(0x0800_0000).spawn()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(&arq).spawn();
    if let Err(e) = r {
        pasta::log(dir, &format!("ver log: falhou ({e})"));
    }
}

/// Ícone e menu da bandeja, iguais aos do Python. Sem bandeja no sistema o app segue sem ela.
pub fn bandeja(app: &AppHandle, ditado: Arc<Ditado>, dir: PathBuf) {
    let versao = app.package_info().version.to_string();
    let tecla = if cfg!(windows) { "Win" } else { "Super" };
    let inicio_rotulo = if cfg!(windows) { "Iniciar com o Windows" } else { "Iniciar com a sessão" };
    let r = (|| -> tauri::Result<()> {
        let inicio = CheckMenuItem::with_id(app, "inicio", inicio_rotulo, true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
        let menu = Menu::with_items(app, &[
            &MenuItem::with_id(app, "titulo", format!("RenanTalk   ·   v{versao}"), false, None::<&str>)?,
            &MenuItem::with_id(app, "dica", format!("Segure Ctrl + {tecla} para ditar"), false, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "config", "Configurações…", true, None::<&str>)?,
            &MenuItem::with_id(app, "colar", "Colar último ditado", true, None::<&str>)?,
            &MenuItem::with_id(app, "log", "Ver log", true, None::<&str>)?,
            &inicio,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "sair", "Sair do RenanTalk", true, None::<&str>)?,
        ])?;
        let inicio_item = inicio.clone();
        let mut b = TrayIconBuilder::with_id("renantalk")
            .tooltip(format!("RenanTalk v{versao} · segure Ctrl+{tecla} para ditar"))
            .menu(&menu)
            .on_menu_event(move |app, e| {
                pasta::log(&dir, &format!("bandeja: {}", e.id().as_ref()));
                match e.id().as_ref() {
                    "config" => abrir_janela(app, Some(ABA_PADRAO.into())),
                    "colar" => ditado.colar_ultimo(Duration::from_millis(350)), // espera o menu fechar e o foco voltar
                    "log" => abrir_log(&dir),
                    "inicio" => {
                        let ligar = inicio_item.is_checked().unwrap_or(false); // o clique já inverteu a marca
                        let l = app.autolaunch();
                        let r = if ligar { l.enable() } else { l.disable() };
                        if let Err(e) = r {
                            pasta::log(&dir, &format!("iniciar com a sessão: falhou ({e})"));
                            let _ = inicio_item.set_checked(l.is_enabled().unwrap_or(!ligar)); // volta ao estado real
                        } else {
                            let _ = pasta::gravar_atomico(&dir.join(MARCA_AUTOSTART), "1");
                        }
                    }
                    "sair" => app.exit(0),
                    _ => {}
                }
            });
        if let Some(icone) = app.default_window_icon() {
            b = b.icon(icone.clone());
        }
        b.build(app)?;
        Ok(())
    })();
    if let Err(e) = r {
        pasta::log(&pasta::padrao(), &format!("bandeja indisponível: {e}"));
    }
}

#[cfg(test)]
mod testes {
    use super::aba_dos_args;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn aba_por_igual_ou_espaco() {
        assert_eq!(aba_dos_args(&v(&["renantalk", "--aba=Vocabulario"])), Some("vocabulario".into()));
        assert_eq!(aba_dos_args(&v(&["renantalk", "--aba", "historico"])), Some("historico".into()));
    }

    #[test]
    fn sem_aba_ou_invalida() {
        assert_eq!(aba_dos_args(&v(&["renantalk"])), None);
        assert_eq!(aba_dos_args(&v(&["renantalk", "--aba"])), None);
        assert_eq!(aba_dos_args(&v(&["renantalk", "--aba=a&b=1"])), None); // nada de query solta na URL
    }
}
