//! Funções equivalentes aos comandos de `config/src/main.rs` (janela de configurações), na mesma forma de
//! resposta (`{ok, ...}`), para a janela passar a chamá-las no lugar do código próprio. Cada uma recebe a
//! pasta de dados.

use crate::substituicoes::{parse_regras, Regra};
use crate::{ajustes, historico, pasta};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Abre o banco só se ele existe: sem ditado ainda, a lista é vazia e nenhum arquivo é criado.
fn abrir(dir: &Path) -> Result<Option<Connection>, String> {
    if !historico::arquivo(dir).exists() {
        return Ok(None);
    }
    historico::conectar(dir).map(Some).map_err(|e| e.to_string())
}

fn erro_historico(e: impl ToString) -> Value {
    json!({ "ok": false, "erro": "Não consegui acessar o histórico agora. Tente de novo.", "detalhe": e.to_string() })
}

pub fn historico_listar(dir: &Path, busca: &str) -> Value {
    match abrir(dir).and_then(|c| match c {
        None => Ok(vec![]),
        Some(c) => historico::listar_em(&c, busca, 500).map_err(|e| e.to_string()),
    }) {
        Ok(itens) => json!({ "ok": true, "itens": itens }),
        Err(e) => erro_historico(e),
    }
}

/// Corrige o texto: marca editado e deixa `original` como está. Texto vazio é recusado (a janela não deixa
/// apagar um ditado por edição; o Python não valida, é só a janela).
pub fn historico_editar(dir: &Path, id: i64, texto: &str) -> Value {
    if texto.trim().is_empty() {
        return json!({ "ok": false, "erro": "O texto não pode ficar vazio." });
    }
    match abrir(dir).and_then(|c| match c {
        None => Ok(false),
        Some(_) => historico::editar(dir, id, texto).map_err(|e| e.to_string()),
    }) {
        Ok(ok) => json!({ "ok": ok }),
        Err(e) => erro_historico(e),
    }
}

pub fn historico_excluir(dir: &Path, id: i64) -> Value {
    match abrir(dir).and_then(|c| match c {
        None => Ok(false),
        Some(_) => historico::excluir(dir, id).map_err(|e| e.to_string()),
    }) {
        Ok(ok) => json!({ "ok": ok }),
        Err(e) => erro_historico(e),
    }
}

pub fn historico_apagar_tudo(dir: &Path) -> Value {
    match abrir(dir).and_then(|c| match c {
        None => Ok(()),
        Some(_) => historico::apagar_tudo(dir).map(|_| ()).map_err(|e| e.to_string()),
    }) {
        Ok(()) => json!({ "ok": true }),
        Err(e) => erro_historico(e),
    }
}

pub fn salvar_vocab(dir: &Path, termos: &[String]) -> Value {
    let termos: Vec<String> = termos.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    json!({ "ok": pasta::salvar_vocabulario(dir, &termos).is_ok() })
}

/// Painel Início: resumo dos ditados e sugestões de termos.
pub fn inicio(dir: &Path) -> Value {
    let hoje = chrono::Local::now().date_naive();
    match abrir(dir) {
        Ok(None) => json!({ "ok": true, "vazio": true, "hoje": 0, "semana": 0, "poupado_s": 0, "dias": [], "ultimo": null, "sugestoes": [] }),
        Err(e) => erro_historico(e),
        Ok(Some(c)) => historico::resumo(&c, hoje, &pasta::ler_vocabulario(dir)).unwrap_or_else(erro_historico),
    }
}

