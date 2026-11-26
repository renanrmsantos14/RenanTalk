//! Janela de configurações do RenanTalk (Tauri).
//!
//! Uso: renantalk-config <pid_do_ditado> <aba> <python> <pasta_do_app>
//! A tela é `app/ui/index.html` (embutida no binário); os comandos abaixo substituem o
//! servidor HTTP que o Python usava. O processo termina quando a janela fecha.
#![cfg_attr(windows, windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use tauri::Manager;

const LIMITE_VOCAB: usize = 850;
const REPO_URL: &str = "https://github.com/renanrmsantos14/RenanTalk";
const VOCAB_PADRAO: &[&str] = &[
    "Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI",
    "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp",
];

struct Ctx {
    pid: u32,
    aba: String,
    python: String,
    app: PathBuf,
}
static CTX: OnceLock<Ctx> = OnceLock::new();

fn ctx() -> &'static Ctx {
    CTX.get().expect("contexto")
}

fn pasta() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("RenanTalk")
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
            .join("RenanTalk")
    }
}

fn ler_chave() -> String {
    fs::read_to_string(pasta().join("chave.txt")).unwrap_or_default().trim().to_string()
}

fn mascarar(chave: &str) -> String {
    if chave.chars().count() > 8 {
        let ini: String = chave.chars().take(4).collect();
        let fim: String = chave.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{ini}{}{fim}", "•".repeat(10))
    } else {
        String::new()
    }
}

fn ler_vocabulario() -> Vec<String> {
    let arq = pasta().join("vocabulario.txt");
    if !arq.exists() {
        let _ = fs::create_dir_all(pasta());
        let _ = fs::write(&arq, VOCAB_PADRAO.join("\n") + "\n");
    }
    fs::read_to_string(arq)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

#[cfg(windows)]
fn memoria_mb(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        CloseHandle(h);
        (ok != 0).then(|| (c.PrivateUsage / 1_048_576) as u64)
    }
}

#[cfg(not(windows))]
fn memoria_mb(pid: u32) -> Option<u64> {
    let s = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let linha = s.lines().find(|l| l.starts_with("RssAnon:"))?;
    let kb: u64 = linha.split_whitespace().nth(1)?.parse().ok()?;
    Some((kb as f64 / 1024.0).round() as u64)
}

