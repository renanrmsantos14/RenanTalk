//! Fluxo do ditado (porta de `app/ditado.pyw`): gravar, transcrever, aplicar substituições e IA, colar e gravar no histórico.
//! Tudo que depende do sistema (colar, app ativo, volume, bipe, microfone, luz) entra pela trait `Plataforma`;
//! a Groq entra por `Transcritor` e `escrita::Cliente`. O log nunca leva o texto ditado nem a chave.

use crate::groq::Transcritor;
use crate::historico::{self, NovoDitado};
use crate::{ajustes, audio, escrita, pasta, substituicoes};
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const MINIMO_S: f64 = 0.4; // gravações mais curtas são ignoradas (toque acidental)

/// Estado da luz (desenhada por `talk_core::luz`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoLuz {
    Gravando { ingles: bool },
    Processando,
    Pronto,
    Erro,
}

/// Peças do sistema. Cada sistema (Linux X11, Windows) implementa uma vez; os métodos podem ser chamados de
/// qualquer thread. O volume dos outros apps guarda o que mudou dentro da própria implementação.
pub trait Plataforma: Send + Sync {
    /// Cola `texto` no campo em foco e devolve a área de transferência ao que era antes.
    fn colar(&self, texto: &str) -> Result<(), String>;
    /// Aplicativo em primeiro plano (ex.: "code", "OUTLOOK.EXE"); vazio se não souber. Nunca falha.
    fn app_ativo(&self) -> String;
    /// Põe os outros apps que tocam som em `fator` (0 a 1) do volume atual. Não bloqueia por muito tempo.
    fn abaixar_outros(&self, fator: f32);
    /// Volta o volume dos apps abaixados por `abaixar_outros`. Sem nada abaixado, não faz nada.
    fn restaurar_outros(&self);
    /// Bipe curto no volume dado (0 a 100); volume 0 não toca. Não pode bloquear: toca em segundo plano.
    fn bipe(&self, freq: u32, ms: u32, volume: u8);
    /// Começa a capturar o microfone dos ajustes ("" = padrão do sistema). Erro: nenhum microfone abriu.
    fn iniciar_captura(&self, microfone: &str) -> Result<(), String>;
    /// Para a captura e devolve o áudio em 16 kHz mono (vazio se a captura falhou no meio).
    fn parar_captura(&self) -> Vec<i16>;
    /// Ponto de chamada da luz. O nível de áudio chega por outro caminho: a captura da plataforma chama `Luz::audio`.
    fn luz(&self, _estado: EstadoLuz) {}
}

#[derive(Default)]
struct Gravacao {
    gravando: bool,
    inicio: Option<Instant>,
    outra_tecla: bool,
    traduzir: bool, // o ditado em andamento veio do atalho de traduzir
}

type Tarefa = Box<dyn FnOnce(&dyn Plataforma) + Send>;

pub struct Ditado {
    dir: PathBuf,
    plat: Arc<dyn Plataforma>,
    transcritor: Arc<dyn Transcritor>,
    cliente: Arc<dyn escrita::Cliente>,
    ajustes: Mutex<ajustes::Cache>,
    estado: Mutex<Gravacao>,
    // Uma thread só abaixa e restaura o volume: mantém a ordem e não segura o gancho do teclado.
    fila_volume: Mutex<mpsc::Sender<Tarefa>>,
}

