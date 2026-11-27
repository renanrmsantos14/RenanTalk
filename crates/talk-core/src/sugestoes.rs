//! Sugestões de termos para o vocabulário, a partir da frequência nos ditados (tabela `sugestao` de
//! historico.db). Porta de `app/sugestoes.py`.
//!
//! Candidato: palavra com maiúscula fora do início de frase (palavras assim em sequência viram um termo só),
//! ou sigla (2+ maiúsculas), com 5 ocorrências ou mais nos ditados da janela. Ficam de fora o que já está no
//! vocabulário (cada palavra, inteira ou como parte de um termo composto, sem diferenciar maiúsculas) e as
//! palavras comuns abaixo. Aceita ou ignorada nunca volta a `nova`; só a contagem acompanha.

use fancy_regex::Regex;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const MINIMO: usize = 5;
// pt-BR comum que aparece com maiúscula no meio da frase (depois de ":" ou em título) e não merece sugestão
const COMUNS: &str = "eu você voce não nao sim olá ola oi bom boa dia tarde noite obrigado obrigada por favor então entao mas e ou que para \
com de da do em na no um uma os as tudo bem ok tá ta né ne também ainda hoje amanhã ontem segunda terça quarta quinta \
sexta sábado sabado domingo pode vai vamos aqui ali quando como porque se já agora depois antes ele ela nós eles isso \
isto esse essa este esta o a meu minha seu sua qual quem onde mais muito pouco todo toda cada outro outra";

/// Termo (chave minúscula) -> formas escritas e quantas vezes cada uma apareceu, na ordem em que surgiram.
pub type Formas = HashMap<String, Vec<(String, usize)>>;

/// Fim de frase ou quebra de linha (a palavra seguinte abre frase e não conta) | palavra.
fn token() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"([.!?…]+|\n)|([^\W\d_][\w'’-]*)").unwrap())
}

fn fechar(formas: &mut Formas, seq: &mut Vec<String>) {
    if seq.is_empty() {
        return;
    }
    let termo = seq.join(" ");
    let lista = formas.entry(termo.to_lowercase()).or_default();
    match lista.iter_mut().find(|(f, _)| *f == termo) {
        Some((_, n)) => *n += 1,
        None => lista.push((termo, 1)),
    }
    seq.clear();
}

/// Palavras com maiúscula seguidas, separadas só por espaço, formam um termo só ("Santos Dumont").
pub fn candidatos<S: AsRef<str>>(textos: impl IntoIterator<Item = S>) -> Formas {
    let comuns: HashSet<&str> = COMUNS.split_whitespace().collect();
    let mut formas = Formas::new();
    for texto in textos {
        let texto = texto.as_ref();
        let (mut inicio, mut seq, mut fim) = (true, Vec::<String>::new(), 0);
        for m in token().captures_iter(texto).flatten() {
            let todo = m.get(0).unwrap();
            let palavra = m.get(2).map(|p| p.as_str().trim_end_matches(['\'', '’', '-']));
            match palavra {
                Some(p) if !inicio && p.chars().count() >= 2 && p.chars().next().is_some_and(char::is_uppercase)
                    && !comuns.contains(p.to_lowercase().as_str()) =>
                {
                    let vao = &texto[fim..todo.start()];
                    if vao.is_empty() || !vao.chars().all(char::is_whitespace) {
                        fechar(&mut formas, &mut seq);
                    }
                    seq.push(p.to_string());
                }
                _ => fechar(&mut formas, &mut seq),
            }
            inicio = m.get(1).is_some();
            fim = todo.end();
        }
        fechar(&mut formas, &mut seq);
    }
    formas
}

/// Conta os candidatos nos ditados desde `corte` (ISO) e grava/atualiza a tabela `sugestao`.
pub fn atualizar(con: &Connection, vocab: &[String], corte: &str) -> rusqlite::Result<()> {
    let textos: Vec<String> = con
        .prepare("SELECT texto FROM ditado WHERE criado_em >= ?1 AND modo != 'ingles'")?
        .query_map([corte], |l| l.get(0))?
        .collect::<Result<_, _>>()?;
    let achados = candidatos(&textos);
    let conhecidos: HashSet<String> = vocab.iter().flat_map(|t| std::iter::once(t.as_str()).chain(t.split_whitespace())).map(str::to_lowercase).collect();
    let existentes: HashMap<String, (String, i64)> = con
        .prepare("SELECT termo, contagem FROM sugestao")?
        .query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(t, n)| (t.to_lowercase(), (t, n)))
        .collect();
    let tx = con.unchecked_transaction()?; // tudo ou nada, como o commit único do Python
    for (chave, formas) in &achados {
        let total: usize = formas.iter().map(|(_, n)| n).sum();
        if let Some((termo, antes)) = existentes.get(chave) {
            if total as i64 != *antes {
                tx.execute("UPDATE sugestao SET contagem=?1 WHERE termo=?2", rusqlite::params![total as i64, termo])?;
            }
        } else if total >= MINIMO && !chave.split_whitespace().all(|p| conhecidos.contains(p)) {
            // a forma mais escrita; empate fica com a que apareceu primeiro
            let melhor = formas.iter().fold(&formas[0], |m, f| if f.1 > m.1 { f } else { m });
            tx.execute("INSERT INTO sugestao(termo, motivo, contagem, estado) VALUES(?1, 'frequencia', ?2, 'nova')",
                       rusqlite::params![melhor.0, total as i64])?;
        }
    }
    // sugestão ainda pendente cujos ditados saíram da janela: some (a contagem "nos últimos 30 dias" ficaria falsa)
    for (chave, (termo, _)) in &existentes {
        if !achados.contains_key(chave) {
            tx.execute("DELETE FROM sugestao WHERE termo=?1 AND estado='nova' AND motivo='frequencia'", [termo])?;
        }
    }
    tx.commit()
}