/// Aceitar acrescenta o termo ao vocabulario.txt (sem duplicar, mantendo comentários) e marca `aceita`;
/// ignorar só marca `ignorada`. Devolve o vocabulário atual.
pub fn sugestao_resolver(dir: &Path, termo: &str, acao: &str) -> Value {
    let termo = termo.trim();
    if termo.is_empty() || !["aceitar", "ignorar"].contains(&acao) {
        return json!({ "ok": false, "erro": "Pedido inválido." });
    }
    let aceitar = acao == "aceitar";
    if aceitar && !pasta::ler_vocabulario(dir).iter().any(|t| t.to_lowercase() == termo.to_lowercase()) {
        let arq = dir.join(pasta::ARQ_VOCAB);
        let mut texto = std::fs::read_to_string(&arq).unwrap_or_default();
        if !texto.is_empty() && !texto.ends_with('\n') {
            texto.push('\n');
        }
        texto.push_str(&format!("{termo}\n"));
        if pasta::gravar_atomico(&arq, &texto).is_err() {
            return json!({ "ok": false, "erro": "Não consegui gravar o vocabulário." });
        }
    }
    let marcado = abrir(dir).and_then(|c| match c {
        None => Ok(()),
        Some(c) => c
            .execute(
                "INSERT INTO sugestao(termo, estado) VALUES(?1, ?2) ON CONFLICT(termo) DO UPDATE SET estado = excluded.estado",
                rusqlite::params![termo, if aceitar { "aceita" } else { "ignorada" }],
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
    });
    match marcado {
        Ok(()) => json!({ "ok": true, "vocab": pasta::ler_vocabulario(dir) }),
        Err(e) => erro_historico(e),
    }
}

pub fn substituicoes_ler(dir: &Path) -> Value {
    let regras = parse_regras(&std::fs::read_to_string(dir.join(pasta::ARQ_SUBST)).unwrap_or_default());
    json!({ "ok": true, "regras": regras })
}

/// Reescreve o arquivo inteiro com as regras recebidas (comentários do arquivo não são mantidos).
pub fn substituicoes_salvar(dir: &Path, regras: &[Regra]) -> Value {
    if regras.len() > 200 {
        return json!({ "ok": false, "erro": "Cabem até 200 regras." });
    }
    let mut vistas: Vec<String> = Vec::new();
    let mut texto = String::new();
    for r in regras {
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
    match pasta::gravar_atomico(&dir.join(pasta::ARQ_SUBST), &texto) {
        Ok(()) => json!({ "ok": true }),
        Err(_) => json!({ "ok": false, "erro": "Não consegui salvar as regras." }),
    }
}

// ---------- ajustes, estado e chave (a janela de configurações) ----------

/// Ajustes para a tela (já normalizados) e a lista de microfones, que só a casca conhece (cpal).
pub fn ajustes_da_tela(dir: &Path, dispositivos: Value) -> Value {
    json!({ "ajustes": ajustes::ler(dir), "dispositivos": dispositivos })
}

/// Chave, rótulo para o erro de conflito.
const ATALHOS_ROTULO: [(&str, &str, &str); 3] = [
    ("atalho_ditar", "ctrl+win", "ditar"),
    ("atalho_colar", "alt+win+v", "colar o último ditado"),
    ("atalho_traduzir", "shift+win", "traduzir"),
];

fn recusa(erro: &str) -> Value {
    json!({ "ok": false, "erro": erro })
}

/// Grava só o que a tela edita; as outras chaves do arquivo ficam como estão (o próximo ditado já lê o valor novo,
/// pelo mtime). Valor inválido recusa tudo; atalho recusado não impede gravar o resto (volume, microfone...):
/// grava e devolve o erro, e a tela volta o cartão ao atalho gravado.
pub fn salvar_ajustes(dir: &Path, valores: &Map<String, Value>) -> Value {
    let atual = ajustes::ler(dir);
    let mut novos = Map::new();
    for chave in ["volume", "volume_outros"] {
        if let Some(v) = valores.get(chave).and_then(Value::as_f64) {
            novos.insert(chave.into(), json!(v.clamp(0.0, 100.0).round() as i64));
        }
    }
    if let Some(m) = valores.get("microfone").and_then(Value::as_str) {
        novos.insert("microfone".into(), json!(m));
    }
    if let Some(i) = valores.get("idioma").and_then(Value::as_str) {
        if !ajustes::IDIOMAS.contains(&i) {
            return recusa("Idioma inválido.");
        }
        novos.insert("idioma".into(), json!(i));
    }
    if let Some(l) = valores.get("luz") {
        let Some(novo) = l.as_object() else { return recusa("Ajuste da luz inválido.") };
        let mut luz = atual["luz"].as_object().cloned().unwrap_or_default();
        for (chave, v) in novo {
            let valido = match chave.as_str() {
                "ligada" => v.is_boolean(),
                "cor" => v.as_str().is_some_and(|x| ajustes::CORES_LUZ.contains(&x)),
                "espessura" => v.as_str().is_some_and(|x| ajustes::ESPESSURAS.contains(&x)),
                "posicao" => v.as_str().is_some_and(|x| ajustes::POSICOES.contains(&x)),
                _ => false,
            };
            if !valido {
                return recusa("Ajuste da luz inválido.");
            }
            luz.insert(chave.clone(), v.clone());
        }
        novos.insert("luz".into(), Value::Object(luz));
    }
    if let Some(c) = valores.get("comandos") {
        let Some(c) = c.as_object() else { return recusa("Ajuste dos comandos inválido.") };
        let mut comandos = atual["comandos"].as_object().cloned().unwrap_or_default();
        for (chave, v) in c {
            if !ajustes::COMANDOS.contains(&chave.as_str()) || !v.is_boolean() {
                return recusa("Ajuste dos comandos inválido.");
            }
            comandos.insert(chave.clone(), v.clone());
        }
        novos.insert("comandos".into(), Value::Object(comandos));
    }
    if let Some(t) = valores.get("tema").and_then(Value::as_str) {
        if !ajustes::TEMAS.contains(&t) {
            return recusa("Tema inválido.");
        }
        novos.insert("tema".into(), json!(t));
    }
    for chave in ["notificacoes", "abaixar_outros"] {
        if let Some(b) = valores.get(chave).and_then(Value::as_bool) {
            novos.insert(chave.into(), json!(b));
        }
    }
    if let Some(m) = valores.get("modo").and_then(Value::as_str) {
        if !ajustes::MODOS.contains(&m) {
            return recusa("Modo de escrita inválido.");
        }
        novos.insert("modo".into(), json!(m));
    }
    if let Some(m) = valores.get("modos_apps") {
        if !m.is_object() {
            return recusa("Modos por aplicativo inválidos.");
        }
        novos.insert("modos_apps".into(), m.clone()); // a normalização (minúsculas, só modos válidos) é do `salvar`
    }
    let mut erro: Option<String> = None;
    for (chave, _, _) in ATALHOS_ROTULO {
        let Some(t) = valores.get(chave).and_then(Value::as_str) else { continue };
        let Some(ok) = ajustes::validar_atalho(t).map(|p| p.join("+")) else {
            erro.get_or_insert_with(|| "Atalho inválido: use Ctrl, Alt ou Win com mais uma tecla, e não atalhos do sistema (Alt + Shift, Win + letra, Ctrl + V e parecidos).".into());
            continue;
        };
        let conflito = ATALHOS_ROTULO.iter().filter(|(outra, _, _)| *outra != chave).find_map(|(outra, padrao, para_que)| {
            let da_outra = valores.get(*outra).and_then(Value::as_str).and_then(ajustes::validar_atalho).map(|p| p.join("+"))
                .or_else(|| atual[*outra].as_str().filter(|s| !s.is_empty()).map(String::from))
                .unwrap_or_else(|| padrao.to_string());
            ajustes::atalhos_conflitam(&ok, &da_outra).then(|| format!("Esse atalho se confunde com o de {para_que} ({da_outra}). Escolha outro."))
        });
        if let Some(e) = conflito {
            erro.get_or_insert(e);
            continue;
        }
        novos.insert(chave.into(), json!(ok));
    }
    match (ajustes::salvar(dir, &novos), erro) {
        (Err(_), _) => recusa("Não consegui salvar os ajustes."),
        (Ok(_), Some(e)) => recusa(&e),
        (Ok(_), None) => json!({ "ok": true }),
    }
}

/// `gsk_abcd••••••••••wxyz`: nunca a chave inteira. Curta demais: vazio.
pub fn mascarar_chave(chave: &str) -> String {
    if chave.chars().count() > 8 {
        let ini: String = chave.chars().take(4).collect();
        let fim: String = chave.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{ini}{}{fim}", "•".repeat(10))
    } else {
        String::new()
    }
}

/// Dados da tela: vocabulário, chave mascarada, ditados de hoje e a última verificação de atualização (lidos do log).
pub fn estado(dir: &Path, memoria_mb: Option<u64>) -> Value {
    let hoje = chrono::Local::now().format("%d/%m/%Y").to_string();
    let (mut ditados, mut verificacao) = (0, String::new());
    if let Ok(f) = std::fs::File::open(dir.join(pasta::ARQ_LOG)) {
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
    json!({
        "vocab": pasta::ler_vocabulario(dir), "limite": pasta::LIMITE_VOCAB,
        "chave": mascarar_chave(&pasta::ler_chave(dir)), "memoria": memoria_mb,
        "ditados_hoje": ditados, "verificacao": verificacao,
    })
}

/// Confere a chave na Groq (lista de modelos) e só então grava. `base` é a URL da API (a casca passa a de
/// `RENANTALK_GROQ_URL` ou a real). A chave nunca vai para log nem para a mensagem de erro.
pub fn salvar_chave(dir: &Path, base: &str, chave: &str) -> Value {
    let k = chave.trim();
    if !k.starts_with("gsk_") {
        return recusa("A chave da Groq começa com gsk_.");
    }
    let resp = ureq::get(&format!("{}/models", base.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {k}"))
        .set("User-Agent", "renantalk/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .call();
    match resp {
        Ok(_) => match pasta::salvar_chave(dir, k) {
            Ok(()) => {
                pasta::log(dir, "chave da Groq atualizada");
                json!({ "ok": true, "chave": mascarar_chave(k) })
            }
            Err(_) => recusa("Não consegui gravar a chave."),
        },
        Err(ureq::Error::Status(401 | 403, _)) => recusa("A Groq recusou essa chave."),
        Err(ureq::Error::Status(c, _)) => recusa(&format!("A Groq respondeu erro {c}.")),
        Err(_) => recusa("Sem conexão com a Groq. Tente de novo."),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::historico::NovoDitado;
    use tempfile::TempDir;

    fn r(q: &str, e: &str) -> Regra {
        Regra { quando: q.into(), escreva: e.into() }
    }

    #[test]
    fn sem_banco_lista_vazia_e_nao_cria_arquivo() {
        let d = TempDir::new().unwrap();
        assert_eq!(historico_listar(d.path(), ""), json!({ "ok": true, "itens": [] }));
        assert_eq!(inicio(d.path())["vazio"], true);
        assert_eq!(historico_apagar_tudo(d.path()), json!({ "ok": true }));
        assert!(!historico::arquivo(d.path()).exists());
    }

    #[test]
    fn listar_editar_excluir_apagar() {
        let d = TempDir::new().unwrap();
        let i = historico::gravar(d.path(), &NovoDitado { texto: "renan talk", ..Default::default() }).unwrap();
        let l = historico_listar(d.path(), "talk");
        assert_eq!((l["itens"][0]["texto"].clone(), l["itens"][0]["editado"].clone()), (json!("renan talk"), json!(false)));
        assert_eq!(historico_editar(d.path(), i, "  ")["erro"], "O texto não pode ficar vazio.");
        assert_eq!(historico_editar(d.path(), i, "RenanTalk"), json!({ "ok": true }));
        assert_eq!(historico_editar(d.path(), 999, "x"), json!({ "ok": false }));
        let l = historico_listar(d.path(), "");
        assert_eq!((l["itens"][0]["texto"].clone(), l["itens"][0]["original"].clone(), l["itens"][0]["editado"].clone()),
                   (json!("RenanTalk"), json!("renan talk"), json!(true)));
        assert_eq!(historico_excluir(d.path(), i), json!({ "ok": true }));
        assert_eq!(historico_excluir(d.path(), i), json!({ "ok": false }));
        historico::gravar(d.path(), &NovoDitado { texto: "x", ..Default::default() });
        assert_eq!(historico_apagar_tudo(d.path()), json!({ "ok": true }));
        assert_eq!(historico_listar(d.path(), "")["itens"], json!([]));
    }

    #[test]
    fn erro_de_banco_vira_mensagem_para_a_tela() {
        let d = TempDir::new().unwrap();
        std::fs::create_dir(historico::arquivo(d.path())).unwrap();
        let r = historico_listar(d.path(), "");
        assert_eq!((r["ok"].clone(), r["erro"].clone()), (json!(false), json!("Não consegui acessar o histórico agora. Tente de novo.")));
    }

    #[test]
    fn vocab_filtra_vazios() {
        let d = TempDir::new().unwrap();
        assert_eq!(salvar_vocab(d.path(), &[" CTF ".into(), "".into(), "  ".into(), "Marcondes".into()]), json!({ "ok": true }));
        assert_eq!(std::fs::read_to_string(d.path().join(pasta::ARQ_VOCAB)).unwrap(), "CTF\nMarcondes\n");
    }

    #[test]
    fn sugestao_aceitar_nao_duplica_e_marca() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join(pasta::ARQ_VOCAB), "# meus termos\nCTF").unwrap(); // sem \n final
        historico::conectar(d.path()).unwrap().execute("INSERT INTO sugestao VALUES('Marcondes','frequencia',7,'nova')", []).unwrap();
        let a = sugestao_resolver(d.path(), "Marcondes", "aceitar");
        assert_eq!(a["vocab"], json!(["CTF", "Marcondes"]));
        sugestao_resolver(d.path(), "marcondes", "aceitar"); // já está: não duplica
        assert_eq!(std::fs::read_to_string(d.path().join(pasta::ARQ_VOCAB)).unwrap(), "# meus termos\nCTF\nMarcondes\n");
        let est: String = historico::conectar(d.path()).unwrap().query_row("SELECT estado FROM sugestao WHERE termo='Marcondes'", [], |l| l.get(0)).unwrap();
        assert_eq!(est, "aceita");
        sugestao_resolver(d.path(), "Ana", "ignorar"); // termo que ainda nem existe na tabela
        let est: String = historico::conectar(d.path()).unwrap().query_row("SELECT estado FROM sugestao WHERE termo='Ana'", [], |l| l.get(0)).unwrap();
        assert_eq!(est, "ignorada");
        assert_eq!(sugestao_resolver(d.path(), "", "aceitar")["ok"], false);
        assert_eq!(sugestao_resolver(d.path(), "x", "apagar")["ok"], false);
    }

    #[test]
    fn inicio_sugestoes_novas_fora_do_vocabulario() {
        let d = TempDir::new().unwrap();
        historico::gravar(d.path(), &NovoDitado { texto: "oi", ..Default::default() }).unwrap();
        historico::conectar(d.path()).unwrap().execute_batch(
            "INSERT INTO sugestao VALUES('Marcondes','frequencia',7,'nova'),('Ana','frequencia',9,'ignorada');").unwrap();
        let r = inicio(d.path());
        assert_eq!(r["vazio"], false);
        assert_eq!(r["sugestoes"], json!([{ "termo": "Marcondes", "motivo": "frequencia", "contagem": 7 }]));
    }

    #[test]
    fn regras_ida_e_volta() {
        let d = TempDir::new().unwrap();
        assert_eq!(substituicoes_ler(d.path()), json!({ "ok": true, "regras": [] }));
        assert_eq!(substituicoes_salvar(d.path(), &[r("bom dia", "Bom dia, tudo bem?"), r("linha", "a\nb")]), json!({ "ok": true }));
        assert_eq!(std::fs::read_to_string(d.path().join(pasta::ARQ_SUBST)).unwrap(), "bom dia => Bom dia, tudo bem?\nlinha => a\\nb\n");
        assert_eq!(substituicoes_ler(d.path())["regras"], json!([{ "quando": "bom dia", "escreva": "Bom dia, tudo bem?" }, { "quando": "linha", "escreva": "a\\nb" }]));
    }

    #[test]
    fn regras_invalidas_nao_gravam() {
        let d = TempDir::new().unwrap();
        for ruim in [r("", "x"), r("x", ""), r("a => b", "x"), r("# c", "x")] {
            assert_eq!(substituicoes_salvar(d.path(), &[ruim])["ok"], false);
        }
        assert_eq!(substituicoes_salvar(d.path(), &[r("Bom  Dia", "a"), r("bom dia", "b")])["ok"], false); // duplicada
        assert_eq!(substituicoes_salvar(d.path(), &vec![r("a", "b"); 201])["erro"], "Cabem até 200 regras.");
        assert!(!d.path().join(pasta::ARQ_SUBST).exists());
    }

    fn m(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn salvar_ajustes_grava_volume_e_o_proximo_ditado_le() {
        let d = TempDir::new().unwrap();
        let mut cache = ajustes::Cache::default();
        assert_eq!(cache.ler(d.path())["volume"], 60);
        assert_eq!(salvar_ajustes(d.path(), &m(json!({"volume": 20, "luz": {"cor": "verde"}, "modo": "formal"}))), json!({"ok": true}));
        let a = cache.ler(d.path()).clone(); // mesma instância do cache que o ditado usa: relê pelo mtime
        assert_eq!((a["volume"].clone(), a["luz"]["cor"].clone(), a["modo"].clone()), (json!(20), json!("verde"), json!("formal")));
        assert_eq!(a["luz"]["espessura"], "media"); // o resto da luz fica
    }

    #[test]
    fn salvar_ajustes_recusa_valor_invalido_sem_gravar() {
        let d = TempDir::new().unwrap();
        for v in [json!({"idioma": "fr"}), json!({"tema": "rosa"}), json!({"luz": {"cor": "preto"}}), json!({"modo": "ingles"}), json!({"comandos": {"x": true}})] {
            assert_eq!(salvar_ajustes(d.path(), &m(v))["ok"], false);
        }
        assert!(!d.path().join(pasta::ARQ_AJUSTES).exists());
    }

    #[test]
    fn atalho_recusado_nao_impede_gravar_o_resto() {
        let d = TempDir::new().unwrap();
        let r = salvar_ajustes(d.path(), &m(json!({"atalho_ditar": "ctrl+c", "volume": 10})));
        assert_eq!(r["ok"], false);
        assert!(r["erro"].as_str().unwrap().starts_with("Atalho inválido"));
        let a = ajustes::ler(d.path());
        assert_eq!((a["volume"].clone(), a["atalho_ditar"].clone()), (json!(10), json!("ctrl+win")));
        let r = salvar_ajustes(d.path(), &m(json!({"atalho_traduzir": "ctrl+win+v"}))); // contém o de ditar
        assert!(r["erro"].as_str().unwrap().contains("se confunde com o de ditar"));
        assert_eq!(salvar_ajustes(d.path(), &m(json!({"atalho_ditar": "Super+Ctrl+d"})))["ok"], true);
        assert_eq!(ajustes::ler(d.path())["atalho_ditar"], "ctrl+win+d");
    }

    #[test]
    fn chave_mascarada_e_estado_sem_a_chave() {
        let d = TempDir::new().unwrap();
        assert_eq!(mascarar_chave("curta"), "");
        pasta::salvar_chave(d.path(), "gsk_ABCDEFGHIJKLMNOP1234").unwrap();
        pasta::log(d.path(), "transcrito 12 caracteres");
        pasta::log(d.path(), "atualização: em_dia v0.0.1");
        let e = estado(d.path(), Some(12));
        assert_eq!(e["chave"], "gsk_••••••••••1234");
        assert_eq!((e["ditados_hoje"].clone(), e["memoria"].clone()), (json!(1), json!(12)));
        assert!(e["verificacao"].as_str().unwrap().ends_with("em dia"));
        assert!(!e.to_string().contains("ABCDEFGH"));
    }

    /// Servidor mínimo que responde sempre `status` a qualquer pedido (a Groq falsa).
    fn groq_falsa(status: u16) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut c = c;
                let mut b = [0u8; 2048];
                let _ = c.read(&mut b);
                let _ = write!(c, "HTTP/1.1 {status} X\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}");
            }
        });
        url
    }

    #[test]
    fn chave_falsa_e_recusada_sem_expor_nem_gravar() {
        let d = TempDir::new().unwrap();
        let r = salvar_chave(d.path(), &groq_falsa(401), "gsk_SEGREDOSEGREDOSEGREDO");
        assert_eq!(r, json!({"ok": false, "erro": "A Groq recusou essa chave."}));
        assert_eq!(salvar_chave(d.path(), "http://127.0.0.1:1", "nao-comeca")["erro"], "A chave da Groq começa com gsk_.");
        assert!(salvar_chave(d.path(), "http://127.0.0.1:1", "gsk_SEGREDOSEGREDOSEGREDO")["erro"].as_str().unwrap().starts_with("Sem conexão"));
        assert!(pasta::ler_chave(d.path()).is_empty());
        assert!(!std::fs::read_to_string(d.path().join(pasta::ARQ_LOG)).unwrap_or_default().contains("SEGREDO"));
    }

    #[test]
    fn chave_valida_e_gravada_e_devolvida_mascarada() {
        let d = TempDir::new().unwrap();
        let r = salvar_chave(d.path(), &groq_falsa(200), " gsk_ABCDEFGHIJKLMNOP1234 ");
        assert_eq!(r, json!({"ok": true, "chave": "gsk_••••••••••1234"}));
        assert_eq!(pasta::ler_chave(d.path()), "gsk_ABCDEFGHIJKLMNOP1234");
    }
}
