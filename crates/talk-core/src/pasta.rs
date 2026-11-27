//! Pasta de dados, `chave.txt`, `vocabulario.txt` e `ditado.log` (porta de `app/comum.py`).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const ARQ_AJUSTES: &str = "ajustes.json";
pub const ARQ_HISTORICO: &str = "historico.db";
pub const ARQ_VOCAB: &str = "vocabulario.txt";
pub const ARQ_SUBST: &str = "substituicoes.txt";
pub const ARQ_CHAVE: &str = "chave.txt";
pub const ARQ_LOG: &str = "ditado.log";
pub const LIMITE_VOCAB: usize = 850; // caracteres; o Whisper só aproveita ~224 tokens de contexto
pub const VOCAB_PADRAO: [&str; 13] = [
    "Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI",
    "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp",
];

/// Pasta de dados real: `%APPDATA%\RenanTalk` no Windows, `$XDG_CONFIG_HOME/RenanTalk` (ou `~/.config`) no Linux.
pub fn padrao() -> PathBuf {
    let var = |nome: &str| std::env::var(nome).ok().filter(|v| !v.is_empty());
    if cfg!(windows) {
        PathBuf::from(var("APPDATA").unwrap_or_default()).join("RenanTalk")
    } else {
        var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(var("HOME").unwrap_or_default()).join(".config"))
            .join("RenanTalk")
    }
}

/// Escrita atômica (temporário + rename): quem lê (o Python) nunca vê o arquivo pela metade.
pub fn gravar_atomico(arq: &Path, texto: &str) -> io::Result<()> {
    if let Some(pai) = arq.parent() {
        fs::create_dir_all(pai)?;
    }
    let mut tmp = arq.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, texto)?;
    // No Windows o rename falha se o outro processo estiver lendo o arquivo naquele instante: tenta de novo.
    let mut ultimo = Ok(());
    for i in 0..3 {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        ultimo = fs::rename(&tmp, arq);
        if ultimo.is_ok() {
            return Ok(());
        }
    }
    let _ = fs::remove_file(&tmp);
    ultimo
}

/// Uma linha no ditado.log (nunca o texto ditado nem a chave). Falha de disco é ignorada.
pub fn log(dir: &Path, msg: &str) {
    let _ = fs::create_dir_all(dir);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join(ARQ_LOG)) {
        let _ = writeln!(f, "{} {msg}", chrono::Local::now().format("%d/%m/%Y %H:%M:%S"));
    }
}

/// Chave da Groq (vazia se não houver). Nunca vai para log, histórico nem erro.
pub fn ler_chave(dir: &Path) -> String {
    fs::read_to_string(dir.join(ARQ_CHAVE)).unwrap_or_default().trim().to_string()
}

pub fn salvar_chave(dir: &Path, chave: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let arq = dir.join(ARQ_CHAVE);
    fs::write(&arq, chave)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&arq, fs::Permissions::from_mode(0o600))?; // só o seu usuário lê
    }
    Ok(())
}

/// Uma palavra ou expressão por linha; linhas vazias e começadas com # são ignoradas.
/// Sem o arquivo, cria com o vocabulário padrão (como o Python).
pub fn ler_vocabulario(dir: &Path) -> Vec<String> {
    let arq = dir.join(ARQ_VOCAB);
    if !arq.exists() {
        let _ = fs::create_dir_all(dir);
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

pub fn salvar_vocabulario(dir: &Path, termos: &[String]) -> io::Result<()> {
    gravar_atomico(&dir.join(ARQ_VOCAB), &(termos.join("\n") + "\n"))
}

/// Termos separados por vírgula, cortados no limite sem quebrar um termo ao meio.
pub fn prompt_vocabulario(dir: &Path) -> String {
    let mut texto = String::new();
    for termo in ler_vocabulario(dir) {
        if texto.chars().count() + termo.chars().count() + 2 > LIMITE_VOCAB {
            break;
        }
        if !texto.is_empty() {
            texto.push_str(", ");
        }
        texto.push_str(&termo);
    }
    if texto.is_empty() { texto } else { texto + "." }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn vocabulario_padrao_ignora_comentarios_e_vazias() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(ler_vocabulario(d.path()).len(), 13); // criou o padrão
        fs::write(d.path().join(ARQ_VOCAB), "# c\n\n  Marcondes  \nCTF\n").unwrap();
        assert_eq!(ler_vocabulario(d.path()), ["Marcondes", "CTF"]);
    }

    #[test]
    fn prompt_corta_sem_quebrar_termo() {
        let d = tempfile::tempdir().unwrap();
        assert!(prompt_vocabulario(d.path()).starts_with("Betinhos, Betinhos Executive Service, Dataverse"));
        let termos: Vec<String> = (0..200).map(|i| format!("termo{i:03}")).collect();
        salvar_vocabulario(d.path(), &termos).unwrap();
        let p = prompt_vocabulario(d.path());
        assert!(p.chars().count() <= LIMITE_VOCAB && p.ends_with('.'));
        assert!(p.trim_end_matches('.').split(", ").all(|t| t.len() == 8));
        fs::write(d.path().join(ARQ_VOCAB), "").unwrap();
        assert_eq!(prompt_vocabulario(d.path()), "");
    }

    #[test]
    fn chave_ida_e_volta_e_log_sem_chave() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(ler_chave(d.path()), "");
        salvar_chave(d.path(), "gsk_segredo\n").unwrap();
        assert_eq!(ler_chave(d.path()), "gsk_segredo");
        log(d.path(), "algo aconteceu");
        let l = fs::read_to_string(d.path().join(ARQ_LOG)).unwrap();
        assert!(l.contains("algo aconteceu") && !l.contains("gsk_segredo"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(d.path().join(ARQ_CHAVE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn gravar_atomico_nao_deixa_temporario() {
        let d = tempfile::tempdir().unwrap();
        let arq = d.path().join("sub").join("x.txt");
        gravar_atomico(&arq, "oi").unwrap();
        assert_eq!(fs::read_to_string(&arq).unwrap(), "oi");
        assert!(!d.path().join("sub").join("x.txt.tmp").exists());
    }
}