#[cfg(test)]
mod testes {
    use crate::historico::{self, NovoDitado};
    use crate::pasta;
    use rusqlite::params;
    use std::collections::BTreeMap;
    use std::path::Path;
    use tempfile::TempDir;

    /// Pasta de dados com vocabulário dado (o `gravar` lê o vocabulário dela, como o Python).
    fn pasta_com(vocab: &[&str]) -> TempDir {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join(pasta::ARQ_VOCAB), vocab.join("\n") + "\n").unwrap();
        d
    }
    fn g(dir: &Path, texto: &str, n: usize) {
        for _ in 0..n {
            historico::gravar(dir, &NovoDitado { texto, ..Default::default() }).unwrap();
        }
    }
    fn sug(dir: &Path) -> BTreeMap<String, (String, i64, String)> {
        let con = historico::conectar(dir).unwrap();
        let mut st = con.prepare("SELECT termo, motivo, contagem, estado FROM sugestao").unwrap();
        st.query_map([], |l| Ok((l.get(0)?, (l.get(1)?, l.get(2)?, l.get(3)?)))).unwrap().map(Result::unwrap).collect()
    }
    fn sql(dir: &Path, comando: &str, p: impl rusqlite::Params) {
        historico::conectar(dir).unwrap().execute(comando, p).unwrap();
    }
    fn so(termo: &str, n: i64, estado: &str) -> BTreeMap<String, (String, i64, String)> {
        BTreeMap::from([(termo.to_string(), ("frequencia".to_string(), n, estado.to_string()))])
    }

    #[test]
    fn cinco_vira_sugestao_quatro_nao() {
        let d = pasta_com(&[]);
        g(d.path(), "falei com Marcondes sobre o prazo", 4);
        assert!(sug(d.path()).is_empty());
        g(d.path(), "falei com Marcondes sobre o prazo", 1);
        assert_eq!(sug(d.path()), so("Marcondes", 5, "nova"));
    }

    #[test]
    fn inicio_de_frase_nao_conta_mas_sigla_sim() {
        let d = pasta_com(&[]);
        g(d.path(), "Marcondes ligou. Marcondes pediu. Hoje o CTF subiu", 5);
        assert_eq!(sug(d.path()), so("CTF", 5, "nova"));
    }

    #[test]
    fn nome_composto_vira_um_termo_so() {
        let d = pasta_com(&["Betinhos Executive Service"]);
        g(d.path(), "buscar no Santos Dumont, Galeão Rio e a Betinhos Executive", 5);
        let termos: Vec<String> = sug(d.path()).into_keys().collect();
        assert_eq!(termos, ["Galeão Rio", "Santos Dumont"]);
    }

    #[test]
    fn vocabulario_e_palavras_comuns_ficam_de_fora() {
        let d = pasta_com(&["renantalk", "Betinhos Executive Service"]);
        g(d.path(), "vi o RenanTalk e o Executive e Você ou Não e Sim", 5);
        assert!(sug(d.path()).is_empty());
    }

    #[test]
    fn contagem_atualiza_e_ignorada_nao_volta() {
        let d = pasta_com(&[]);
        g(d.path(), "o Marcondes ligou", 5);
        sql(d.path(), "UPDATE sugestao SET estado='ignorada'", []);
        g(d.path(), "o marcondes e o Marcondes ligaram", 1);
        assert_eq!(sug(d.path()), so("Marcondes", 6, "ignorada"));
        sql(d.path(), "UPDATE sugestao SET estado='aceita'", []);
        g(d.path(), "de novo o Marcondes", 1);
        assert_eq!(sug(d.path())["Marcondes"].2, "aceita");
    }

    #[test]
    fn so_ultimos_30_dias() {
        let d = pasta_com(&[]);
        g(d.path(), "o Marcondes ligou", 5);
        sql(d.path(), "UPDATE ditado SET criado_em=?1", params!["2000-01-01T00:00:00"]);
        sql(d.path(), "DELETE FROM sugestao", []);
        g(d.path(), "o Marcondes ligou", 2);
        assert!(sug(d.path()).is_empty());
    }

    #[test]
    fn pendente_some_quando_os_ditados_saem_da_janela() {
        let d = pasta_com(&[]);
        g(d.path(), "o Marcondes ligou", 5);
        sql(d.path(), "UPDATE ditado SET criado_em=?1", params!["2000-01-01T00:00:00"]);
        g(d.path(), "oi", 1);
        assert!(sug(d.path()).is_empty());
    }

    #[test]
    fn falha_nas_sugestoes_nao_impede_gravar() {
        let d = pasta_com(&[]);
        let con = historico::conectar(d.path()).unwrap();
        con.execute_batch("DROP TABLE sugestao").unwrap();
        drop(con);
        assert!(historico::gravar(d.path(), &NovoDitado { texto: "oi", ..Default::default() }).is_some());
        assert_eq!(historico::listar(d.path(), "", 500).unwrap().len(), 1);
        assert!(std::fs::read_to_string(d.path().join(pasta::ARQ_LOG)).unwrap().contains("sugestões não atualizaram"));
    }
}
