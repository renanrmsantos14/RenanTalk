//! Ajustes do RenanTalk (`ajustes.json`), porta de `app/ajustes.py`: padrões, normalização, atalhos, leitura
//! com cache e gravação. Chaves desconhecidas são preservadas; valor inválido volta ao padrão.

use crate::pasta;
use serde_json::{json, Map, Value};
use std::io;
use std::path::Path;
use std::time::SystemTime;

pub const COMANDOS: [&str; 6] = ["nova_linha", "ponto_final", "virgula", "interrogacao", "abre_parenteses", "fecha_parenteses"];
/// Nenhum pode repetir nem conter outro.
pub const ATALHOS: [&str; 3] = ["atalho_ditar", "atalho_colar", "atalho_traduzir"];
pub const IDIOMAS: [&str; 3] = ["pt", "auto", "en"];
pub const MODIFICADORES: [&str; 4] = ["ctrl", "alt", "shift", "win"];
pub const ATALHOS_PROIBIDOS: [&str; 8] = ["ctrl+c", "ctrl+v", "ctrl+x", "ctrl+z", "ctrl+a", "ctrl+s", "alt+f4", "alt+space"];
/// "literal" mais os modos de IA de `escrita::INSTRUCOES`, menos "ingles" (só pelo atalho de traduzir).
pub const MODOS: [&str; 4] = ["literal", "revisado", "formal", "whatsapp"];
pub const TEMAS: [&str; 3] = ["sistema", "claro", "escuro"];
pub const POSICOES: [&str; 2] = ["embaixo", "topo"];
pub const ESPESSURAS: [&str; 3] = ["fina", "media", "grossa"];
pub const CORES_LUZ: [&str; 4] = ["azul", "verde", "roxo", "ambar"];

pub fn padroes() -> Value {
    let comandos: Map<String, Value> = COMANDOS.iter().map(|c| (c.to_string(), json!(true))).collect();
    json!({
        "volume": 60, "microfone": "", "idioma": "pt", "atalho_ditar": "ctrl+win", "atalho_colar": "alt+win+v",
        "luz": {"ligada": true, "cor": "azul", "espessura": "media", "posicao": "embaixo"},
        "tema": "sistema", "notificacoes": true, "abaixar_outros": false, "volume_outros": 30,
        "comandos": comandos,
        "atalho_traduzir": "shift+win", "modo": "literal",
        "modos_apps": {"outlook": "formal", "olk": "formal", "whatsapp": "whatsapp", "code": "literal"}, // olk = Outlook novo
    })
}

// ---------- atalho ----------
fn tecla_valida(t: &str) -> bool {
    let b = t.as_bytes();
    (b.len() == 1 && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())) || t == "space" || matches!(
        t.strip_prefix('f').filter(|n| !n.starts_with('0')).and_then(|n| n.parse::<u8>().ok().filter(|_| n.bytes().all(|c| c.is_ascii_digit()))),
        Some(1..=12)
    )
}

/// "Super+Ctrl" -> ["ctrl", "win"]: modificadores em ordem fixa e no máximo uma tecla. `None` se inválido.
pub fn validar_atalho(texto: &str) -> Option<Vec<String>> {
    let partes: Vec<String> = texto.split('+').map(|p| match p.trim().to_lowercase().as_str() {
        "super" | "windows" | "meta" => "win".to_string(),
        o => o.to_string(),
    }).collect();
    let mut unicos = partes.clone();
    unicos.sort();
    unicos.dedup();
    if partes.len() < 2 || unicos.len() != partes.len() {
        return None;
    }
    let mods: Vec<&str> = MODIFICADORES.iter().copied().filter(|m| partes.iter().any(|p| p == m)).collect();
    let teclas: Vec<&String> = partes.iter().filter(|p| !MODIFICADORES.contains(&p.as_str())).collect();
    // só Shift intercepta a digitação comum (Shift+A): precisa de Ctrl, Alt ou Win
    if teclas.len() > 1 || teclas.first().is_some_and(|t| !tecla_valida(t)) || mods.iter().all(|m| *m == "shift") {
        return None;
    }
    let saida: Vec<String> = mods.iter().map(|m| m.to_string()).chain(teclas.iter().map(|t| t.to_string())).collect();
    // do sistema: Alt+Shift troca o idioma do teclado (mesmo com Win junto) e o Windows usa quase todo Win+tecla
    let do_sistema = (mods.contains(&"alt") && mods.contains(&"shift")) || (mods == ["win"] && !teclas.is_empty());
    if do_sistema || ATALHOS_PROIBIDOS.contains(&saida.join("+").as_str()) {
        return None;
    }
    Some(saida)
}

