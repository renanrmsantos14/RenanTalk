//! Funções equivalentes aos comandos de `config/src/main.rs` (janela de configurações), na mesma forma de
//! resposta (`{ok, ...}`), para a janela passar a chamá-las no lugar do código próprio. Cada uma recebe a
//! pasta de dados.

use crate::substituicoes::{parse_regras, Regra};
use crate::{historico, pasta};
use rusqlite::Connection;
use serde_json::{json, Value};
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
}
