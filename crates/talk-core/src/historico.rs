//! Histórico de ditados (`historico.db` na pasta de dados, SQLite em modo WAL). Porta de `app/historico.py`:
//! mesmo schema e mesmo formato de data, para o Python e o Rust lerem e gravarem o mesmo arquivo.
//! Itens com mais de 30 dias são apagados. Gravar nunca pode impedir o texto de ser colado: `gravar` captura
//! qualquer erro e só registra no log (sem conteúdo).

use crate::{pasta, sugestoes};
use chrono::{Days, Duration, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

pub const VERSAO: i64 = 2; // 2: garante a tabela `sugestao` em bancos criados antes dela (tudo é IF NOT EXISTS)
pub const RETENCAO_DIAS: i64 = 30;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2); // banco travado por outro processo: espera pouco e desiste
const FORMATO: &str = "%Y-%m-%dT%H:%M:%S";
/// Palavras por minuto de quem digita: base da estimativa de tempo poupado.
pub const PALAVRAS_POR_MIN: f64 = 40.0;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Ditado {
    pub id: i64,
    pub criado_em: String,
    pub texto: String,
    pub original: String,
    pub app: String,
    pub modo: String,
    pub idioma: String,
    pub duracao_s: f64,
    pub palavras: i64,
    pub editado: bool,
}

/// O que `gravar` recebe. `texto` é o colado (já com as substituições); `original` é a transcrição crua
/// do Whisper (vazio = igual ao texto).
#[derive(Default, Clone)]
pub struct NovoDitado<'a> {
    pub texto: &'a str,
    pub original: Option<&'a str>,
    pub app: &'a str,
    pub modo: &'a str, // vazio = literal
    pub idioma: &'a str,
    pub duracao_s: f64,
}

fn agora(menos_dias: i64) -> String {
    (Local::now() - Duration::days(menos_dias)).format(FORMATO).to_string()
}

pub fn arquivo(dir: &Path) -> std::path::PathBuf {
    dir.join(pasta::ARQ_HISTORICO)
}

/// Abre (criando, se preciso) o banco e garante o schema.
pub fn conectar(dir: &Path) -> rusqlite::Result<Connection> {
    conectar_com(dir, TIMEOUT)
}

fn conectar_com(dir: &Path, espera: std::time::Duration) -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(dir).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let con = Connection::open(arquivo(dir))?;
    con.busy_timeout(espera)?;
    let versao: i64 = con.query_row("PRAGMA user_version", [], |l| l.get(0))?;
    if versao < VERSAO {
        con.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?; // fica gravado no arquivo: basta na criação
        con.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS ditado(
                id INTEGER PRIMARY KEY,
                criado_em TEXT NOT NULL,
                texto TEXT NOT NULL,
                original TEXT NOT NULL,
                app TEXT NOT NULL DEFAULT '',
                modo TEXT NOT NULL DEFAULT 'literal',
                idioma TEXT NOT NULL DEFAULT '',
                duracao_s REAL NOT NULL DEFAULT 0,
                palavras INTEGER NOT NULL DEFAULT 0,
                editado INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS ix_ditado_criado_em ON ditado(criado_em);
            CREATE TABLE IF NOT EXISTS sugestao(
                termo TEXT PRIMARY KEY,
                motivo TEXT NOT NULL DEFAULT '',
                contagem INTEGER NOT NULL DEFAULT 0,
                estado TEXT NOT NULL DEFAULT 'nova'
            );
            PRAGMA user_version={VERSAO};"
        ))?;
    }
    Ok(con)
}

fn gravar_com(dir: &Path, d: &NovoDitado, espera: std::time::Duration) -> rusqlite::Result<i64> {
    let con = conectar_com(dir, espera)?;
    let duracao: f64 = format!("{:.2}", d.duracao_s).parse().unwrap_or(0.0); // arredonda como o round() do Python
    con.execute(
        "INSERT INTO ditado(criado_em, texto, original, app, modo, idioma, duracao_s, palavras, editado) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0)",
        params![agora(0), d.texto, d.original.unwrap_or(d.texto), d.app,
                if d.modo.is_empty() { "literal" } else { d.modo }, d.idioma, duracao, d.texto.split_whitespace().count() as i64],
    )?;
    let id = con.last_insert_rowid();
    if let Err(e) = sugestoes::atualizar(&con, &pasta::ler_vocabulario(dir), &agora(RETENCAO_DIAS)) {
        // sugestão é extra: o ditado já foi gravado
        pasta::log(dir, &format!("histórico: sugestões não atualizaram ({})", tipo_erro(&e)));
    }
    Ok(id)
}

