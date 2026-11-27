//! Cliente HTTP da Groq: transcrição (Whisper), escrita com IA (`escrita::Cliente`) e conferência dos modelos.
//!
//! A chave é relida do `chave.txt` a cada chamada (trocar nas configurações vale na hora) e nunca vai a log nem a erro.
//! Os erros devolvem só o tipo da falha. A Groq responde 403 ao User-Agent padrão: mandamos `renantalk/<versão>`.
//! `RENANTALK_GROQ_URL` troca a base (padrão `https://api.groq.com/openai/v1`), para testar contra um servidor falso.

use crate::{escrita, pasta};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const BASE: &str = "https://api.groq.com/openai/v1";
pub const MODELO_VOZ: &str = "whisper-large-v3";
const LIMITE_VOZ: Duration = Duration::from_secs(30);
const LIMITE_MODELOS: Duration = Duration::from_secs(10);

/// Quem transcreve o áudio (a Groq, ou um dublê nos testes). `wav` é WAV 16 kHz mono; `idioma` "auto" não manda `language`.
pub trait Transcritor: Send + Sync {
    fn transcrever(&self, wav: &[u8], idioma: &str, vocabulario: &str) -> Result<String, String>;
}

pub struct Groq {
    dir: PathBuf,
    base: String,
    agente: String,
}

fn tipo_erro(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, _) => format!("HTTPError {c}"),
        ureq::Error::Transport(t) if t.kind() == ureq::ErrorKind::Io => "Timeout ou rede".into(),
        ureq::Error::Transport(t) => format!("{:?}", t.kind()),
    }
}

impl Groq {
    pub fn novo(dir: &Path, versao: &str) -> Self {
        let base = std::env::var("RENANTALK_GROQ_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| BASE.into());
        Groq { dir: dir.to_path_buf(), base: base.trim_end_matches('/').to_string(), agente: format!("renantalk/{versao}") }
    }

    fn pedido(&self, metodo: &str, caminho: &str, limite: Duration) -> ureq::Request {
        ureq::AgentBuilder::new().timeout(limite).build()
            .request(metodo, &format!("{}{caminho}", self.base))
            .set("Authorization", &format!("Bearer {}", pasta::ler_chave(&self.dir)))
            .set("User-Agent", &self.agente)
    }

    /// Avisa no log se a Groq desligou algum modelo que o app usa. Sem chave ou sem rede, fica quieto.
    pub fn conferir_modelos(&self, modelos: &[&str]) {
        if pasta::ler_chave(&self.dir).is_empty() {
            return;
        }
        let Ok(r) = self.pedido("GET", "/models", LIMITE_MODELOS).call() else { return };
        let Ok(json) = r.into_json::<Value>() else { return };
        let Some(lista) = json["data"].as_array() else { return };
        let ativo = |m: &str| lista.iter().any(|x| x["id"] == m && x["active"].as_bool().unwrap_or(true));
        for m in modelos.iter().filter(|m| !ativo(m)) {
            pasta::log(&self.dir, &format!("modelo {m} não está ativo na Groq: atualize o app ou troque o modelo"));
        }
    }
}

impl escrita::Cliente for Groq {
    fn enviar(&self, corpo: &Value) -> Result<Value, String> {
        let r = self.pedido("POST", "/chat/completions", escrita::LIMITE).send_json(corpo).map_err(tipo_erro)?;
        r.into_json().map_err(|_| "RespostaInvalida".to_string())
    }
}

/// Corpo multipart/form-data na mão (como o Python): campos de texto e o arquivo `audio.wav`.
fn multipart(limite: &str, campos: &[(&str, &str)], wav: &[u8]) -> Vec<u8> {
    let mut c = Vec::new();
    for (nome, valor) in campos {
        c.extend(format!("--{limite}\r\nContent-Disposition: form-data; name=\"{nome}\"\r\n\r\n{valor}\r\n").bytes());
    }
    c.extend(format!("--{limite}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").bytes());
    c.extend(wav);
    c.extend(format!("\r\n--{limite}--\r\n").bytes());
    c
}

impl Transcritor for Groq {
    fn transcrever(&self, wav: &[u8], idioma: &str, vocabulario: &str) -> Result<String, String> {
        // O vocabulário vai como contexto (não como ordem) para o Whisper grafar certo.
        let mut campos = vec![("model", MODELO_VOZ), ("temperature", "0"), ("response_format", "json")];
        if idioma != "auto" { // automático: sem `language`, o Whisper detecta
            campos.push(("language", idioma));
        }
        if !vocabulario.is_empty() {
            campos.push(("prompt", vocabulario));
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let limite = format!("renantalk{nanos:x}");
        let corpo = multipart(&limite, &campos, wav);
        let r = self.pedido("POST", "/audio/transcriptions", LIMITE_VOZ)
            .set("Content-Type", &format!("multipart/form-data; boundary={limite}"))
            .send_bytes(&corpo).map_err(tipo_erro)?;
        let json: Value = r.into_json().map_err(|_| "RespostaInvalida".to_string())?;
        Ok(json["text"].as_str().ok_or("RespostaInvalida")?.trim().to_string())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Servidor de uma resposta só: devolve o texto e entrega ao teste o pedido recebido.
    fn servidor(resposta: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/openai/v1", l.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = vec![0u8; 65536];
            let mut tot = 0;
            loop { // lê até ter o corpo inteiro (Content-Length)
                let n = s.read(&mut buf[tot..]).unwrap();
                tot += n;
                let t = String::from_utf8_lossy(&buf[..tot]).to_string();
                if let Some(i) = t.find("\r\n\r\n") {
                    let cl = t.to_lowercase().split("content-length: ").nth(1).and_then(|x| x.split("\r\n").next()?.trim().parse::<usize>().ok()).unwrap_or(0);
                    if tot >= i + 4 + cl { break; }
                }
                if n == 0 { break; }
            }
            tx.send(String::from_utf8_lossy(&buf[..tot]).to_string()).unwrap();
            let corpo = resposta;
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len()).unwrap();
        });
        (url, rx)
    }