impl Ditado {
    pub fn novo(dir: PathBuf, plat: Arc<dyn Plataforma>, transcritor: Arc<dyn Transcritor>, cliente: Arc<dyn escrita::Cliente>) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<Tarefa>();
        let p = plat.clone();
        std::thread::spawn(move || rx.iter().for_each(|t| t(p.as_ref())));
        Arc::new(Ditado {
            dir, plat, transcritor, cliente,
            ajustes: Mutex::default(), estado: Mutex::default(), fila_volume: Mutex::new(tx),
        })
    }

    fn ajustes(&self) -> Value {
        self.ajustes.lock().unwrap().ler(&self.dir).clone()
    }

    fn log(&self, msg: &str) {
        pasta::log(&self.dir, msg);
    }

    fn bipe(&self, freq: u32, ms: u32) {
        let v = self.ajustes()["volume"].as_u64().unwrap_or(60).min(100) as u8;
        self.plat.bipe(freq, ms, v);
    }

    fn volume(&self, t: Tarefa) {
        let _ = self.fila_volume.lock().unwrap().send(t);
    }

    pub fn gravando(&self) -> bool {
        self.estado.lock().unwrap().gravando
    }

    /// Atalho de ditar (ou de traduzir, com `em_ingles`) apertado.
    pub fn comecar(&self, em_ingles: bool) {
        let mut e = self.estado.lock().unwrap();
        if e.gravando {
            return;
        }
        *e = Gravacao { traduzir: em_ingles, inicio: Some(Instant::now()), ..Default::default() };
        let a = self.ajustes();
        if let Err(motivo) = self.plat.iniciar_captura(a["microfone"].as_str().unwrap_or("")) {
            drop(e);
            self.log(&format!("erro: nenhum microfone abriu ({motivo})")); // avisa em vez de falhar calado
            self.plat.luz(EstadoLuz::Erro);
            self.bipe(300, 250);
            return;
        }
        e.gravando = true;
        if a["abaixar_outros"].as_bool().unwrap_or(false) {
            let fator = a["volume_outros"].as_f64().unwrap_or(30.0) as f32 / 100.0;
            self.volume(Box::new(move |p| p.abaixar_outros(fator))); // dentro da trava: o parar() não enfileira antes
        }
        drop(e);
        self.plat.luz(EstadoLuz::Gravando { ingles: em_ingles });
        self.bipe(880, 60);
    }

    /// Ctrl+Win+outra tecla é atalho do sistema, não ditado.
    pub fn marcar_outra_tecla(&self, qual: &str) {
        let mut e = self.estado.lock().unwrap();
        if !e.outra_tecla {
            self.log(&format!("outra tecla: {qual}"));
        }
        e.outra_tecla = true;
    }

    /// Atalho solto: para a captura (e restaura o volume, mesmo se a captura falhou) e transcreve em segundo plano.
    pub fn parar(self: &Arc<Self>) {
        let (audio, duracao, outra_tecla, em_ingles) = {
            let mut e = self.estado.lock().unwrap();
            if !e.gravando {
                return;
            }
            e.gravando = false;
            let audio = self.plat.parar_captura();
            self.volume(Box::new(|p| p.restaurar_outros()));
            let dur = e.inicio.map_or(0.0, |i| i.elapsed().as_secs_f64());
            (audio, dur, e.outra_tecla, e.traduzir) // traduzir lido na trava: um comecar() logo depois não troca o modo deste
        };
        self.log(&format!("gravou {duracao:.1}s, outra_tecla={outra_tecla}"));
        if outra_tecla || duracao < MINIMO_S {
            self.plat.luz(EstadoLuz::Pronto);
            return;
        }
        if audio::pico(&audio) < audio::SILENCIO {
            self.log("ignorado: sem voz"); // o Whisper inventa texto quando recebe silêncio
            self.plat.luz(EstadoLuz::Pronto);
            return;
        }
        self.plat.luz(EstadoLuz::Processando);
        self.bipe(660, 60);
        let d = self.clone();
        std::thread::spawn(move || d.transcrever_e_colar(&audio, duracao, em_ingles));
    }

    pub fn transcrever_e_colar(&self, amostras: &[i16], duracao: f64, em_ingles: bool) {
        if let Err(e) = self.fluxo(amostras, duracao, em_ingles) {
            self.log(&format!("erro: {e}")); // só o tipo da falha: nunca o texto nem a chave
            self.plat.luz(EstadoLuz::Erro);
            self.bipe(300, 250);
        }
    }

    fn fluxo(&self, amostras: &[i16], duracao: f64, em_ingles: bool) -> Result<(), String> {
        let a = self.ajustes();
        let texto = self.transcritor.transcrever(&audio::wav(amostras), a["idioma"].as_str().unwrap_or("pt"), &pasta::prompt_vocabulario(&self.dir))?;
        self.log(&format!("transcrito {} caracteres", texto.chars().count()));
        if texto.is_empty() {
            self.plat.luz(EstadoLuz::Pronto);
            return Ok(());
        }
        let app = self.plat.app_ativo(); // com o foco na janela de destino: escolhe o modo
        let regras = substituicoes::ler_regras(&self.dir);
        // Regra ou comando com defeito não pode perder o ditado: cola como veio.
        let corrigido = catch_unwind(AssertUnwindSafe(|| substituicoes::aplicar(&texto, &a["comandos"], &regras))).unwrap_or_else(|_| {
            self.log("substituições falharam (panic)");
            texto.clone()
        });
        let pedido = if em_ingles { "ingles".to_string() } else { escrita::modo_para(&app, &a) };
        let (final_, modo) = escrita::aplicar(&self.dir, &corrigido, &pedido, self.cliente.clone(), escrita::LIMITE); // nunca falha
        self.plat.colar(&format!("{final_} "))?;
        if modo != pedido && escrita::INSTRUCOES.iter().any(|(m, _)| *m == pedido) {
            self.bipe(300, 150); // a IA falhou: colou o literal, avisa com bipe grave
        }
        self.plat.luz(EstadoLuz::Pronto); // antes de gravar: banco travado não atrasa o texto nem a luz
        historico::gravar(&self.dir, &NovoDitado {
            texto: &final_, original: Some(&texto), app: &app, modo: &modo, idioma: a["idioma"].as_str().unwrap_or("pt"), duracao_s: duracao,
        });
        Ok(())
    }

    /// Cola o texto do ditado mais recente (já corrigido). Histórico vazio: bipe grave e nada colado.
    /// `atraso` deixa o menu da bandeja fechar e o foco voltar para a janela anterior antes do Ctrl+V.
    pub fn colar_ultimo(self: &Arc<Self>, atraso: Duration) {
        let d = self.clone();
        std::thread::spawn(move || {
            if d.gravando() {
                return;
            }
            std::thread::sleep(atraso);
            match historico::listar(&d.dir, "", 1) {
                Ok(itens) if itens.is_empty() => {
                    d.log("colar último: histórico vazio");
                    d.bipe(300, 150);
                }
                Ok(itens) => match d.plat.colar(&format!("{} ", itens[0].texto)) {
                    Ok(()) => d.log("colar último: colado"),
                    Err(e) => {
                        d.log(&format!("colar último: falhou ({e})"));
                        d.bipe(300, 150);
                    }
                },
                Err(_) => {
                    d.log("colar último: falhou (histórico)");
                    d.bipe(300, 150);
                }
            }
        });
    }

    /// Ao sair: sair no meio do ditado não deixa os outros apps baixos. Espera até 3 s.
    pub fn encerrar(&self) {
        let (ack, fim) = mpsc::channel();
        self.volume(Box::new(move |p| {
            p.restaurar_outros();
            let _ = ack.send(());
        }));
        let _ = fim.recv_timeout(Duration::from_secs(3));
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Falsa {
        eventos: Mutex<Vec<String>>,
        app: String,
        sem_microfone: AtomicBool,
        audio: Vec<i16>,
    }
    impl Plataforma for Falsa {
        fn colar(&self, t: &str) -> Result<(), String> {
            self.eventos.lock().unwrap().push(format!("colar:{t}"));
            Ok(())
        }
        fn app_ativo(&self) -> String { self.app.clone() }
        fn abaixar_outros(&self, f: f32) { self.eventos.lock().unwrap().push(format!("abaixar:{f:.1}")); }
        fn restaurar_outros(&self) { self.eventos.lock().unwrap().push("restaurar".into()); }
        fn bipe(&self, f: u32, _ms: u32, _v: u8) { self.eventos.lock().unwrap().push(format!("bipe:{f}")); }
        fn iniciar_captura(&self, _m: &str) -> Result<(), String> {
            if self.sem_microfone.load(Ordering::SeqCst) { Err("sem".into()) } else { Ok(()) }
        }
        fn parar_captura(&self) -> Vec<i16> { self.audio.clone() }
    }

    struct Voz(Result<String, String>);
    impl Transcritor for Voz {
        fn transcrever(&self, _w: &[u8], _i: &str, _v: &str) -> Result<String, String> { self.0.clone() }
    }
    struct Chat(Result<Value, String>, Mutex<Vec<Value>>);
    impl escrita::Cliente for Chat {
        fn enviar(&self, c: &Value) -> Result<Value, String> {
            self.1.lock().unwrap().push(c.clone());
            self.0.clone()
        }
    }

    fn montar(dir: &std::path::Path, plat: Falsa, voz: Result<String, String>, chat: Result<Value, String>) -> (Arc<Ditado>, Arc<Falsa>, Arc<Chat>) {
        let p = Arc::new(plat);
        let c = Arc::new(Chat(chat, Mutex::default()));
        (Ditado::novo(dir.to_path_buf(), p.clone(), Arc::new(Voz(voz)), c.clone()), p, c)
    }

    fn voz_alta() -> Vec<i16> { vec![5000; 16000] }
    fn eventos(p: &Falsa) -> Vec<String> { p.eventos.lock().unwrap().clone() }
    fn ia(t: &str) -> Result<Value, String> { Ok(json!({"choices": [{"message": {"content": t}}]})) }

    fn ditar(d: &Arc<Ditado>, ingles: bool) {
        d.comecar(ingles);
        std::thread::sleep(Duration::from_millis(500)); // acima do mínimo de 0,4 s
        d.parar();
    }

    fn esperar_historico(dir: &std::path::Path) -> Vec<historico::Ditado> {
        for _ in 0..100 {
            if let Ok(l) = historico::listar(dir, "", 5) { if !l.is_empty() { return l; } }
            std::thread::sleep(Duration::from_millis(50));
        }
        vec![]
    }

    #[test]
    fn fluxo_cola_com_substituicao_e_grava_no_historico() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("substituicoes.txt"), "meu email => contato@exemplo.com\n").unwrap();
        let (dit, p, _) = montar(d.path(), Falsa { audio: voz_alta(), app: "teste".into(), ..Default::default() },
                                 Ok("Mande para meu email ponto final".into()), Err("x".into()));
        ditar(&dit, false);
        let h = esperar_historico(d.path());
        assert_eq!(h[0].texto, "Mande para contato@exemplo.com.");
        assert_eq!(h[0].original, "Mande para meu email ponto final");
        assert_eq!(h[0].modo, "literal");
        assert!(eventos(&p).contains(&"colar:Mande para contato@exemplo.com. ".to_string()));
    }

    #[test]
    fn traduzir_usa_modo_ingles_e_ia_falha_cola_o_cru() {
        let d = tempfile::tempdir().unwrap();
        let (dit, p, chat) = montar(d.path(), Falsa { audio: voz_alta(), ..Default::default() }, Ok("olá".into()), ia("hello"));
        ditar(&dit, true);
        let h = esperar_historico(d.path());
        assert_eq!((h[0].texto.as_str(), h[0].modo.as_str()), ("hello", "ingles"));
        assert!(chat.1.lock().unwrap()[0].to_string().contains("inglês"));
        assert!(eventos(&p).contains(&"colar:hello ".to_string()));

        let d2 = tempfile::tempdir().unwrap();
        let (dit, p, _) = montar(d2.path(), Falsa { audio: voz_alta(), ..Default::default() }, Ok("olá".into()), Err("Timeout".into()));
        ditar(&dit, true);
        let h = esperar_historico(d2.path());
        assert_eq!((h[0].texto.as_str(), h[0].modo.as_str()), ("olá", "literal"));
        assert!(eventos(&p).contains(&"bipe:300".to_string())); // bipe grave: a IA falhou
        let log = std::fs::read_to_string(d2.path().join("ditado.log")).unwrap();
        assert!(log.contains("IA (ingles) falhou") && !log.contains("olá"));
    }

    #[test]
    fn modo_do_app_ativo_vale() {
        let d = tempfile::tempdir().unwrap();
        let (dit, _, chat) = montar(d.path(), Falsa { audio: voz_alta(), app: "OUTLOOK.EXE".into(), ..Default::default() }, Ok("oi".into()), ia("Prezado."));
        ditar(&dit, false);
        assert_eq!(esperar_historico(d.path())[0].modo, "formal");
        assert_eq!(chat.1.lock().unwrap().len(), 1);
    }

    #[test]
    fn abaixa_e_restaura_o_volume_mesmo_com_erro() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("ajustes.json"), r#"{"abaixar_outros": true, "volume_outros": 30}"#).unwrap();
        let (dit, p, _) = montar(d.path(), Falsa { audio: voz_alta(), ..Default::default() }, Err("HTTPError 500".into()), Err("x".into()));
        ditar(&dit, false);
        std::thread::sleep(Duration::from_millis(300));
        let ev = eventos(&p);
        let i = ev.iter().position(|e| e == "abaixar:0.3").expect("abaixou");
        assert!(ev[i..].contains(&"restaurar".to_string()), "{ev:?}");
        let log = std::fs::read_to_string(d.path().join("ditado.log")).unwrap();
        assert!(log.contains("erro: HTTPError 500"));
    }

    #[test]
    fn curto_e_silencio_nao_transcrevem() {
        let d = tempfile::tempdir().unwrap();
        let (dit, p, _) = montar(d.path(), Falsa { audio: vec![10; 16000], ..Default::default() }, Ok("não deve".into()), Err("x".into()));
        ditar(&dit, false);
        dit.comecar(false);
        dit.parar(); // curto demais
        std::thread::sleep(Duration::from_millis(200));
        assert!(!eventos(&p).iter().any(|e| e.starts_with("colar")));
        let log = std::fs::read_to_string(d.path().join("ditado.log")).unwrap();
        assert!(log.contains("ignorado: sem voz"));
    }

    #[test]
    fn sem_microfone_avisa_e_nao_grava() {
        let d = tempfile::tempdir().unwrap();
        let f = Falsa { audio: voz_alta(), ..Default::default() };
        f.sem_microfone.store(true, Ordering::SeqCst);
        let (dit, p, _) = montar(d.path(), f, Ok("x".into()), Err("x".into()));
        dit.comecar(false);
        assert!(!dit.gravando());
        assert!(eventos(&p).contains(&"bipe:300".to_string()));
    }

    #[test]
    fn outra_tecla_cancela() {
        let d = tempfile::tempdir().unwrap();
        let (dit, p, _) = montar(d.path(), Falsa { audio: voz_alta(), ..Default::default() }, Ok("x".into()), Err("x".into()));
        dit.comecar(false);
        dit.marcar_outra_tecla("keycode=42");
        std::thread::sleep(Duration::from_millis(500));
        dit.parar();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!eventos(&p).iter().any(|e| e.starts_with("colar")));
    }

    #[test]
    fn colar_ultimo_e_historico_vazio() {
        let d = tempfile::tempdir().unwrap();
        let (dit, p, _) = montar(d.path(), Falsa::default(), Ok("x".into()), Err("x".into()));
        dit.colar_ultimo(Duration::ZERO);
        assert!(esperar_evento(&p, "bipe:300"));
        historico::gravar(d.path(), &NovoDitado { texto: "Ligar para o motorista", original: None, app: "", modo: "", idioma: "pt", duracao_s: 1.0 });
        dit.colar_ultimo(Duration::ZERO);
        assert!(esperar_evento(&p, "colar:Ligar para o motorista "));
    }

    /// Espera o evento aparecer (até 5 s): o colar roda em thread e o CI sob carga passa dos 300 ms.
    fn esperar_evento(p: &Falsa, alvo: &str) -> bool {
        let fim = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < fim {
            if eventos(p).iter().any(|e| e == alvo) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn log_nao_leva_texto_nem_chave() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("chave.txt"), "gsk_falsa").unwrap();
        let (dit, _, _) = montar(d.path(), Falsa { audio: voz_alta(), ..Default::default() }, Ok("frase secreta".into()), Err("x".into()));
        ditar(&dit, false);
        esperar_historico(d.path());
        let log = std::fs::read_to_string(d.path().join("ditado.log")).unwrap();
        assert!(!log.contains("frase secreta") && !log.contains("gsk_falsa"));
    }
}