fn sem_janela(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

#[derive(Serialize)]
struct Estado {
    versao: String,
    vocab: Vec<String>,
    limite: usize,
    chave: String,
    memoria: Option<u64>,
    ditados_hoje: usize,
    verificacao: String,
    aba: String,
}

#[tauri::command]
fn estado() -> Estado {
    let c = ctx();
    let hoje = chrono::Local::now().format("%d/%m/%Y").to_string();
    let (mut ditados, mut verificacao) = (0, String::new());
    if let Ok(f) = fs::File::open(pasta().join("ditado.log")) {
        for linha in BufReader::new(f).lines().map_while(Result::ok) {
            if linha.starts_with(&hoje) && linha.contains("transcrito") {
                ditados += 1;
            }
            if let Some((_, resto)) = linha.split_once("atualização:") {
                let sit = resto.split_whitespace().next().unwrap_or("");
                let sit = match sit { "em_dia" => "em dia", "erro" => "falhou", s => s };
                verificacao = format!("{} · {sit}", linha.chars().take(16).collect::<String>());
            }
        }
    }
    Estado {
        versao: fs::read_to_string(c.app.join("VERSION")).map(|v| v.trim().to_string()).unwrap_or_else(|_| "0.0.0 · dev".into()),
        vocab: ler_vocabulario(),
        limite: LIMITE_VOCAB,
        chave: mascarar(&ler_chave()),
        memoria: if c.pid > 0 { memoria_mb(c.pid) } else { None },
        ditados_hoje: ditados,
        verificacao,
        aba: c.aba.clone(),
    }
}

#[tauri::command]
fn salvar_vocab(termos: Vec<String>) -> Value {
    let termos: Vec<String> = termos.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    let _ = fs::create_dir_all(pasta());
    let ok = fs::write(pasta().join("vocabulario.txt"), termos.join("\n") + "\n").is_ok();
    json!({ "ok": ok })
}

#[tauri::command]
async fn salvar_chave(chave: String) -> Value {
    let k = chave.trim().to_string();
    if !k.starts_with("gsk_") {
        return json!({ "ok": false, "erro": "A chave da Groq começa com gsk_." });
    }
    let resp = ureq::get("https://api.groq.com/openai/v1/models")
        .set("Authorization", &format!("Bearer {k}"))
        .set("User-Agent", "renantalk/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .call();
    match resp {
        Ok(_) => {
            let arq = pasta().join("chave.txt");
            let _ = fs::create_dir_all(pasta());
            if fs::File::create(&arq).and_then(|mut f| f.write_all(k.as_bytes())).is_err() {
                return json!({ "ok": false, "erro": "Não consegui gravar a chave." });
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&arq, fs::Permissions::from_mode(0o600)); // só o seu usuário lê
            }
            json!({ "ok": true, "chave": mascarar(&k) })
        }
        Err(ureq::Error::Status(401 | 403, _)) => json!({ "ok": false, "erro": "A Groq recusou essa chave." }),
        Err(ureq::Error::Status(c, _)) => json!({ "ok": false, "erro": format!("A Groq respondeu erro {c}.") }),
        Err(_) => json!({ "ok": false, "erro": "Sem conexão com a Groq. Tente de novo." }),
    }
}

/// Reaproveita o atualizador em Python (o mesmo do ciclo automático).
#[tauri::command]
async fn atualizar() -> Value {
    let c = ctx();
    let saida = sem_janela(
        Command::new(&c.python).arg(c.app.join("atualizar_cli.py")).arg(c.pid.to_string()).current_dir(&c.app),
    )
    .output();
    match saida {
        Ok(o) => serde_json::from_slice(&o.stdout).unwrap_or_else(|_| json!({ "situacao": "erro", "detalhe": "resposta inválida" })),
        Err(e) => json!({ "situacao": "erro", "detalhe": e.to_string() }),
    }
}

#[tauri::command]
fn abrir(alvo: String) -> Value {
    let destino = match alvo.as_str() {
        "log" => pasta().join("ditado.log").to_string_lossy().into_owned(),
        "repo" => REPO_URL.into(),
        "groq" => "https://console.groq.com/keys".into(),
        _ => return json!({ "ok": false }),
    };
    let r = if cfg!(windows) {
        sem_janela(Command::new("explorer").arg(&destino)).spawn()
    } else {
        Command::new("xdg-open").arg(&destino).spawn()
    };
    json!({ "ok": r.is_ok() })
}

#[tauri::command]
fn mostrar(janela: tauri::WebviewWindow) {
    let _ = janela.show();
    let _ = janela.set_focus();
}

#[tauri::command]
fn fechar(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    // Uma janela por vez: a porta fica presa enquanto esta instância vive.
    let Ok(_trava) = TcpListener::bind("127.0.0.1:47819") else { return };
    let args: Vec<String> = std::env::args().collect();
    // Aberta pelo atalho da barra (sem parâmetros): usa o que o ditado em execução gravou em ditado.json.
    let salvo: Value = fs::read_to_string(pasta().join("ditado.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let app_dir = args.get(4).map(PathBuf::from).or_else(|| salvo["app"].as_str().map(PathBuf::from)).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default()
    });
    let _ = CTX.set(Ctx {
        pid: args.get(1).and_then(|p| p.parse().ok()).or_else(|| salvo["pid"].as_u64().map(|p| p as u32)).unwrap_or(0),
        aba: args.get(2).cloned().unwrap_or_else(|| "voc".into()),
        python: args.get(3).cloned().or_else(|| salvo["python"].as_str().map(String::from))
            .unwrap_or_else(|| if cfg!(windows) { "py".into() } else { "python3".into() }),
        app: app_dir,
    });
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![estado, salvar_vocab, salvar_chave, atualizar, abrir, mostrar, fechar])
        .setup(|app| {
            // Mostra a janela na hora (como o RenanMail), já com o fundo do tema para não piscar branco;
            // o conteúdo aparece assim que o WebView2 termina de carregar.
            let j = app.get_webview_window("main").expect("janela");
            let escuro = j.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(true);
            let fundo = if escuro { tauri::window::Color(11, 16, 32, 255) } else { tauri::window::Color(255, 255, 255, 255) };
            let _ = j.set_background_color(Some(fundo));
            let _ = j.show();
            let _ = j.set_focus();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao abrir a janela");
}
