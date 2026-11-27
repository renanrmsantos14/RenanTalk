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
            || (t.len() >= 2 && t.starts_with('f') && !t[1..].starts_with('0') && t[1..].chars().all(|c| c.is_ascii_digit())
                && t[1..].parse::<u8>().map_or(false, |n| (1..=12).contains(&n)))
    };
    // só Shift intercepta a digitação comum (Shift+A); precisa de Ctrl, Alt ou Win
    if mods.iter().all(|m| *m == "shift") || teclas.len() > 1 || teclas.first().map_or(false, |t| !tecla_ok(t)) {
        return None;
    }
    let mut saida: Vec<String> = mods.iter().map(|m| m.to_string()).collect();
    saida.extend(teclas.into_iter().cloned());
    let junto = saida.join("+");
    // atalhos que o próprio app usa (o colar manda Ctrl+V) ou que todo programa usa
    if ATALHOS_PROIBIDOS.contains(&junto.as_str()) {
        return None;
    }
    Some(junto)
}

const ATALHOS_PROIBIDOS: [&str; 7] = ["ctrl+c", "ctrl+v", "ctrl+x", "ctrl+z", "ctrl+a", "ctrl+s", "alt+f4"];

/// Um atalho contido no outro (ctrl+win e ctrl+win+v) faz um disparar no lugar do outro.
fn atalhos_conflitam(a: &str, b: &str) -> bool {
    let ca: Vec<&str> = a.split('+').collect();
    let cb: Vec<&str> = b.split('+').collect();
    ca.iter().all(|x| cb.contains(x)) || cb.iter().all(|x| ca.contains(x))
}

/// Escrita atômica: arquivo temporário e rename, para quem lê (o Python) nunca ver o arquivo pela metade.
fn gravar_atomico(arq: &std::path::Path, texto: &str) -> bool {
    let _ = fs::create_dir_all(pasta());
    let mut tmp = arq.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // No Windows o rename falha se o Python estiver lendo o arquivo naquele instante: tenta de novo.
    fs::write(&tmp, texto).is_ok()
        && (0..3).any(|i| {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            fs::rename(&tmp, arq).is_ok()
        })
}