/// Só o tipo do erro, nunca o conteúdo (o texto ditado não pode ir para o log).
fn tipo_erro(e: &rusqlite::Error) -> String {
    match e.sqlite_error_code() {
        Some(c) => format!("{c:?}"),
        None => "Erro".into(),
    }
}

/// Guarda um ditado e atualiza as sugestões de termos. Devolve o id, ou None se falhar (o erro vai ao log, sem o texto).
pub fn gravar(dir: &Path, d: &NovoDitado) -> Option<i64> {
    gravar_com(dir, d, TIMEOUT)
        .map_err(|e| pasta::log(dir, &format!("histórico: não gravou ({})", tipo_erro(&e))))
        .ok()
}

/// Ditados do mais novo ao mais antigo; `busca` vazia lista tudo (LIKE simples, sem curingas do usuário).
pub fn listar(dir: &Path, busca: &str, limite: i64) -> rusqlite::Result<Vec<Ditado>> {
    listar_em(&conectar(dir)?, busca, limite)
}

pub fn listar_em(con: &Connection, busca: &str, limite: i64) -> rusqlite::Result<Vec<Ditado>> {
    let like = format!("%{}%", busca.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let mut st = con.prepare(
        "SELECT id, criado_em, texto, original, app, modo, idioma, duracao_s, palavras, editado FROM ditado \
         WHERE texto LIKE ?1 ESCAPE '\\' ORDER BY criado_em DESC, id DESC LIMIT ?2",
    )?;
    let linhas = st.query_map(params![like, limite], |l| {
        Ok(Ditado {
            id: l.get(0)?, criado_em: l.get(1)?, texto: l.get(2)?, original: l.get(3)?, app: l.get(4)?, modo: l.get(5)?,
            idioma: l.get(6)?, duracao_s: l.get(7)?, palavras: l.get(8)?, editado: l.get::<_, i64>(9)? != 0,
        })
    })?;
    linhas.collect()
}

/// Corrige o texto: marca editado=1 e preserva `original`.
pub fn editar(dir: &Path, id: i64, texto: &str) -> rusqlite::Result<bool> {
    let n = conectar(dir)?.execute(
        "UPDATE ditado SET texto=?1, palavras=?2, editado=1 WHERE id=?3",
        params![texto, texto.split_whitespace().count() as i64, id],
    )?;
    Ok(n > 0)
}

pub fn excluir(dir: &Path, id: i64) -> rusqlite::Result<bool> {
    Ok(conectar(dir)?.execute("DELETE FROM ditado WHERE id=?1", [id])? > 0)
}

pub fn apagar_tudo(dir: &Path) -> rusqlite::Result<usize> {
    let con = conectar(dir)?;
    let n = con.execute("DELETE FROM ditado", [])?;
    con.execute("DELETE FROM sugestao", [])?; // os termos sugeridos vêm dos ditados: somem junto
    Ok(n)
}

/// Apaga ditados com mais de `dias` dias. Nunca propaga erro (roda em thread de fundo).
pub fn limpar_antigos(dir: &Path, dias: i64) -> usize {
    conectar(dir)
        .and_then(|c| c.execute("DELETE FROM ditado WHERE criado_em < ?1", [agora(dias)]))
        .unwrap_or_else(|e| {
            pasta::log(dir, &format!("histórico: limpeza falhou ({})", tipo_erro(&e)));
            0
        })
}

/// Resumo do painel: palavras hoje e nos últimos 7 dias, tempo poupado (palavras ÷ 40 por minuto, menos o tempo
/// falado, nunca negativo), gráfico por dia, último ditado e sugestões novas fora do vocabulário.
pub fn resumo(con: &Connection, hoje: NaiveDate, vocab: &[String]) -> rusqlite::Result<Value> {
    let dias: Vec<String> = (0..7u64).rev().map(|i| (hoje - Days::new(i)).format("%Y-%m-%d").to_string()).collect();
    let mut st = con.prepare("SELECT substr(criado_em, 1, 10), SUM(palavras), SUM(duracao_s) FROM ditado WHERE criado_em >= ?1 GROUP BY 1")?;
    let linhas = st.query_map([&dias[0]], |l| Ok((l.get::<_, String>(0)?, l.get::<_, i64>(1)?, l.get::<_, f64>(2)?)))?;
    let por_dia: HashMap<String, (i64, f64)> = linhas.filter_map(Result::ok).map(|(d, p, s)| (d, (p, s))).collect();
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

#[cfg(test)]
mod testes {
    use super::*;
    use tempfile::TempDir;

    fn g(dir: &Path, texto: &str) -> i64 {
        gravar(dir, &NovoDitado { texto, ..Default::default() }).unwrap()
    }
    fn textos(dir: &Path) -> Vec<String> {
        listar(dir, "", 500).unwrap().into_iter().map(|d| d.texto).collect()
    }

    #[test]
    fn banco_ausente_e_criado_com_schema() {
        let d = TempDir::new().unwrap();
        assert!(!arquivo(d.path()).exists());
        assert_eq!(listar(d.path(), "", 500).unwrap(), vec![]);
        let con = Connection::open(arquivo(d.path())).unwrap();
        assert_eq!(con.query_row("PRAGMA user_version", [], |l| l.get::<_, i64>(0)).unwrap(), VERSAO);
        assert_eq!(con.query_row("PRAGMA journal_mode", [], |l| l.get::<_, String>(0)).unwrap(), "wal");
        assert_eq!(con.query_row("SELECT count(*) FROM sugestao", [], |l| l.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn gravar_guarda_campos() {
        let d = TempDir::new().unwrap();
        let i = gravar(d.path(), &NovoDitado { texto: "Reunião às 15h 😀", app: "code", idioma: "pt", duracao_s: 2.345, ..Default::default() }).unwrap();
        let l = listar(d.path(), "", 500).unwrap();
        assert_eq!(l.len(), 1);
        let x = &l[0];
        assert_eq!((x.id, x.texto.as_str(), x.original.as_str(), x.app.as_str(), x.modo.as_str(), x.idioma.as_str(), x.palavras, x.editado),
                   (i, "Reunião às 15h 😀", "Reunião às 15h 😀", "code", "literal", "pt", 4, false));
        assert_eq!(x.duracao_s, 2.35);
        assert_eq!(x.criado_em.len(), 19); // %Y-%m-%dT%H:%M:%S
    }

    #[test]
    fn busca_like_ignora_curingas() {
        let d = TempDir::new().unwrap();
        for t in ["fechar a cotação do cliente", "100% certo", "outro texto"] {
            g(d.path(), t);
        }
        let n = |b: &str| listar(d.path(), b, 500).unwrap().len();
        assert_eq!((n("cotação"), n("100%"), n("%"), n("")), (1, 1, 1, 3)); // % literal, não curinga
        assert_eq!(n("_"), 0);
    }

    #[test]
    fn editar_marca_editado_e_preserva_original() {
        let d = TempDir::new().unwrap();
        let i = g(d.path(), "renan talk");
        assert!(editar(d.path(), i, "RenanTalk").unwrap());
        let l = &listar(d.path(), "", 500).unwrap()[0];
        assert_eq!((l.texto.as_str(), l.original.as_str(), l.editado), ("RenanTalk", "renan talk", true));
        assert!(!editar(d.path(), 999, "x").unwrap());
    }

    #[test]
    fn excluir_e_apagar_tudo() {
        let d = TempDir::new().unwrap();
        let a = g(d.path(), "um");
        g(d.path(), "dois");
        assert!(excluir(d.path(), a).unwrap());
        assert_eq!(textos(d.path()), ["dois"]);
        g(d.path(), "três");
        assert_eq!(apagar_tudo(d.path()).unwrap(), 2);
        assert!(textos(d.path()).is_empty());
    }

    #[test]
    fn retencao_apaga_so_mais_de_30_dias() {
        let d = TempDir::new().unwrap();
        g(d.path(), "recente");
        g(d.path(), "velho");
        let con = conectar(d.path()).unwrap();
        con.execute("UPDATE ditado SET criado_em=?1 WHERE texto='velho'", [agora(31)]).unwrap();
        con.execute("INSERT INTO ditado(criado_em,texto,original) VALUES(?1,'29 dias','29 dias')", [agora(29)]).unwrap();
        drop(con);
        assert_eq!(limpar_antigos(d.path(), RETENCAO_DIAS), 1);
        let mut t = textos(d.path());
        t.sort();
        assert_eq!(t, ["29 dias", "recente"]);
    }

    #[test]
    fn erro_de_gravacao_nao_propaga_nem_loga_texto() {
        let d = TempDir::new().unwrap();
        std::fs::create_dir(arquivo(d.path())).unwrap(); // "historico.db" é pasta: abrir falha
        assert_eq!(gravar(d.path(), &NovoDitado { texto: "segredo-xyz", ..Default::default() }), None);
        assert_eq!(limpar_antigos(d.path(), 30), 0);
        let log = std::fs::read_to_string(d.path().join(pasta::ARQ_LOG)).unwrap();
        assert!(log.contains("não gravou") && log.contains("limpeza falhou"));
        assert!(!log.contains("segredo-xyz"));
    }

    #[test]
    fn banco_travado_desiste_rapido() {
        let d = TempDir::new().unwrap();
        g(d.path(), "existe");
        let trava = Connection::open(arquivo(d.path())).unwrap();
        trava.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let t = std::time::Instant::now();
        let r = gravar_com(d.path(), &NovoDitado { texto: "bloqueado", ..Default::default() }, std::time::Duration::from_millis(200));
        assert!(r.is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        trava.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn texto_longo() {
        let d = TempDir::new().unwrap();
        let i = g(d.path(), &"palavra ".repeat(20000));
        assert_eq!(listar(d.path(), "", 500).unwrap()[0].palavras, 20000);
        assert!(i > 0);
    }

    // ---- resumo (porta dos testes de config/src/main.rs) ----
    fn banco(linhas: &[(&str, &str, i64, f64)]) -> Connection {
        let con = Connection::open_in_memory().unwrap();
        con.execute_batch(
            "CREATE TABLE ditado(id INTEGER PRIMARY KEY, criado_em TEXT NOT NULL, texto TEXT NOT NULL, original TEXT NOT NULL DEFAULT '', app TEXT NOT NULL DEFAULT '', palavras INTEGER NOT NULL DEFAULT 0, duracao_s REAL NOT NULL DEFAULT 0);",
        ).unwrap();
        for (quando, texto, palavras, dur) in linhas {
            con.execute("INSERT INTO ditado(criado_em, texto, palavras, duracao_s) VALUES(?1,?2,?3,?4)", params![quando, texto, palavras, dur]).unwrap();
        }
        con
    }

    #[test]
    fn resumo_conta_hoje_semana_e_poupado() {
        let hoje = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
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
        let hoje = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let r = resumo(&banco(&[("2026-11-26T09:00:00", "oi", 2, 60.0)]), hoje, &[]).unwrap();
        assert_eq!(r["poupado_s"].as_f64(), Some(0.0));
        assert_eq!(r["sugestoes"], json!([]));
        assert_eq!(resumo(&banco(&[]), hoje, &[]).unwrap()["vazio"], true);
    }

    #[test]
    fn resumo_sugestoes_so_novas_e_fora_do_vocabulario() {
        let hoje = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let con = banco(&[]);
        con.execute_batch(
            "CREATE TABLE sugestao(termo TEXT PRIMARY KEY, motivo TEXT NOT NULL DEFAULT '', contagem INTEGER NOT NULL DEFAULT 0, estado TEXT NOT NULL DEFAULT 'nova');
             INSERT INTO sugestao VALUES('Marcondes','frequencia',7,'nova'),('CTF','frequencia',6,'nova'),('Ana','frequencia',9,'ignorada'),('Dataverse','frequencia',5,'nova');",
        ).unwrap();
        let r = resumo(&con, hoje, &["dataverse".to_string()]).unwrap();
        let termos: Vec<&str> = r["sugestoes"].as_array().unwrap().iter().map(|s| s["termo"].as_str().unwrap()).collect();
        assert_eq!(termos, ["Marcondes", "CTF"]);
    }
}