    #[test]
    fn transcreve_com_agente_proprio_e_campos() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("chave.txt"), "gsk_falsa123").unwrap();
        let (url, rx) = servidor(r#"{"text": "  olá mundo "}"#);
        let g = Groq { dir: d.path().into(), base: url, agente: "renantalk/9.9.9".into() };
        let t = g.transcrever(&crate::audio::wav(&[1, 2, 3]), "pt", "Betinhos").unwrap();
        assert_eq!(t, "olá mundo");
        let pedido = rx.recv().unwrap();
        assert!(pedido.starts_with("POST /openai/v1/audio/transcriptions"));
        assert!(pedido.to_lowercase().contains("user-agent: renantalk/9.9.9"));
        assert!(pedido.contains("Bearer gsk_falsa123"));
        assert!(pedido.contains("name=\"language\"\r\n\r\npt") && pedido.contains("name=\"prompt\"\r\n\r\nBetinhos"));
        assert!(pedido.contains("whisper-large-v3"));
    }

    #[test]
    fn idioma_auto_nao_manda_language() {
        let c = multipart("x", &[("model", "m")], b"w");
        assert!(String::from_utf8_lossy(&c).contains("name=\"model\""));
        let d = tempfile::tempdir().unwrap();
        let (url, rx) = servidor(r#"{"text": "oi"}"#);
        let g = Groq { dir: d.path().into(), base: url, agente: "a".into() };
        g.transcrever(&[], "auto", "").unwrap();
        let p = rx.recv().unwrap();
        assert!(!p.contains("name=\"language\"") && !p.contains("name=\"prompt\""));
    }

    #[test]
    fn erro_http_nao_vaza_a_chave() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("chave.txt"), "gsk_segredo").unwrap();
        let g = Groq { dir: d.path().into(), base: "http://127.0.0.1:1".into(), agente: "a".into() };
        let e = g.transcrever(&[], "pt", "").unwrap_err();
        assert!(!e.contains("gsk_segredo"), "{e}");
    }
}