fn ler_ajustes_arquivo() -> serde_json::Map<String, Value> {
    fs::read_to_string(pasta().join("ajustes.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

const CORES_LUZ: [&str; 4] = ["azul", "verde", "roxo", "ambar"];
const ESPESSURAS: [&str; 3] = ["fina", "media", "grossa"];
const POSICOES: [&str; 2] = ["embaixo", "topo"];
const TEMAS: [&str; 3] = ["sistema", "claro", "escuro"];
const MODOS: [&str; 4] = ["literal", "revisado", "formal", "whatsapp"];
/// (chave, padrão, para que serve): nenhum atalho pode repetir nem conter outro (mesma regra do Python).
const ATALHOS: [(&str, &str, &str); 3] = [
    ("atalho_ditar", "ctrl+win", "ditar"),
    ("atalho_colar", "alt+win+v", "colar o último ditado"),
    ("atalho_traduzir", "shift+win", "traduzir"),
];
const COMANDOS: [&str; 6] = ["nova_linha", "ponto_final", "virgula", "interrogacao", "abre_parenteses", "fecha_parenteses"];

/// Liga/desliga dos comandos falados: o que faltar ou não for booleano fica ligado; chaves extras ficam.
fn comandos_normalizados(bruto: Option<&Value>) -> serde_json::Map<String, Value> {
    let mut c = bruto.and_then(Value::as_object).cloned().unwrap_or_default();
    for chave in COMANDOS {
        if !c.get(chave).map_or(false, Value::is_boolean) {
            c.insert(chave.into(), json!(true));
        }
    }
    c
}

/// Luz com os quatro campos válidos (o que faltar ou for inválido volta ao padrão); chaves extras ficam.
fn luz_normalizada(bruto: Option<&Value>) -> serde_json::Map<String, Value> {
    let mut luz = bruto.and_then(Value::as_object).cloned().unwrap_or_default();
    if !luz.get("ligada").map_or(false, Value::is_boolean) {
        luz.insert("ligada".into(), json!(true));
    }
    for (chave, validos, padrao) in [("cor", &CORES_LUZ[..], "azul"), ("espessura", &ESPESSURAS[..], "media"), ("posicao", &POSICOES[..], "embaixo")] {
        if !luz.get(chave).and_then(Value::as_str).map_or(false, |v| validos.contains(&v)) {
            luz.insert(chave.into(), json!(padrao));
        }
    }
    luz
}

/// Modos por aplicativo: nome em minúsculas, sem espaço nas pontas e só com modo válido.
/// Ausente: os padrões; presente mas sem ser objeto: vazio (igual ao Python).
fn modos_apps_normalizados(bruto: Option<&Value>) -> serde_json::Map<String, Value> {
    let Some(bruto) = bruto else {
        return [("outlook", "formal"), ("olk", "formal"), ("whatsapp", "whatsapp"), ("code", "literal")]
            .into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    };
    let Some(m) = bruto.as_object() else { return serde_json::Map::new() };
    m.iter()
        .filter_map(|(k, v)| {
            let k = k.trim().to_lowercase();
            let v = v.as_str().filter(|v| MODOS.contains(v))?;
            (!k.is_empty()).then(|| (k, json!(v)))
        })
        .collect()
}

/// Ajustes (padrões onde faltar ou for inválido) e a lista de microfones gravada pelo Python.
#[tauri::command]
fn ajustes() -> Value {
    let mut a = ler_ajustes_arquivo();
    for (chave, padrao) in [("volume", 60), ("volume_outros", 30)] {
        let v = a.get(chave).and_then(Value::as_f64).map_or(padrao, |v| v.clamp(0.0, 100.0) as i64);
        a.insert(chave.into(), json!(v));
    }
    if !a.get("microfone").map_or(false, Value::is_string) {
        a.insert("microfone".into(), json!(""));
    }
    if !a.get("idioma").and_then(Value::as_str).map_or(false, |i| ["pt", "auto", "en"].contains(&i)) {
        a.insert("idioma".into(), json!("pt"));
    }
    // cada atalho é conferido contra os anteriores; inválido ou em conflito volta ao padrão, e "" se até o padrão conflitar
    let mut validos: Vec<String> = vec![];
    for (chave, padrao, _) in ATALHOS {
        let livre = |t: &String| !validos.iter().any(|o| atalhos_conflitam(t, o));
        let t = a.get(chave).and_then(Value::as_str).and_then(validar_atalho).filter(livre)
            .or_else(|| Some(padrao.to_string()).filter(livre)).unwrap_or_default();
        if !t.is_empty() {
            validos.push(t.clone());
        }
        a.insert(chave.into(), json!(t));
    }
    if !a.get("modo").and_then(Value::as_str).map_or(false, |m| MODOS.contains(&m)) {
        a.insert("modo".into(), json!("literal"));
    }
    let apps = modos_apps_normalizados(a.get("modos_apps"));
    a.insert("modos_apps".into(), Value::Object(apps));
    let luz = luz_normalizada(a.get("luz"));
    a.insert("luz".into(), Value::Object(luz));
    let comandos = comandos_normalizados(a.get("comandos"));
    a.insert("comandos".into(), Value::Object(comandos));
    if !a.get("tema").and_then(Value::as_str).map_or(false, |t| TEMAS.contains(&t)) {
        a.insert("tema".into(), json!("sistema"));
    }
    for (chave, padrao) in [("notificacoes", true), ("abaixar_outros", false)] {
        if !a.get(chave).map_or(false, Value::is_boolean) {
            a.insert(chave.into(), json!(padrao));
        }
    }
    let dispositivos = fs::read_to_string(pasta().join("dispositivos.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("dispositivos").cloned())
        .unwrap_or_else(|| json!([]));
    json!({ "ajustes": a, "dispositivos": dispositivos })
}

/// Grava só as chaves editáveis na janela; as outras chaves do arquivo ficam como estão.
/// Escrita atômica: arquivo temporário e rename.
#[tauri::command]
fn salvar_ajustes(valores: serde_json::Map<String, Value>) -> Value {
    let mut a = ler_ajustes_arquivo();
    for chave in ["volume", "volume_outros"] {
        if let Some(v) = valores.get(chave).and_then(Value::as_f64) {
            a.insert(chave.into(), json!(v.clamp(0.0, 100.0).round() as i64));
        }
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
    if let Some(l) = valores.get("luz") {
        let Some(novo) = l.as_object() else { return json!({ "ok": false, "erro": "Ajuste da luz inválido." }) };
        let mut luz = luz_normalizada(a.get("luz"));
        for (chave, v) in novo {
            let valido = match chave.as_str() {
                "ligada" => v.is_boolean(),
                "cor" => v.as_str().map_or(false, |x| CORES_LUZ.contains(&x)),
                "espessura" => v.as_str().map_or(false, |x| ESPESSURAS.contains(&x)),
                "posicao" => v.as_str().map_or(false, |x| POSICOES.contains(&x)),
                _ => false,
            };
            if !valido {
                return json!({ "ok": false, "erro": "Ajuste da luz inválido." });
            }
            luz.insert(chave.clone(), v.clone());
        }
        a.insert("luz".into(), Value::Object(luz));
    }
    if let Some(novos) = valores.get("comandos") {
        let Some(novos) = novos.as_object() else { return json!({ "ok": false, "erro": "Ajuste dos comandos inválido." }) };
        let mut comandos = comandos_normalizados(a.get("comandos"));
        for (chave, v) in novos {
            if !COMANDOS.contains(&chave.as_str()) || !v.is_boolean() {
                return json!({ "ok": false, "erro": "Ajuste dos comandos inválido." });
            }
            comandos.insert(chave.clone(), v.clone());
        }
        a.insert("comandos".into(), Value::Object(comandos));
    }
    if let Some(t) = valores.get("tema").and_then(Value::as_str) {
        if !TEMAS.contains(&t) {
            return json!({ "ok": false, "erro": "Tema inválido." });
        }
        a.insert("tema".into(), json!(t));
    }
    for chave in ["notificacoes", "abaixar_outros"] {
        if let Some(n) = valores.get(chave).and_then(Value::as_bool) {
            a.insert(chave.into(), json!(n));
        }
    }
    if let Some(m) = valores.get("modo").and_then(Value::as_str) {
        if !MODOS.contains(&m) {
            return json!({ "ok": false, "erro": "Modo de escrita inválido." });
        }
        a.insert("modo".into(), json!(m));
    }
    if let Some(m) = valores.get("modos_apps") {
        if !m.is_object() {
            return json!({ "ok": false, "erro": "Modos por aplicativo inválidos." });
        }
        a.insert("modos_apps".into(), Value::Object(modos_apps_normalizados(Some(m))));
    }
    // Um atalho não pode repetir nem conter outro. Atalho recusado não impede gravar o resto (volume, microfone...):
    // grava e devolve o erro.
    let mut erro: Option<String> = None;
    for (chave, _, _) in ATALHOS {
        let Some(t) = valores.get(chave).and_then(Value::as_str) else { continue };
        let Some(ok) = validar_atalho(t) else {
            erro.get_or_insert_with(|| "Atalho inválido: use Ctrl, Alt ou Win com mais uma tecla, e não Ctrl+C, Ctrl+V e parecidos.".into());
            continue;
        };
        let conflito = ATALHOS.iter().filter(|(outra, _, _)| *outra != chave).find_map(|(outra, padrao, para_que)| {
            let da_outra = valores.get(*outra).and_then(Value::as_str).and_then(validar_atalho)
                .or_else(|| a.get(*outra).and_then(Value::as_str).and_then(validar_atalho))
                .unwrap_or_else(|| padrao.to_string());
            atalhos_conflitam(&ok, &da_outra)
                .then(|| format!("Esse atalho se confunde com o de {para_que} ({da_outra}). Escolha outro."))
        });
        if let Some(e) = conflito {
            erro.get_or_insert(e);
            continue;
        }
        a.insert(chave.into(), json!(ok));
    }
    let texto = serde_json::to_string_pretty(&Value::Object(a)).unwrap_or_default();
    let gravou = gravar_atomico(&pasta().join("ajustes.json"), &texto);
    match (gravou, erro) {
        (false, _) => json!({ "ok": false, "erro": "Não consegui salvar os ajustes." }),
        (true, Some(e)) => json!({ "ok": false, "erro": e }),
        (true, None) => json!({ "ok": true }),
    }
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
        r => match r.and_then(|c| {
            c.execute("DELETE FROM ditado", []).map_err(|e| e.to_string())?;
            let _ = c.execute("DELETE FROM sugestao", []); // os termos sugeridos vêm dos ditados: somem junto (a tabela pode não existir)
            Ok(())
        }) {
            Ok(()) => json!({ "ok": true }),
            Err(e) => erro_historico(e),
        },
    }
}

#[tauri::command]
fn salvar_vocab(termos: Vec<String>) -> Value {
    let termos: Vec<String> = termos.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    json!({ "ok": gravar_atomico(&pasta().join("vocabulario.txt"), &(termos.join("\n") + "\n")) })
}

// ---------- Início (resumo dos ditados e sugestões de termos) ----------
/// Palavras por minuto de quem digita: base da estimativa de tempo poupado.
const PALAVRAS_POR_MIN: f64 = 40.0;

/// Resumo do painel: palavras hoje e nos últimos 7 dias, tempo poupado (palavras ÷ 40 por minuto, menos o tempo
/// falado, nunca negativo), gráfico por dia, último ditado e sugestões novas fora do vocabulário.
fn resumo(con: &rusqlite::Connection, hoje: chrono::NaiveDate, vocab: &[String]) -> rusqlite::Result<Value> {
    let dias: Vec<String> = (0..7u64).rev().map(|i| (hoje - chrono::Days::new(i)).format("%Y-%m-%d").to_string()).collect();
    let mut st = con.prepare("SELECT substr(criado_em, 1, 10), SUM(palavras), SUM(duracao_s) FROM ditado WHERE criado_em >= ?1 GROUP BY 1")?;
    let linhas = st.query_map([&dias[0]], |l| Ok((l.get::<_, String>(0)?, l.get::<_, i64>(1)?, l.get::<_, f64>(2)?)))?;
    let por_dia: std::collections::HashMap<String, (i64, f64)> = linhas.filter_map(Result::ok).map(|(d, p, s)| (d, (p, s))).collect();
    let palavras_dia = |d: &String| por_dia.get(d).map_or(0, |v| v.0);
    let semana: i64 = dias.iter().map(palavras_dia).sum();
    let falado_s: f64 = dias.iter().filter_map(|d| por_dia.get(d)).map(|v| v.1).sum();
    let poupado_s = (semana as f64 / PALAVRAS_POR_MIN * 60.0 - falado_s).max(0.0).round();
    let total: i64 = con.query_row("SELECT COUNT(*) FROM ditado", [], |l| l.get(0))?;
    let ultimo = con
        .query_row("SELECT id, criado_em, texto, app FROM ditado ORDER BY criado_em DESC, id DESC LIMIT 1", [], |l| {
            Ok(json!({ "id": l.get::<_, i64>(0)?, "criado_em": l.get::<_, String>(1)?, "texto": l.get::<_, String>(2)?, "app": l.get::<_, String>(3)? }))
        })
        .ok();
    // banco sem a tabela (criado antes dela): sem sugestões, sem erro
    let conhecidos: Vec<String> = vocab.iter().map(|t| t.to_lowercase()).collect();
    let sugestoes: Vec<Value> = (|| -> rusqlite::Result<Vec<Value>> {
        let mut st = con.prepare("SELECT termo, motivo, contagem FROM sugestao WHERE estado = 'nova' ORDER BY contagem DESC, termo LIMIT 30")?;
        let l = st.query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?, l.get::<_, i64>(2)?)))?;
        Ok(l.filter_map(Result::ok)
            .filter(|(t, _, _)| !conhecidos.contains(&t.to_lowercase()))
            .take(8)
            .map(|(t, m, c)| json!({ "termo": t, "motivo": m, "contagem": c }))
            .collect())
    })()
    .unwrap_or_default();
    Ok(json!({
        "ok": true, "vazio": total == 0, "hoje": palavras_dia(&dias[6]), "semana": semana, "poupado_s": poupado_s,
        "dias": dias.iter().map(|d| json!({ "dia": d, "palavras": palavras_dia(d) })).collect::<Vec<_>>(),
        "ultimo": ultimo, "sugestoes": sugestoes,
    }))
}

#[tauri::command]
fn inicio() -> Value {
    let hoje = chrono::Local::now().date_naive();
    match abrir_historico() {
        Err(e) if e == "vazio" => json!({ "ok": true, "vazio": true, "hoje": 0, "semana": 0, "poupado_s": 0, "dias": [], "ultimo": null, "sugestoes": [] }),
        Err(e) => erro_historico(e),
        Ok(con) => resumo(&con, hoje, &ler_vocabulario()).unwrap_or_else(erro_historico),
    }
}

/// Aceitar acrescenta o termo ao vocabulario.txt (sem duplicar, mantendo comentários) e marca `aceita`;
/// ignorar só marca `ignorada`. Devolve o vocabulário atual quando aceita.
#[tauri::command]
fn sugestao_resolver(termo: String, acao: String) -> Value {
    let termo = termo.trim().to_string();
    if termo.is_empty() || !["aceitar", "ignorar"].contains(&acao.as_str()) {
        return json!({ "ok": false, "erro": "Pedido inválido." });
    }
    let aceitar = acao == "aceitar";
    if aceitar && !ler_vocabulario().iter().any(|t| t.to_lowercase() == termo.to_lowercase()) {
        let arq = pasta().join("vocabulario.txt");
        let mut texto = fs::read_to_string(&arq).unwrap_or_default();
        if !texto.is_empty() && !texto.ends_with('\n') {
            texto.push('\n');
        }
        texto.push_str(&format!("{termo}\n"));
        if !gravar_atomico(&arq, &texto) {
            return json!({ "ok": false, "erro": "Não consegui gravar o vocabulário." });
        }
    }
    let marcado = match abrir_historico() {
        Err(e) if e == "vazio" => Ok(()),
        r => r.and_then(|c| {
            c.execute_batch("CREATE TABLE IF NOT EXISTS sugestao(termo TEXT PRIMARY KEY, motivo TEXT NOT NULL DEFAULT '', contagem INTEGER NOT NULL DEFAULT 0, estado TEXT NOT NULL DEFAULT 'nova')")
                .and_then(|_| c.execute(
                    "INSERT INTO sugestao(termo, estado) VALUES(?1, ?2) ON CONFLICT(termo) DO UPDATE SET estado = excluded.estado",
                    rusqlite::params![termo, if aceitar { "aceita" } else { "ignorada" }],
                ))
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
    };
    match marcado {
        Ok(()) => json!({ "ok": true, "vocab": ler_vocabulario() }),
        Err(e) => erro_historico(e),
    }
}

// ---------- Substituições (substituicoes.txt: `quando eu disser => escreva`, uma por linha) ----------
#[derive(Serialize, serde::Deserialize)]
struct Regra {
    quando: String,
    escreva: String,
}

/// Mesmo formato do Python (`substituicoes.ler_regras`); o `\n` do lado direito segue como está no arquivo.
fn parse_regras(texto: &str) -> Vec<Regra> {
    texto
        .lines()
        .filter_map(|l| {
            let (q, e) = l.split_once("=>")?;
            let (q, e) = (q.trim(), e.trim());
            (!q.is_empty() && !q.starts_with('#') && !e.is_empty()).then(|| Regra { quando: q.into(), escreva: e.into() })
        })
        .collect()
}

#[tauri::command]
fn substituicoes_ler() -> Value {
    let regras = parse_regras(&fs::read_to_string(pasta().join("substituicoes.txt")).unwrap_or_default());
    json!({ "ok": true, "regras": regras })
}

/// Reescreve o arquivo inteiro com as regras recebidas (comentários do arquivo não são mantidos).
#[tauri::command]
fn substituicoes_salvar(regras: Vec<Regra>) -> Value {
    if regras.len() > 200 {
        return json!({ "ok": false, "erro": "Cabem até 200 regras." });
    }
    let mut vistas: Vec<String> = Vec::new();
    let mut texto = String::new();
    for r in &regras {
        let (q, e) = (r.quando.trim(), r.escreva.trim().replace("\r\n", "\\n").replace('\n', "\\n"));
        if q.is_empty() || e.is_empty() || q.contains("=>") || q.contains('\n') || q.starts_with('#') || q.chars().count() > 200 || e.chars().count() > 500 {
            return json!({ "ok": false, "erro": "Cada regra precisa de “quando eu disser” e “escreva”, sem “=>” nem # no começo." });
        }
        let chave = q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if vistas.contains(&chave) {
            return json!({ "ok": false, "erro": format!("Já existe uma regra para “{q}”.") });
        }
        vistas.push(chave);
        texto.push_str(&format!("{q} => {e}\n"));
    }
    if gravar_atomico(&pasta().join("substituicoes.txt"), &texto) {
        json!({ "ok": true })
    } else {
        json!({ "ok": false, "erro": "Não consegui salvar as regras." })
    }
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

/// Aberto sozinho (sem o PID do ditado), sobe o ditado também; o mutex dele evita duplicata.
fn iniciar_ditado(app: &std::path::Path) {
    let script = app.join("ditado.pyw");
    if !script.exists() {
        return;
    }
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("pyw");
        c.arg("-3");
        c
    } else {
        Command::new("python3")
    };
    let _ = cmd.arg(&script).current_dir(app).spawn();
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
        aba: args.get(2).cloned().unwrap_or_else(|| "inicio".into()),
        python: args.get(3).cloned().unwrap_or_else(|| if cfg!(windows) { "python".into() } else { "python3".into() }),
        app: app_dir,
    });
    if ctx().pid == 0 {
        iniciar_ditado(&ctx().app);
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![estado, ajustes, salvar_ajustes, historico_listar, historico_editar, historico_excluir, historico_apagar_tudo, salvar_vocab, inicio, sugestao_resolver, substituicoes_ler, substituicoes_salvar, salvar_chave, atualizar, abrir, mostrar, fechar])
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

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn atalho_mesma_regra_do_python() {
        assert_eq!(validar_atalho("Super+Ctrl").as_deref(), Some("ctrl+win"));
        assert_eq!(validar_atalho("alt+win+v").as_deref(), Some("alt+win+v"));
        assert_eq!(validar_atalho("ctrl+f12").as_deref(), Some("ctrl+f12"));
        for ruim in ["ctrl", "d+e", "shift+a", "shift+space", "ctrl+v", "V+Ctrl", "alt+f4", "ctrl+f01", "ctrl+f+1", "ctrl+f13"] {
            assert_eq!(validar_atalho(ruim), None, "{ruim}");
        }
    }

    #[test]
    fn atalho_contido_no_outro_conflita() {
        assert!(atalhos_conflitam("ctrl+win", "ctrl+win+v"));
        assert!(atalhos_conflitam("ctrl+win+v", "ctrl+win"));
        assert!(!atalhos_conflitam("ctrl+win", "alt+win+v"));
    }

    fn banco(linhas: &[(&str, &str, i64, f64)]) -> rusqlite::Connection {
        let con = rusqlite::Connection::open_in_memory().unwrap();
        con.execute_batch(
            "CREATE TABLE ditado(id INTEGER PRIMARY KEY, criado_em TEXT NOT NULL, texto TEXT NOT NULL, original TEXT NOT NULL DEFAULT '', app TEXT NOT NULL DEFAULT '', palavras INTEGER NOT NULL DEFAULT 0, duracao_s REAL NOT NULL DEFAULT 0);",
        )
        .unwrap();
        for (quando, texto, palavras, dur) in linhas {
            con.execute("INSERT INTO ditado(criado_em, texto, palavras, duracao_s) VALUES(?1,?2,?3,?4)", rusqlite::params![quando, texto, palavras, dur]).unwrap();
        }
        con
    }

    #[test]
    fn resumo_conta_hoje_semana_e_poupado() {
        let hoje = chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        // 80 palavras em 20 s hoje + 40 em 10 s há 6 dias; 500 palavras há 7 dias ficam de fora
        let con = banco(&[("2026-11-26T09:00:00", "a", 80, 20.0), ("2026-11-20T10:00:00", "b", 40, 10.0), ("2026-11-19T10:00:00", "c", 500, 5.0)]);
        let r = resumo(&con, hoje, &[]).unwrap();
        assert_eq!((r["hoje"].as_i64(), r["semana"].as_i64(), r["vazio"].as_bool()), (Some(80), Some(120), Some(false)));
        assert_eq!(r["poupado_s"].as_f64(), Some(120.0 / 40.0 * 60.0 - 30.0));
        assert_eq!(r["dias"].as_array().unwrap().len(), 7);
        assert_eq!(r["dias"][0], json!({ "dia": "2026-11-20", "palavras": 40 }));
        assert_eq!(r["ultimo"]["texto"], "a");
    }

    #[test]
    fn resumo_poupado_nunca_negativo_e_banco_sem_tabela_de_sugestao() {
        let hoje = chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let con = banco(&[("2026-11-26T09:00:00", "oi", 2, 60.0)]);
        let r = resumo(&con, hoje, &[]).unwrap();
        assert_eq!(r["poupado_s"].as_f64(), Some(0.0));
        assert_eq!(r["sugestoes"], json!([]));
        assert_eq!(resumo(&banco(&[]), hoje, &[]).unwrap()["vazio"], true);
    }

    #[test]
    fn sugestoes_so_novas_e_fora_do_vocabulario() {
        let hoje = chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let con = banco(&[]);
        con.execute_batch(
            "CREATE TABLE sugestao(termo TEXT PRIMARY KEY, motivo TEXT NOT NULL DEFAULT '', contagem INTEGER NOT NULL DEFAULT 0, estado TEXT NOT NULL DEFAULT 'nova');
             INSERT INTO sugestao VALUES('Marcondes','frequencia',7,'nova'),('CTF','frequencia',6,'nova'),('Ana','frequencia',9,'ignorada'),('Dataverse','frequencia',5,'nova');",
        )
        .unwrap();
        let r = resumo(&con, hoje, &["dataverse".to_string()]).unwrap();
        let termos: Vec<&str> = r["sugestoes"].as_array().unwrap().iter().map(|s| s["termo"].as_str().unwrap()).collect();
        assert_eq!(termos, ["Marcondes", "CTF"]);
    }

    #[test]
    fn regras_ida_e_volta_do_arquivo() {
        let r = parse_regras("# c\nbom dia => Bom dia, tudo bem?\nlinha => a\\nb\nsem seta\n => x\ny =>\n");
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].quando.as_str(), r[0].escreva.as_str()), ("bom dia", "Bom dia, tudo bem?"));
        assert_eq!(r[1].escreva, "a\\nb");
    }

    #[test]
    fn modos_por_app_normalizados() {
        let m = modos_apps_normalizados(Some(&json!({" Slack ": "whatsapp", "x": "poesia", "": "formal"})));
        assert_eq!(Value::Object(m), json!({"slack": "whatsapp"}));
        assert_eq!(modos_apps_normalizados(None).get("outlook"), Some(&json!("formal")));
        assert!(modos_apps_normalizados(Some(&json!([]))).is_empty());  // igual ao Python
    }

    #[test]
    fn padrao_do_traduzir_nao_conflita_com_os_outros() {
        let (d, c, t) = (ATALHOS[0].1, ATALHOS[1].1, ATALHOS[2].1);
        assert!(!atalhos_conflitam(t, d) && !atalhos_conflitam(t, c) && validar_atalho(t).as_deref() == Some(t));
    }

    #[test]
    fn comandos_faltando_ficam_ligados() {
        let c = comandos_normalizados(Some(&json!({ "nova_linha": false, "virgula": "x", "extra": 1 })));
        assert_eq!((c["nova_linha"].as_bool(), c["virgula"].as_bool(), c["ponto_final"].as_bool(), c["extra"].as_i64()), (Some(false), Some(true), Some(true), Some(1)));
        assert_eq!(comandos_normalizados(None).len(), 6);
    }
}
