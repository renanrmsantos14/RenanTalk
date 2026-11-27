//! Substituições no texto ditado: comandos falados prontos e regras próprias (`substituicoes.txt`).
//! Porta de `app/substituicoes.py`.
//!
//! Arquivo: uma regra `quando eu disser => escreva` por linha; `#` comenta; `\n` no lado direito vira quebra de
//! linha. Sem diferenciar maiúsculas e só em palavra inteira. Regra própria vale antes dos comandos prontos.
//!
//! Comandos prontos (ligados e desligados em ajustes.json, chave `comandos`): o Whisper já põe pontuação ao redor
//! do que ouve ("Nova linha.", "vírgula,"); ela é tirada. A pontuação colada à palavra anterior sai, o espaço some
//! antes de `. , ? )` e depois de `(`, e a palavra seguinte a "ponto final" e "interrogação" ganha maiúscula.
//! Não faz: maiúscula depois de "nova linha" nem concordância; "nova linha" dentro de uma frase comum também
//! vira quebra (desligue o comando).

use crate::pasta;
use fancy_regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const SINAIS: &str = ".!?…";
/// (id, frases, saída, pontuação anterior que se mantém, pontuação posterior que se mantém).
/// `{antes}`/`{depois}` é a pontuação que o Whisper pôs ao redor e que vale manter; NUL marca onde a próxima
/// palavra ganha maiúscula.
const COMANDOS: [(&str, &str, &str, &str, &str); 6] = [
    ("nova_linha", r"nova\s+linha", "{antes}\n", SINAIS, ""),
    ("ponto_final", r"ponto\s+final", ".\0 ", "", ""),
    ("virgula", r"v[ií]rgula", ", ", "", ""),
    ("interrogacao", r"(?:ponto\s+de\s+)?interroga[cç][aã]o", "?\0 ", "", ""),
    ("abre_parenteses", r"(?:abre|abrir)\s+par[eê]nteses?", "{antes} (", SINAIS, ""),
    ("fecha_parenteses", r"(?:fecha|fechar)\s+par[eê]nteses?", "){depois} ", "", SINAIS),
];

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Regra {
    pub quando: String,
    pub escreva: String,
}

/// Regras do texto do arquivo; linha sem `=>` ou com lado vazio é ignorada. O `\n` do lado direito segue
/// como está no arquivo (é assim que a tela mostra e grava).
pub fn parse_regras(texto: &str) -> Vec<Regra> {
    texto.lines().filter_map(|l| {
        let (q, e) = l.split_once("=>")?;
        let (q, e) = (q.trim(), e.trim());
        (!q.is_empty() && !q.starts_with('#') && !e.is_empty()).then(|| Regra { quando: q.into(), escreva: e.into() })
    }).collect()
}

/// Pares (quando, escreva) prontos para `aplicar`: `\n` do lado direito já é quebra de linha. Ausente: vazio.
pub fn ler_regras(dir: &Path) -> Vec<(String, String)> {
    let texto = std::fs::read_to_string(dir.join(pasta::ARQ_SUBST)).unwrap_or_default();
    parse_regras(&texto).into_iter().map(|r| (r.quando, r.escreva.replace("\\n", "\n"))).collect()
}

fn frase(texto: &str) -> String {
    texto.split_whitespace().map(fancy_regex::escape).collect::<Vec<_>>().join(r"\s+")
}

/// Troca cada ocorrência por `f`; devolve o texto e quantas trocas fez. Erro do motor (limite de
/// retrocesso): devolve o texto como veio.
fn substituir(re: &Regex, texto: &str, mut f: impl FnMut(&Captures) -> String) -> (String, usize) {
    let (mut saida, mut ultimo, mut n) = (String::new(), 0, 0);
    for c in re.captures_iter(texto) {
        let Ok(c) = c else { return (texto.to_string(), 0) };
        let m = c.get(0).unwrap();
        saida.push_str(&texto[ultimo..m.start()]);
        saida.push_str(&f(&c));
        (ultimo, n) = (m.end(), n + 1);
    }
    saida.push_str(&texto[ultimo..]);
    (saida, n)
}