/// Um atalho contido no outro (ctrl+win e ctrl+win+v) faz um disparar no lugar do outro.
pub fn atalhos_conflitam(a: &str, b: &str) -> bool {
    let (ca, cb): (Vec<&str>, Vec<&str>) = (a.split('+').collect(), b.split('+').collect());
    ca.iter().all(|x| cb.contains(x)) || cb.iter().all(|x| ca.contains(x))
}

// ---------- normalização ----------
fn objeto(v: Option<&Value>) -> Map<String, Value> {
    v.and_then(Value::as_object).cloned().unwrap_or_default()
}

fn str_em(v: Option<&Value>, validos: &[&str]) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| validos.contains(&s))
}

/// Luz com os quatro campos válidos; o que faltar ou for inválido volta ao padrão (chaves extras ficam).
fn normalizar_luz(bruto: Option<&Value>) -> Map<String, Value> {
    let mut luz = objeto(bruto);
    if !luz.get("ligada").is_some_and(Value::is_boolean) {
        luz.insert("ligada".into(), json!(true));
    }
    for (chave, validos, padrao) in [("cor", &CORES_LUZ[..], "azul"), ("espessura", &ESPESSURAS[..], "media"), ("posicao", &POSICOES[..], "embaixo")] {
        if !str_em(luz.get(chave), validos) {
            luz.insert(chave.into(), json!(padrao));
        }
    }
    luz
}

/// Liga/desliga dos comandos falados: o que faltar ou não for booleano fica ligado (chaves extras ficam).
fn normalizar_comandos(bruto: Option<&Value>) -> Map<String, Value> {
    let mut c = objeto(bruto);
    for chave in COMANDOS {
        if !c.get(chave).is_some_and(Value::is_boolean) {
            c.insert(chave.into(), json!(true));
        }
    }
    c
}

/// Nome em minúsculas, sem espaço nas pontas e só com modo válido. Não-objeto: vazio (igual ao Python).
fn normalizar_modos_apps(bruto: Option<&Value>) -> Map<String, Value> {
    let mut saida = Map::new();
    for (k, v) in bruto.and_then(Value::as_object).into_iter().flatten() {
        let k = k.trim().to_lowercase();
        if !k.is_empty() && str_em(Some(v), &MODOS) {
            saida.insert(k, v.clone());
        }
    }
    saida
}

fn repr(v: &Value) -> String {
    match v.as_str() {
        Some(s) => format!("'{s}'"),
        None => v.to_string(),
    }
}

/// Mescla com os padrões e troca valor inválido pelo padrão (a chave desconhecida segue junto).
/// Devolve também os avisos de atalho inválido, que quem lê/grava manda ao log.
pub fn normalizar_com_avisos(bruto: &Value) -> (Value, Vec<String>) {
    let mut avisos = vec![];
    let mut dados = objeto(Some(bruto));
    let padroes = padroes();
    for (chave, padrao) in padroes.as_object().unwrap() {
        dados.entry(chave.clone()).or_insert_with(|| padrao.clone());
    }
    for chave in ["volume", "volume_outros"] {
        // bool não é número; Python faz int() (corta a parte decimal) e limita a 0-100
        let n = dados[chave].as_f64().filter(|v| v.is_finite()).map(|v| v.trunc().clamp(0.0, 100.0) as i64);
        dados[chave] = json!(n.unwrap_or_else(|| padroes[chave].as_i64().unwrap()));
    }
    if !dados["microfone"].is_string() {
        dados["microfone"] = json!("");
    }
    if !str_em(Some(&dados["idioma"]), &IDIOMAS) {
        dados["idioma"] = padroes["idioma"].clone();
    }
    for (i, chave) in ATALHOS.iter().enumerate() { // cada um é conferido contra os anteriores, já válidos
        let anteriores: Vec<String> = ATALHOS[..i].iter().filter_map(|c| dados[*c].as_str().filter(|s| !s.is_empty()).map(String::from)).collect();
        let mut escolhido = String::new(); // nem o padrão serve: fica desligado
        for candidato in [dados[*chave].clone(), padroes[*chave].clone()] {
            let t = candidato.as_str().and_then(validar_atalho).map(|t| t.join("+"));
            if let Some(t) = t.filter(|t| !anteriores.iter().any(|o| atalhos_conflitam(t, o))) {
                escolhido = t;
                break;
            }
            avisos.push(format!("ajustes: {chave} inválido ou em conflito com outro atalho {}", repr(&candidato)));
        }
        dados[*chave] = json!(escolhido);
    }
    if !str_em(Some(&dados["modo"]), &MODOS) {
        dados["modo"] = padroes["modo"].clone();
    }
    dados["modos_apps"] = Value::Object(normalizar_modos_apps(Some(&dados["modos_apps"])));
    dados["luz"] = Value::Object(normalizar_luz(Some(&dados["luz"])));
    dados["comandos"] = Value::Object(normalizar_comandos(Some(&dados["comandos"])));
    if !str_em(Some(&dados["tema"]), &TEMAS) {
        dados["tema"] = padroes["tema"].clone();
    }
    for chave in ["notificacoes", "abaixar_outros"] {
        if !dados[chave].is_boolean() {
            dados[chave] = padroes[chave].clone();
        }
    }
    (Value::Object(dados), avisos)
}

