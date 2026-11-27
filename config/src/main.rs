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

const MODIFICADORES: [&str; 4] = ["ctrl", "alt", "shift", "win"];

/// Mesma regra do Python (`ajustes.validar_atalho`): 2+ teclas sem repetir, modificadores e no máximo uma tecla.
fn validar_atalho(texto: &str) -> Option<String> {
    let partes: Vec<String> = texto
        .split('+')
        .map(|p| match p.trim().to_lowercase().as_str() {
            "super" | "windows" | "meta" => "win".to_string(),
            o => o.to_string(),
        })
        .collect();
    let mut unicos = partes.clone();
    unicos.sort();
    unicos.dedup();
    if partes.len() < 2 || unicos.len() != partes.len() {
        return None;
    }
    let mods: Vec<&str> = MODIFICADORES.iter().copied().filter(|m| partes.iter().any(|p| p == m)).collect();
    let teclas: Vec<&String> = partes.iter().filter(|p| !MODIFICADORES.contains(&p.as_str())).collect();
    let tecla_ok = |t: &str| {
        (t.len() == 1 && t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
            || t == "space"
            || (t.starts_with('f') && t[1..].parse::<u8>().map_or(false, |n| (1..=12).contains(&n)))
    };
    if mods.is_empty() || teclas.len() > 1 || teclas.first().map_or(false, |t| !tecla_ok(t)) {
        return None;
    }
    let mut saida: Vec<String> = mods.iter().map(|m| m.to_string()).collect();
    saida.extend(teclas.into_iter().cloned());
    Some(saida.join("+"))
}

fn ler_ajustes_arquivo() -> serde_json::Map<String, Value> {
    fs::read_to_string(pasta().join("ajustes.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Ajustes (padrões onde faltar ou for inválido) e a lista de microfones gravada pelo Python.
#[tauri::command]
fn ajustes() -> Value {
    let mut a = ler_ajustes_arquivo();
    let volume = a.get("volume").and_then(Value::as_f64).map_or(60, |v| v.clamp(0.0, 100.0) as i64);
    a.insert("volume".into(), json!(volume));
    if !a.get("microfone").map_or(false, Value::is_string) {
        a.insert("microfone".into(), json!(""));
    }
    if !a.get("idioma").and_then(Value::as_str).map_or(false, |i| ["pt", "auto", "en"].contains(&i)) {
        a.insert("idioma".into(), json!("pt"));
    }
    let atalho = a.get("atalho_ditar").and_then(Value::as_str).and_then(validar_atalho).unwrap_or_else(|| "ctrl+win".into());
    a.insert("atalho_ditar".into(), json!(atalho));
    let dispositivos = fs::read_to_string(pasta().join("dispositivos.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("dispositivos").cloned())
        .unwrap_or_else(|| json!([]));
    json!({ "ajustes": a, "dispositivos": dispositivos })
}

/// Grava só volume, microfone, idioma e atalho_ditar; as outras chaves do arquivo ficam como estão.
/// Escrita atômica: arquivo temporário e rename.
#[tauri::command]
fn salvar_ajustes(valores: serde_json::Map<String, Value>) -> Value {
    let mut a = ler_ajustes_arquivo();
    if let Some(v) = valores.get("volume").and_then(Value::as_f64) {
        a.insert("volume".into(), json!(v.clamp(0.0, 100.0).round() as i64));
    }
    if let Some(m) = valores.get("microfone").and_then(Value::as_str) {
        a.insert("microfone".into(), json!(m));
    }
    if let Some(i) = valores.get("idioma").and_then(Value::as_str) {
        if !["pt", "auto", "en"].contains(&i) {
            return json!({ "ok": false, "erro": "Idioma inválido." });
        }
        a.insert("idioma".into(), json!(i));
    }
    if let Some(t) = valores.get("atalho_ditar").and_then(Value::as_str) {
        match validar_atalho(t) {
            Some(ok) => a.insert("atalho_ditar".into(), json!(ok)),
            None => return json!({ "ok": false, "erro": "Atalho inválido: use pelo menos 2 teclas diferentes." }),
        };
    }
    let _ = fs::create_dir_all(pasta());
    let arq = pasta().join("ajustes.json");
    let tmp = pasta().join("ajustes.json.tmp");
    let texto = serde_json::to_string_pretty(&Value::Object(a)).unwrap_or_default();
    let ok = fs::write(&tmp, texto).and_then(|_| fs::rename(&tmp, &arq)).is_ok();
    json!({ "ok": ok })
}

// ---------- Histórico (historico.db; o Python grava, esta janela lê e edita) ----------
fn abrir_historico() -> Result<rusqlite::Connection, String> {
    let arq = pasta().join("historico.db");
    if !arq.exists() {
        return Err("vazio".into()); // ainda não houve ditado: lista vazia, sem criar banco
    }
    let con = rusqlite::Connection::open(arq).map_err(|e| e.to_string())?;
    con.busy_timeout(std::time::Duration::from_secs(2)).map_err(|e| e.to_string())?;
    Ok(con)
}

fn erro_historico(e: impl ToString) -> Value {
    json!({ "ok": false, "erro": "Não consegui acessar o histórico agora. Tente de novo.", "detalhe": e.to_string() })
}

/// Ditados do mais novo ao mais antigo; `busca` vazia lista tudo. LIKE simples (sem curingas do usuário).
#[tauri::command]
fn historico_listar(busca: String) -> Value {
    let con = match abrir_historico() {
        Ok(c) => c,
        Err(e) if e == "vazio" => return json!({ "ok": true, "itens": [] }),
        Err(e) => return erro_historico(e),
    };
    let like = format!("%{}%", busca.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let r = (|| -> rusqlite::Result<Vec<Value>> {
        let mut st = con.prepare(
            "SELECT id, criado_em, texto, original, app, modo, idioma, duracao_s, palavras, editado FROM ditado \
             WHERE texto LIKE ?1 ESCAPE '\\' ORDER BY criado_em DESC, id DESC LIMIT 500",
        )?;
        let linhas = st.query_map([&like], |l| {
            Ok(json!({
                "id": l.get::<_, i64>(0)?, "criado_em": l.get::<_, String>(1)?, "texto": l.get::<_, String>(2)?,
                "original": l.get::<_, String>(3)?, "app": l.get::<_, String>(4)?, "modo": l.get::<_, String>(5)?,
                "idioma": l.get::<_, String>(6)?, "duracao_s": l.get::<_, f64>(7)?, "palavras": l.get::<_, i64>(8)?,
                "editado": l.get::<_, i64>(9)? != 0,
            }))
        })?;
        linhas.collect()
    })();
    match r {
        Ok(itens) => json!({ "ok": true, "itens": itens }),
        Err(e) => erro_historico(e),
    }
}

/// Corrige o texto: marca editado e deixa `original` como está.
#[tauri::command]
fn historico_editar(id: i64, texto: String) -> Value {
    if texto.trim().is_empty() {
        return json!({ "ok": false, "erro": "O texto não pode ficar vazio." });
    }
    let palavras = texto.split_whitespace().count() as i64;
    match abrir_historico().and_then(|c| {
        c.execute("UPDATE ditado SET texto=?1, palavras=?2, editado=1 WHERE id=?3", rusqlite::params![texto, palavras, id])
            .map_err(|e| e.to_string())
    }) {
        Ok(n) => json!({ "ok": n > 0 }),
        Err(e) => erro_historico(e),
    }
}

#[tauri::command]
fn historico_excluir(id: i64) -> Value {
    match abrir_historico().and_then(|c| c.execute("DELETE FROM ditado WHERE id=?1", [id]).map_err(|e| e.to_string())) {
        Ok(n) => json!({ "ok": n > 0 }),
        Err(e) => erro_historico(e),
    }
}

#[tauri::command]
fn historico_apagar_tudo() -> Value {
    match abrir_historico() {
        Err(e) if e == "vazio" => json!({ "ok": true }),
        r => match r.and_then(|c| c.execute("DELETE FROM ditado", []).map_err(|e| e.to_string())) {
            Ok(_) => json!({ "ok": true }),
            Err(e) => erro_historico(e),
        },
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
    let app_dir = args.get(4).map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default()
    });
    let _ = CTX.set(Ctx {
        pid: args.get(1).and_then(|p| p.parse().ok()).unwrap_or(0),
        aba: args.get(2).cloned().unwrap_or_else(|| "voc".into()),
        python: args.get(3).cloned().unwrap_or_else(|| if cfg!(windows) { "python".into() } else { "python3".into() }),
        app: app_dir,
    });
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![estado, ajustes, salvar_ajustes, historico_listar, historico_editar, historico_excluir, historico_apagar_tudo, salvar_vocab, salvar_chave, atualizar, abrir, mostrar, fechar])
        .setup(|app| {
            // rede de segurança: se a página não pedir para aparecer, mostra assim mesmo
            let j = app.get_webview_window("main").expect("janela");
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let _ = j.show();
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao abrir a janela");
}