fn simples(padrao: &str) -> Regex {
    Regex::new(padrao).unwrap()
}

/// Texto com as regras próprias e os comandos ligados aplicados. Sem nada a trocar, devolve o texto como veio.
pub fn aplicar(texto: &str, comandos: &Value, regras: &[(String, String)]) -> String {
    let mut texto = texto.to_string();
    if !regras.is_empty() {
        let norm = |s: &str| s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
        let mut mapa: Vec<(String, &str)> = vec![]; // ordem de chegada; a última regra repetida vence
        for (q, e) in regras {
            match mapa.iter_mut().find(|(k, _)| *k == norm(q)) {
                Some(item) => item.1 = e,
                None => mapa.push((norm(q), e)),
            }
        }
        let mut chaves: Vec<&String> = mapa.iter().map(|(k, _)| k).collect();
        chaves.sort_by_key(|k| std::cmp::Reverse(k.chars().count())); // a mais longa primeiro (sort é estável)
        let achar = chaves.iter().map(|k| frase(k)).collect::<Vec<_>>().join("|");
        let re = Regex::new(&format!(r"(?i)(?<!\w)(?:{achar})(?!\w)")).unwrap();
        texto = substituir(&re, &texto, |c| {
            let chave = norm(c.get(0).unwrap().as_str());
            mapa.iter().find(|(k, _)| *k == chave).map_or(String::new(), |(_, e)| e.to_string())
        }).0;
    }
    let ativos: Vec<_> = COMANDOS.iter().filter(|(id, ..)| comandos.get(id).and_then(Value::as_bool).unwrap_or(true)).collect();
    if ativos.is_empty() {
        return texto;
    }
    let partes: Vec<String> = ativos.iter().map(|(id, frase, ..)| format!("(?P<{id}>{frase})")).collect();
    let re = Regex::new(&format!(
        r"(?i)(?P<antes>[,;:.!?…]?)[ \t]*(?<!\w)(?:{})(?!\w)(?P<depois>[.,;:!?…]*)[ \t]*", partes.join("|")
    )).unwrap();
    let (trocado, n) = substituir(&re, &texto, |c| {
        let (_, _, saida, mantem_antes, mantem_depois) = ativos.iter().find(|(id, ..)| c.name(id).is_some_and(|m| !m.as_str().is_empty())).unwrap();
        let antes = c.name("antes").map_or("", |m| m.as_str());
        let antes = if !antes.is_empty() && mantem_antes.contains(antes) { antes } else { "" };
        let depois: String = c.name("depois").map_or("", |m| m.as_str()).chars().filter(|ch| mantem_depois.contains(*ch)).collect();
        saida.replace("{antes}", antes).replace("{depois}", &depois)
    });
    if n == 0 {
        return texto;
    }
    let t = simples(r"[ \t]+\n").replace_all(&trocado, "\n").into_owned();
    let t = simples(r"\n[ \t]+").replace_all(&t, "\n").into_owned();
    let t = substituir(&simples(r"\x00([ \n]*)(\w)"), &t, |c| format!("{}{}", &c[1], c[2].to_uppercase())).0;
    simples(r" {2,}").replace_all(&t.replace('\0', ""), " ").trim_matches(' ').to_string()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::ajustes::{self, COMANDOS as IDS};
    use serde_json::json;

    fn ligados() -> Value {
        Value::Object(IDS.iter().map(|c| (c.to_string(), json!(true))).collect())
    }
    fn ap(texto: &str) -> String {
        aplicar(texto, &ligados(), &[])
    }
    fn com(texto: &str, regras: &[(&str, &str)]) -> String {
        let r: Vec<_> = regras.iter().map(|(q, e)| (q.to_string(), e.to_string())).collect();
        aplicar(texto, &ligados(), &r)
    }

    #[test]
    fn ids_dos_comandos_sao_os_dos_ajustes() {
        assert_eq!(COMANDOS.map(|c| c.0), IDS);
    }

    #[test]
    fn nova_linha_com_pontuacao_do_whisper() {
        assert_eq!(ap("Olá, nova linha. Tudo bem?"), "Olá\nTudo bem?");
        assert_eq!(ap("Eu fiz isso. Nova linha. Depois fui."), "Eu fiz isso.\nDepois fui.");
    }

    #[test]
    fn ponto_e_virgula_sem_espaco_antes_e_maiuscula_depois() {
        assert_eq!(ap("Terminei ponto final, próxima frase"), "Terminei. Próxima frase");
        assert_eq!(ap("Você vai Vírgula, né ponto de interrogação sim"), "Você vai, né? Sim");
        assert_eq!(ap("Terminei. Ponto final."), "Terminei.");
    }

    #[test]
    fn parenteses() {
        assert_eq!(ap("Veja, abre parênteses, o anexo, fecha parênteses. Depois"), "Veja (o anexo). Depois");
        assert_eq!(ap("veja abrir parêntese x fechar parêntese"), "veja (x)");
    }

    #[test]
    fn palavra_inteira_e_sem_comando_nao_mexe() {
        assert_eq!(ap("Os pontos finais e a nova linhagem"), "Os pontos finais e a nova linhagem");
        assert_eq!(ap("texto  com  espaços. Nada."), "texto  com  espaços. Nada.");
    }

    #[test]
    fn comando_desligado_fica_como_dito() {
        let mut off = ligados();
        off["nova_linha"] = json!(false);
        assert_eq!(aplicar("uma nova linha de produtos", &off, &[]), "uma nova linha de produtos");
        assert_eq!(aplicar("oi vírgula tudo", &off, &[]), "oi, tudo"); // os outros seguem ligados
        let tudo_off = Value::Object(IDS.iter().map(|c| (c.to_string(), json!(false))).collect());
        assert_eq!(aplicar("oi vírgula tudo", &tudo_off, &[]), "oi vírgula tudo");
    }

    #[test]
    fn comandos_seguidos() {
        assert_eq!(ap("a ponto final nova linha b"), "a.\nB");
    }

    #[test]
    fn regra_propria_sem_diferenciar_maiusculas() {
        assert_eq!(com("Meu E-mail é esse", &[("meu e-mail", "renan@betinhos.com.br")]), "renan@betinhos.com.br é esse");
    }

    #[test]
    fn regra_propria_pontuacao_do_texto_fica() {
        assert_eq!(com("Segue o meu assinatura.", &[("meu assinatura", "Att,\nRenan")]), "Segue o Att,\nRenan.");
    }

    #[test]
    fn regra_so_em_palavra_inteira_e_a_mais_longa_vale() {
        assert_eq!(com("bolsa bol", &[("bol", "X")]), "bolsa X");
        assert_eq!(com("bom dia a todos", &[("bom", "A"), ("bom dia", "B")]), "B a todos");
    }

    #[test]
    fn ler_arquivo() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(ler_regras(d.path()), vec![]); // ausente
        std::fs::write(d.path().join(pasta::ARQ_SUBST), "# comentário\nbom dia => Bom dia, tudo bem?\n\nlinha => a\\nb\nsem seta\n => vazio\nx =>\n").unwrap();
        assert_eq!(ler_regras(d.path()), [("bom dia".into(), "Bom dia, tudo bem?".into()), ("linha".into(), "a\nb".into())]);
        assert_eq!(parse_regras("linha => a\\nb")[0].escreva, "a\\nb"); // cru, como a tela mostra
    }

    #[test]
    fn regra_vale_antes_do_comando() {
        assert_eq!(com("ponto final", &[("ponto final", "FIM")]), "FIM");
    }

    #[test]
    fn ajuste_dos_comandos_padrao_e_invalido() {
        assert_eq!(ajustes::normalizar(&json!({}))["comandos"], ligados());
    }
}