pub fn normalizar(bruto: &Value) -> Value {
    normalizar_com_avisos(bruto).0
}

// ---------- leitura e gravação ----------
fn mtime(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir.join(pasta::ARQ_AJUSTES)).and_then(|m| m.modified()).ok()
}

/// Ajustes atuais, normalizados. Ausente ou corrompido: padrões (o motivo vai ao log).
pub fn ler(dir: &Path) -> Value {
    let arq = dir.join(pasta::ARQ_AJUSTES);
    let mut bruto = json!({});
    if arq.exists() {
        let lido = std::fs::read_to_string(&arq).map_err(|e| e.to_string()).and_then(|t| {
            let v: Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
            if v.is_object() { Ok(v) } else { Err("não é um objeto".into()) }
        });
        match lido {
            Ok(v) => bruto = v,
            Err(e) => pasta::log(dir, &format!("ajustes: {} ilegível ({e}), usando os padrões", arq.display())),
        }
    }
    let (dados, avisos) = normalizar_com_avisos(&bruto);
    avisos.iter().for_each(|a| pasta::log(dir, a));
    dados
}

/// Como `ler`, mas só relê o arquivo quando o mtime muda (o ditado lê a cada uso).
#[derive(Default)]
pub struct Cache {
    lido: Option<(Option<SystemTime>, Value)>,
}

impl Cache {
    pub fn ler(&mut self, dir: &Path) -> &Value {
        let m = mtime(dir);
        if self.lido.as_ref().is_none_or(|(antigo, _)| *antigo != m) {
            self.lido = Some((m, ler(dir)));
        }
        &self.lido.as_ref().unwrap().1
    }
}

/// Grava só as chaves recebidas, mantendo as demais do arquivo. Escrita atômica (temporário + rename).
pub fn salvar(dir: &Path, novos: &Map<String, Value>) -> io::Result<Value> {
    let mut atual = std::fs::read_to_string(dir.join(pasta::ARQ_AJUSTES)).ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for (k, v) in novos {
        atual.insert(k.clone(), v.clone());
    }
    let (dados, avisos) = normalizar_com_avisos(&Value::Object(atual));
    avisos.iter().for_each(|a| pasta::log(dir, a));
    // o Python grava com ensure_ascii=False e indent=2, como o serde_json (acentos e emoji ficam como são)
    pasta::gravar_atomico(&dir.join(pasta::ARQ_AJUSTES), &serde_json::to_string_pretty(&dados)?)?;
    Ok(dados)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::fs;

    fn dir_com(conteudo: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(pasta::ARQ_AJUSTES), conteudo).unwrap();
        d
    }
    fn log_de(d: &Path) -> String {
        fs::read_to_string(d.join(pasta::ARQ_LOG)).unwrap_or_default()
    }
    fn ler_json(v: Value) -> Value {
        ler(dir_com(&v.to_string()).path())
    }

    #[test]
    fn ausente_usa_padroes() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(ler(d.path()), padroes());
        assert_eq!(log_de(d.path()), "");
    }

    #[test]
    fn corrompido_usa_padroes_e_loga() {
        let d = dir_com("{nao é json");
        assert_eq!(ler(d.path()), padroes());
        assert!(log_de(d.path()).contains("ilegível"));
        let d = dir_com("[1]");
        assert_eq!(ler(d.path()), padroes());
        assert!(log_de(d.path()).contains("não é um objeto"));
    }

    #[test]
    fn chave_desconhecida_preservada() {
        let d = dir_com(&json!({"volume": 10, "modos": {"app": "formal"}}).to_string());
        salvar(d.path(), json!({"volume": 30}).as_object().unwrap()).unwrap();
        let dados: Value = serde_json::from_str(&fs::read_to_string(d.path().join(pasta::ARQ_AJUSTES)).unwrap()).unwrap();
        assert_eq!(dados["modos"], json!({"app": "formal"}));
        assert_eq!(dados["volume"], 30);
        assert!(!d.path().join("ajustes.json.tmp").exists());
    }

    #[test]
    fn salvar_sem_arquivo_cria_e_ler_devolve_igual() {
        let d = tempfile::tempdir().unwrap();
        let salvo = salvar(d.path(), json!({"microfone": "Mic ç 😀", "luz": {"cor": "verde"}}).as_object().unwrap()).unwrap();
        assert_eq!(ler(d.path()), salvo);
        assert_eq!(salvo["microfone"], "Mic ç 😀");
        assert_eq!(salvo["luz"], json!({"ligada": true, "cor": "verde", "espessura": "media", "posicao": "embaixo"})); // completa o que faltou
        assert_eq!(log_de(d.path()), ""); // o que o Rust grava, o Python relê sem aviso
    }

    #[test]
    fn atalho_invalido_vira_padrao() {
        assert_eq!(ler_json(json!({"atalho_ditar": "ctrl"}))["atalho_ditar"], "ctrl+win");
    }

    #[test]
    fn atalho_colar_padrao_invalido_e_igual() {
        assert_eq!(ler_json(json!({}))["atalho_colar"], "alt+win+v");
        let d = tempfile::tempdir().unwrap();
        for ruim in [json!("v"), json!("alt+win+v+x"), json!(5), json!(["alt", "win"]), json!("ctrl+win"), json!("ctrl+win+h")] {
            fs::write(d.path().join(pasta::ARQ_AJUSTES), json!({"atalho_colar": ruim}).to_string()).unwrap();
            assert_eq!(ler(d.path())["atalho_colar"], "alt+win+v", "{ruim}");
        }
        assert!(log_de(d.path()).contains("atalho_colar inválido ou em conflito"));
        assert_eq!(ler_json(json!({"atalho_colar": "Super+Alt+H"}))["atalho_colar"], "alt+win+h");
    }

    #[test]
    fn luz_tema_notificacoes_padroes() {
        let a = ler_json(json!({}));
        assert_eq!(a["luz"], json!({"ligada": true, "cor": "azul", "espessura": "media", "posicao": "embaixo"}));
        assert_eq!((&a["tema"], &a["notificacoes"]), (&json!("sistema"), &json!(true)));
    }

    #[test]
    fn luz_tema_notificacoes_validos_e_invalidos() {
        let a = ler_json(json!({"luz": {"ligada": false, "cor": "roxo", "espessura": "grossa", "posicao": "topo"}, "tema": "escuro", "notificacoes": false}));
        assert_eq!(a["luz"], json!({"ligada": false, "cor": "roxo", "espessura": "grossa", "posicao": "topo"}));
        assert_eq!((&a["tema"], &a["notificacoes"]), (&json!("escuro"), &json!(false)));
        // inválidos voltam ao padrão, campo a campo (o que estava certo fica)
        let a = ler_json(json!({"luz": {"ligada": "sim", "cor": "rosa", "espessura": "grossa", "posicao": 3, "extra": 1}, "tema": "azul", "notificacoes": "não"}));
        assert_eq!(a["luz"], json!({"ligada": true, "cor": "azul", "espessura": "grossa", "posicao": "embaixo", "extra": 1}));
        assert_eq!((&a["tema"], &a["notificacoes"]), (&json!("sistema"), &json!(true)));
        assert_eq!(ler_json(json!({"luz": "verde"}))["luz"], padroes()["luz"]); // tipo errado inteiro
    }

    #[test]
    fn validar_atalho_mesma_regra_do_python() {
        let v = |t: &str| validar_atalho(t).map(|x| x.join("+"));
        assert_eq!(v("Super+Ctrl").as_deref(), Some("ctrl+win"));
        assert_eq!(v("ctrl+alt+d").as_deref(), Some("ctrl+alt+d"));
        assert_eq!(v("ctrl+f12").as_deref(), Some("ctrl+f12"));
        for ruim in ["ctrl", "ctrl+ctrl", "d+e", "ctrl+ç", "", "d+f1", "shift+a", "shift+space", "ctrl+v", "V+Ctrl", "ctrl+c", "alt+f4",
                     "ctrl+f01", "ctrl+f13", "ctrl+f+1", "alt+shift+win", "alt+shift", "win+l", "win+space", "alt+space"] {
            assert_eq!(v(ruim), None, "{ruim}");
        }
        let p = padroes();
        for a in ATALHOS { // os padrões continuam válidos
            assert!(v(p[a].as_str().unwrap()).is_some(), "{a}");
        }
    }

    #[test]
    fn atalho_contido_no_outro_conflita() {
        assert!(atalhos_conflitam("ctrl+win", "ctrl+win+v"));
        assert!(atalhos_conflitam("ctrl+win+v", "ctrl+win"));
        assert!(!atalhos_conflitam("ctrl+win", "alt+win+v"));
        assert_eq!(normalizar(&json!({"atalho_colar": "ctrl+win+v"}))["atalho_colar"], "alt+win+v");
    }

    #[test]
    fn traduzir_padrao_e_conflitos() {
        assert_eq!(normalizar(&json!({}))["atalho_traduzir"], "shift+win");
        assert_eq!(normalizar(&json!({"atalho_traduzir": "ctrl+win+t"}))["atalho_traduzir"], "shift+win");
        // o de ditar ocupou o padrão do traduzir: traduzir fica desligado em vez de disparar no lugar do outro
        assert_eq!(normalizar(&json!({"atalho_ditar": "shift+win"}))["atalho_traduzir"], "");
    }

    #[test]
    fn abaixar_outros_padrao_validos_e_invalidos() {
        let a = ler_json(json!({}));
        assert_eq!((&a["abaixar_outros"], &a["volume_outros"]), (&json!(false), &json!(30)));
        let a = ler_json(json!({"abaixar_outros": true, "volume_outros": 150}));
        assert_eq!((&a["abaixar_outros"], &a["volume_outros"]), (&json!(true), &json!(100)));
        let a = ler_json(json!({"abaixar_outros": "sim", "volume_outros": "alto"}));
        assert_eq!((&a["abaixar_outros"], &a["volume_outros"]), (&json!(false), &json!(30)));
    }

    #[test]
    fn volume_corta_decimal_e_ignora_bool() {
        for (bruto, esperado) in [(json!(45.9), 45), (json!(-5.5), 0), (json!(true), 60), (json!(null), 60), (json!(1e30), 100)] {
            assert_eq!(normalizar(&json!({"volume": bruto}))["volume"], esperado);
        }
    }

    #[test]
    fn comandos_padrao_e_invalidos() {
        let ligados: Value = COMANDOS.iter().map(|c| (c.to_string(), json!(true))).collect::<Map<_, _>>().into();
        assert_eq!(normalizar(&json!({}))["comandos"], ligados);
        let a = &normalizar(&json!({"comandos": {"nova_linha": false, "virgula": "não", "extra": 1}}))["comandos"];
        assert_eq!((&a["nova_linha"], &a["virgula"], &a["ponto_final"], &a["extra"]), (&json!(false), &json!(true), &json!(true), &json!(1)));
        assert_eq!(normalizar(&json!({"comandos": "x"}))["comandos"], ligados);
    }

    #[test]
    fn padroes_nao_sao_alterados_por_quem_le() {
        let mut a = normalizar(&json!({}));
        a["luz"]["cor"] = json!("verde");
        assert_eq!(padroes()["luz"]["cor"], "azul");
    }

    #[test]
    fn cache_so_relê_quando_o_mtime_muda() {
        let d = dir_com(&json!({"volume": 10}).to_string());
        let mut c = Cache::default();
        assert_eq!(c.ler(d.path())["volume"], 10);
        let arq = d.path().join(pasta::ARQ_AJUSTES);
        let antes = fs::metadata(&arq).unwrap().modified().unwrap();
        fs::write(&arq, json!({"volume": 20}).to_string()).unwrap();
        fs::File::options().write(true).open(&arq).unwrap().set_modified(antes).unwrap(); // mesmo mtime: usa o cache
        assert_eq!(c.ler(d.path())["volume"], 10);
        fs::File::options().write(true).open(&arq).unwrap().set_modified(antes + std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(c.ler(d.path())["volume"], 20);
        fs::remove_file(&arq).unwrap();
        assert_eq!(c.ler(d.path()), &padroes());
    }
}
