//! Linux (X11), porta de `app/plataforma_linux.py`:
//! - atalhos pela extensão RECORD (sem root nem grupo input), casados por `talk_core::casador::Casador`;
//! - colar por XTest (Ctrl+V; Ctrl+Shift+V em terminais) devolvendo a área de transferência;
//! - microfone pelo `cpal`, bipe pelo `cpal`, volume dos outros apps pelo `pactl`.

use arboard::Clipboard;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use talk_core::casador::{Acao, Casador};
use talk_core::ditado::{Ditado, Plataforma};
use talk_core::{ajustes, audio, pasta};
use x11rb::connection::Connection;
use x11rb::protocol::record::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const TERMINAIS: [&str; 10] = ["terminal", "xterm", "urxvt", "kitty", "alacritty", "konsole", "tilix", "terminator", "wezterm", "foot"];
const GRUPOS: [(&str, [u32; 2]); 4] = [
    ("ctrl", [0xffe3, 0xffe4]), ("alt", [0xffe9, 0xffea]), ("shift", [0xffe1, 0xffe2]), ("win", [0xffeb, 0xffec]),
];

// ---------- teclado ----------
/// Tabela keysym -> keycode do servidor X.
struct Teclado {
    primeiro: u8,
    por_tecla: usize,
    keysyms: Vec<u32>,
}

impl Teclado {
    fn ler(c: &RustConnection) -> Result<Self, String> {
        let (min, max) = (c.setup().min_keycode, c.setup().max_keycode);
        let m = c.get_keyboard_mapping(min, max - min + 1).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        Ok(Teclado { primeiro: min, por_tecla: m.keysyms_per_keycode as usize, keysyms: m.keysyms })
    }

    /// Primeira tecla que produz o keysym (colunas mais baixas primeiro, como o python-xlib).
    fn keycode(&self, ks: u32) -> Option<u8> {
        (0..self.por_tecla).find_map(|col| {
            self.keysyms.chunks(self.por_tecla).position(|t| t.get(col) == Some(&ks)).map(|i| self.primeiro + i as u8)
        })
    }
}

/// Keysym da tecla do atalho ("v", "5", "space", "f5").
fn keysym(nome: &str) -> Option<u32> {
    let mut c = nome.chars();
    match (c.next(), c.next()) {
        (Some(u), None) => Some(u as u32),
        _ if nome == "space" => Some(0x20),
        _ => nome.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=12).contains(n)).map(|n| 0xffbd + n),
    }
}

/// keycode -> nome ("ctrl", "alt", "shift", "win" e a tecla do atalho, se houver).
fn codigos(t: &Teclado, alvos: &[&[String]]) -> HashMap<u8, String> {
    let mut mapa = HashMap::new();
    for (nome, ks) in GRUPOS {
        ks.iter().filter_map(|k| t.keycode(*k)).for_each(|c| { mapa.insert(c, nome.to_string()); });
    }
    for nome in alvos.iter().flat_map(|a| a.iter()).filter(|n| !GRUPOS.iter().any(|(g, _)| g == n)) {
        if let Some(c) = keysym(nome).and_then(|k| t.keycode(k)) {
            mapa.insert(c, nome.clone());
        }
    }
    mapa
}

// ---------- gancho de teclado ----------
/// Escuta os atalhos (ditar, traduzir e colar o último) numa thread. Os ajustes são relidos a cada tecla: valem sem reiniciar.
pub fn iniciar_gancho(ditado: Arc<Ditado>, dir: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = escutar(&ditado, &dir) {
            pasta::log(&dir, &format!("gancho do teclado falhou: {e}"));
        }
    });
}

fn escutar(ditado: &Arc<Ditado>, dir: &Path) -> Result<(), String> {
    let e = |e: &dyn std::fmt::Display| e.to_string();
    let (local, _) = RustConnection::connect(None).map_err(|x| e(&x))?;
    let (rec, _) = RustConnection::connect(None).map_err(|x| e(&x))?;
    let teclado = Teclado::ler(&local)?;
    let zero = record::Range8 { first: 0, last: 0 };
    let zero_ext = record::ExtRange { major: zero, minor: record::Range16 { first: 0, last: 0 } };
    let faixa = record::Range {
        core_requests: zero, core_replies: zero, ext_requests: zero_ext, ext_replies: zero_ext, delivered_events: zero,
        device_events: record::Range8 { first: 2, last: 3 }, // KeyPress e KeyRelease
        errors: zero, client_started: false, client_died: false,
    };
    let ctx = rec.generate_id().map_err(|x| e(&x))?;
    rec.record_create_context(ctx, 0, &[record::CS::ALL_CLIENTS.into()], &[faixa]).map_err(|x| e(&x))?.check().map_err(|x| e(&x))?;

    let mut cache = ajustes::Cache::default();
    let casador = Arc::new(Mutex::new(Casador::default()));
    let mut vigente: Option<([Option<Vec<String>>; 3], HashMap<u8, String>)> = None;
    for resposta in rec.record_enable_context(ctx).map_err(|x| e(&x))? {
        let r = resposta.map_err(|x| e(&x))?;
        if r.category != 0 || r.client_swapped { // 0 = FromServer
            continue;
        }
        for ev in r.data.chunks_exact(32) {
            let (tipo, codigo) = (ev[0] & 0x7f, ev[1]);
            if tipo != 2 && tipo != 3 {
                continue;
            }
            let a = cache.ler(dir);
            let alvos = ["atalho_ditar", "atalho_colar", "atalho_traduzir"].map(|k| ajustes::validar_atalho(a[k].as_str().unwrap_or("")));
            if vigente.as_ref().is_none_or(|(v, _)| *v != alvos) {
                let planos: Vec<&[String]> = alvos.iter().flatten().map(Vec::as_slice).collect();
                vigente = Some((alvos.clone(), codigos(&teclado, &planos)));
            }
            let nome = vigente.as_ref().unwrap().1.get(&codigo).map_or("outra", String::as_str);
            let [ditar, colar, traduzir] = &alvos;
            let acao = casador.lock().unwrap().evento(
                nome, tipo == 2, ditar.as_deref().unwrap_or(&[]), ditado.gravando(), colar.as_deref(), traduzir.as_deref(),
            );
            match acao {
                Some(Acao::Colar) => colar_ao_soltar(casador.clone(), ditado.clone()),
                Some(a @ (Acao::Iniciar | Acao::Traduzir)) => ditado.comecar(a == Acao::Traduzir),
                Some(Acao::Parar) => ditado.parar(),
                Some(Acao::Outra) => ditado.marcar_outra_tecla(&format!("keycode={codigo}")),
                None => {}
            }
        }
    }
    Ok(())
}

/// Atalho de colar: espera soltar as teclas (até 2 s, numa thread) e só então cola; com Alt/Win apertados viraria outro atalho.
fn colar_ao_soltar(casador: Arc<Mutex<Casador>>, ditado: Arc<Ditado>) {
    std::thread::spawn(move || {
        let fim = Instant::now() + Duration::from_secs(2);
        while !casador.lock().unwrap().apertadas.is_empty() && Instant::now() < fim {
            std::thread::sleep(Duration::from_millis(20));
        }
        ditado.colar_ultimo(Duration::from_millis(100));
    });
}

// ---------- plataforma ----------
struct Abaixado {
    indice: u64,
    app: Option<String>,
    antes: Vec<i64>,
}

/// Conexão X usada para colar e ler a janela ativa.
struct ConexaoX {
    conn: RustConnection,
    raiz: u32,
    teclado: Teclado,
}

enum Captura {
    Arquivo(Vec<i16>), // RENANTALK_AUDIO_TESTE: o Xvfb não tem microfone
    Microfone { parar: mpsc::Sender<()>, fim: mpsc::Receiver<Vec<i16>> },
}

pub struct Linux {
    dir: PathBuf,
    x: Mutex<Option<ConexaoX>>,
    clip: Mutex<Option<Clipboard>>,
    captura: Mutex<Option<Captura>>,
    abaixados: Mutex<Vec<Abaixado>>,
    // O PipeWire guarda o volume por app: um app que fechou ainda baixo voltaria baixo. Fica aqui até reaparecer.
    pendentes: Mutex<HashMap<String, Vec<i64>>>,
}

pub fn nova(dir: &Path) -> Arc<dyn Plataforma> {
    Arc::new(Linux {
        dir: dir.to_path_buf(), x: Mutex::new(None), clip: Mutex::new(None), captura: Mutex::new(None),
        abaixados: Mutex::default(), pendentes: Mutex::default(),
    })
}

impl Linux {
    fn log(&self, msg: &str) {
        pasta::log(&self.dir, msg);
    }

    /// Executa `f` com a conexão X (aberta na primeira vez).
    fn com_x<T>(&self, f: impl FnOnce(&ConexaoX) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.x.lock().unwrap();
        if g.is_none() {
            let (conn, n) = RustConnection::connect(None).map_err(|e| e.to_string())?;
            let raiz = conn.setup().roots[n].root;
            let teclado = Teclado::ler(&conn)?;
            *g = Some(ConexaoX { conn, raiz, teclado });
        }
        let r = f(g.as_ref().unwrap());
        if r.is_err() {
            *g = None; // reconecta na próxima vez
        }
        r
    }

    /// "instância classe" da janela ativa, em minúsculas.
    fn classe_ativa(&self) -> String {
        self.com_x(|x| {
            let atom = |n: &str| x.conn.intern_atom(false, n.as_bytes()).map_err(|e| e.to_string())?.reply().map(|r| r.atom).map_err(|e| e.to_string());
            let ativa = x.conn.get_property(false, x.raiz, atom("_NET_ACTIVE_WINDOW")?, AtomEnum::ANY, 0, 1).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
            let janela = ativa.value32().and_then(|mut v| v.next()).ok_or("sem janela ativa")?;
            let c = x.conn.get_property(false, janela, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
            Ok(String::from_utf8_lossy(&c.value).split('\0').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ").to_lowercase())
        }).unwrap_or_default()
    }

    fn teclar(&self, keysyms: &[u32]) -> Result<(), String> {
        self.com_x(|x| {
            let codigos: Vec<u8> = keysyms.iter().map(|k| x.teclado.keycode(*k).ok_or("tecla sem keycode")).collect::<Result<_, _>>()?;
            let enviar = |tipo: u8, c: &u8| x.conn.xtest_fake_input(tipo, *c, 0, x.raiz, 0, 0, 0).map(|_| ()).map_err(|e| e.to_string());
            codigos.iter().try_for_each(|c| enviar(2, c))?; // KeyPress
            codigos.iter().rev().try_for_each(|c| enviar(3, c))?; // KeyRelease
            x.conn.get_input_focus().map_err(|e| e.to_string())?.reply().map(|_| ()).map_err(|e| e.to_string()) // sync
        })
    }
}

impl Plataforma for Linux {
    fn colar(&self, texto: &str) -> Result<(), String> {
        let mut g = self.clip.lock().unwrap();
        if g.is_none() {
            *g = Some(Clipboard::new().map_err(|e| e.to_string())?);
        }
        let clip = g.as_mut().unwrap();
        let anterior = clip.get_text().ok(); // vazia (ou com não-texto): volta vazia, o ditado não fica nela
        clip.set_text(texto).map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(50));
        let terminal = TERMINAIS.iter().any(|t| self.classe_ativa().contains(t));
        let r = if terminal { self.teclar(&[0xffe3, 0xffe1, 0x76]) } else { self.teclar(&[0xffe3, 0x76]) }; // Ctrl(+Shift)+V
        std::thread::sleep(Duration::from_millis(300));
        let _ = match anterior { Some(a) => clip.set_text(a), None => clip.clear() };
        r
    }

    fn app_ativo(&self) -> String {
        self.classe_ativa().split_whitespace().last().unwrap_or("").to_string()
    }

    /// Põe cada app que toca som em `fator` do volume atual. O bipe de início ainda pode estar tocando: pula o fluxo do
    /// nosso processo (`application.process.id`). O bipe de fim sai num fluxo novo, que não está nesta lista.
    fn abaixar_outros(&self, fator: f32) {
        let entradas = match sink_inputs() {
            Ok(e) => e,
            Err(e) => return self.log(&format!("abaixar volume: {e}")),
        };
        let mut feitos = vec![];
        for e in entradas { // um fluxo que sumiu ou sem volume não impede abaixar os outros
            let f = || -> Result<Option<Abaixado>, String> {
                let pid = e["properties"]["application.process.id"].as_str().map(String::from).or_else(|| e["properties"]["application.process.id"].as_u64().map(|n| n.to_string()));
                if pid.as_deref() == Some(&std::process::id().to_string()) {
                    return Ok(None); // os bipes do RenanTalk ficam no volume dos ajustes
                }
                let mut antes = canais(&e);
                let indice = e["index"].as_u64().ok_or("sem index")?;
                if antes.is_empty() {
                    return Ok(None);
                }
                let app = app_de(&e);
                if let Some(p) = app.as_ref().and_then(|a| self.pendentes.lock().unwrap().get(a).cloned()).filter(|p| p.len() == antes.len()) {
                    antes = p;
                }
                definir_volume(indice, &antes.iter().map(|v| (*v as f64 * fator as f64).round() as i64).collect::<Vec<_>>())?;
                if let Some(a) = &app {
                    self.pendentes.lock().unwrap().remove(a); // só some depois de abaixar: se o fluxo sumiu no meio, continua pendente
                }
                Ok(Some(Abaixado { indice, app, antes }))
            };
            match f() {
                Ok(Some(a)) => feitos.push(a),
                Ok(None) => {}
                Err(err) => self.log(&format!("abaixar volume: {err}")),
            }
        }
        *self.abaixados.lock().unwrap() = feitos;
    }

    /// Volta sempre o volume de antes (o Spotify reage à mudança de fora e ficaria no valor errado se respeitássemos o do meio).
    /// Fluxo que sumiu (o app fechou ou trocou de fluxo): acerta o fluxo novo do mesmo app ou fica pendente.
    fn restaurar_outros(&self) {
        let feitos = std::mem::take(&mut *self.abaixados.lock().unwrap());
        let mut sumiram: HashMap<String, Vec<i64>> = HashMap::new();
        let mut restaurados = vec![];
        for a in &feitos {
            match definir_volume(a.indice, &a.antes) {
                Ok(()) => restaurados.push(a.indice),
                Err(e) => {
                    self.log(&format!("restaurar volume: {e}"));
                    if let Some(app) = &a.app {
                        sumiram.insert(app.clone(), a.antes.clone());
                    }
                }
            }
        }
        if sumiram.is_empty() {
            return;
        }
        for e in sink_inputs().unwrap_or_default() {
            let (Some(app), Some(indice)) = (app_de(&e), e["index"].as_u64()) else { continue };
            // fluxo já restaurado acima não leva o volume de outro fluxo do mesmo app (vários "Chromium", abas do Firefox)
            if let Some(antes) = sumiram.get(&app).filter(|_| !restaurados.contains(&indice)) {
                match definir_volume(indice, antes) {
                    Ok(()) => { sumiram.remove(&app); }
                    Err(err) => self.log(&format!("restaurar volume: {err}")),
                }
            }
        }
        self.pendentes.lock().unwrap().extend(sumiram);
    }

    fn bipe(&self, freq: u32, ms: u32, volume: u8) {
        if volume == 0 {
            return;
        }
        std::thread::spawn(move || tocar(freq, ms, volume)); // sem saída de áudio não é motivo para travar o ditado
    }

    fn iniciar_captura(&self, microfone: &str) -> Result<(), String> {
        let nova = if let Ok(arq) = std::env::var("RENANTALK_AUDIO_TESTE") {
            Captura::Arquivo(audio::ler_wav(&std::fs::read(&arq).map_err(|e| e.to_string())?)?)
        } else {
            self.abrir_microfone(microfone)?
        };
        *self.captura.lock().unwrap() = Some(nova);
        Ok(())
    }

    fn parar_captura(&self) -> Vec<i16> {
        match self.captura.lock().unwrap().take() {
            Some(Captura::Arquivo(a)) => a,
            Some(Captura::Microfone { parar, fim }) => {
                let _ = parar.send(());
                fim.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
            }
            None => vec![],
        }
    }
}

// ---------- volume (pactl) ----------
/// Roda `pactl` com limite de 2 s.
fn pactl(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut filho = Command::new("pactl").args(args).env("LC_ALL", "C").stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let mut saida = filho.stdout.take().unwrap();
    let leitor = std::thread::spawn(move || { let mut b = vec![]; let _ = saida.read_to_end(&mut b); b });
    let fim = Instant::now() + Duration::from_secs(2);
    loop {
        match filho.try_wait().map_err(|e| e.to_string())? {
            Some(s) if s.success() => return Ok(leitor.join().unwrap_or_default()),
            Some(s) => return Err(format!("pactl saiu com {s}")),
            None if Instant::now() > fim => {
                let _ = filho.kill();
                return Err("pactl: tempo esgotado".into());
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

fn sink_inputs() -> Result<Vec<Value>, String> {
    let json: Value = serde_json::from_slice(&pactl(&["-f", "json", "list", "sink-inputs"])?).map_err(|e| e.to_string())?;
    Ok(json.as_array().cloned().unwrap_or_default())
}

fn definir_volume(indice: u64, canais: &[i64]) -> Result<(), String> {
    let (i, c) = (indice.to_string(), canais.iter().map(i64::to_string).collect::<Vec<_>>());
    let mut args = vec!["set-sink-input-volume", i.as_str()];
    args.extend(c.iter().map(String::as_str));
    pactl(&args).map(|_| ())
}

fn app_de(e: &Value) -> Option<String> {
    e["properties"]["application.name"].as_str().map(String::from)
}

/// Volume atual de cada canal do fluxo.
fn canais(e: &Value) -> Vec<i64> {
    e["volume"].as_object().into_iter().flatten().filter_map(|(_, c)| c["value"].as_i64()).collect()
}

// ---------- áudio (cpal) ----------
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

impl Linux {
    /// Abre o microfone dos ajustes ("" = padrão). Nome que não existe ou não abre: cai no padrão e registra no log.
    fn abrir_microfone(&self, microfone: &str) -> Result<Captura, String> {
        let (parar_tx, parar_rx) = mpsc::channel::<()>();
        let (fim_tx, fim_rx) = mpsc::channel::<Vec<i16>>();
        let (pronto_tx, pronto_rx) = mpsc::channel::<Result<(), String>>();
        let (dir, mic) = (self.dir.clone(), microfone.to_string());
        // O fluxo do cpal não é Send em todos os sistemas: nasce, vive e morre numa thread só.
        std::thread::spawn(move || {
            let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
            match abrir_fluxo(&mic, &dir, buf.clone()) {
                Ok((fluxo, taxa, n_canais)) => {
                    let _ = pronto_tx.send(Ok(()));
                    let _ = parar_rx.recv();
                    drop(fluxo);
                    let dados = std::mem::take(&mut *buf.lock().unwrap());
                    let _ = fim_tx.send(audio::para_16k_mono(&dados, taxa, n_canais));
                }
                Err(e) => { let _ = pronto_tx.send(Err(e)); }
            }
        });
        pronto_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "microfone não respondeu".to_string())??;
        Ok(Captura::Microfone { parar: parar_tx, fim: fim_rx })
    }
}

fn abrir_fluxo(mic: &str, dir: &Path, buf: Arc<Mutex<Vec<f32>>>) -> Result<(cpal::Stream, u32, usize), String> {
    let host = cpal::default_host();
    let escolhido = (!mic.is_empty()).then(|| {
        let nomes = || host.input_devices().into_iter().flatten().map(|d| (d.name().unwrap_or_default(), d));
        nomes().find(|(n, _)| n == mic).or_else(|| nomes().find(|(n, _)| n.to_lowercase().contains(&mic.to_lowercase()))).map(|(_, d)| d)
    }).flatten();
    if !mic.is_empty() && escolhido.is_none() {
        pasta::log(dir, &format!("microfone {mic:?} não encontrado, usando o padrão do sistema"));
    }
    let padrao = || host.default_input_device().ok_or("nenhum microfone".to_string());
    if let Some(d) = escolhido {
        match ligar(&d, buf.clone()) {
            Ok(r) => return Ok(r),
            Err(e) => pasta::log(dir, &format!("microfone {mic:?} não abriu ({e}), usando o padrão do sistema")),
        }
    }
    ligar(&padrao()?, buf)
}

/// Liga o fluxo de entrada; os quadros vão para `buf` como f32 intercalado.
fn ligar(d: &cpal::Device, buf: Arc<Mutex<Vec<f32>>>) -> Result<(cpal::Stream, u32, usize), String> {
    let cfg = d.default_input_config().map_err(|e| e.to_string())?;
    let (taxa, n_canais, formato) = (cfg.sample_rate().0, cfg.channels() as usize, cfg.sample_format());
    let erro = |e: cpal::StreamError| eprintln!("áudio: {e}");
    let c = cfg.into();
    let fluxo = match formato {
        cpal::SampleFormat::F32 => d.build_input_stream(&c, move |q: &[f32], _| buf.lock().unwrap().extend_from_slice(q), erro, None),
        cpal::SampleFormat::I16 => d.build_input_stream(&c, move |q: &[i16], _| buf.lock().unwrap().extend(q.iter().map(|s| *s as f32 / 32768.0)), erro, None),
        cpal::SampleFormat::U16 => d.build_input_stream(&c, move |q: &[u16], _| buf.lock().unwrap().extend(q.iter().map(|s| (*s as f32 - 32768.0) / 32768.0)), erro, None),
        f => return Err(format!("formato de áudio não suportado: {f:?}")),
    }.map_err(|e| e.to_string())?;
    fluxo.play().map_err(|e| e.to_string())?;
    Ok((fluxo, taxa, n_canais))
}

/// Toca o bipe na saída padrão e espera ele terminar.
fn tocar(freq: u32, ms: u32, volume: u8) {
    let Some(d) = cpal::default_host().default_output_device() else { return };
    let Ok(cfg) = d.default_output_config() else { return };
    let (taxa, n_canais, formato) = (cfg.sample_rate().0, cfg.channels() as usize, cfg.sample_format());
    let onda = audio::onda_bipe(freq, ms, volume, taxa);
    let mut i = 0;
    let c = cfg.into();
    let erro = |_| {};
    let fluxo = match formato {
        cpal::SampleFormat::F32 => d.build_output_stream(&c, move |q: &mut [f32], _| {
            for quadro in q.chunks_mut(n_canais) {
                quadro.fill(onda.get(i).copied().unwrap_or(0.0));
                i += 1;
            }
        }, erro, None),
        cpal::SampleFormat::I16 => d.build_output_stream(&c, move |q: &mut [i16], _| {
            for quadro in q.chunks_mut(n_canais) {
                quadro.fill((onda.get(i).copied().unwrap_or(0.0) * 32767.0) as i16);
                i += 1;
            }
        }, erro, None),
        _ => return,
    };
    let Ok(fluxo) = fluxo else { return };
    if fluxo.play().is_ok() {
        std::thread::sleep(Duration::from_millis(ms as u64 + 80));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn keysym_das_teclas_do_atalho() {
        assert_eq!(keysym("v"), Some(0x76));
        assert_eq!(keysym("5"), Some(0x35));
        assert_eq!(keysym("space"), Some(0x20));
        assert_eq!(keysym("f5"), Some(0xffc2));
        assert_eq!(keysym("f13"), None);
        assert_eq!(keysym("ctrl"), None);
    }

    #[test]
    fn teclado_acha_o_primeiro_keycode() {
        // 2 keysyms por tecla; keycodes a partir de 8
        let t = Teclado { primeiro: 8, por_tecla: 2, keysyms: vec![0x61, 0x41, 0x76, 0x56, 0xffeb, 0] };
        assert_eq!(t.keycode(0x76), Some(9));
        assert_eq!(t.keycode(0x41), Some(8)); // coluna 1 só vale depois da coluna 0 de todas
        assert_eq!(t.keycode(0xffeb), Some(10));
        assert_eq!(t.keycode(0x7a), None);
        let m = codigos(&t, &[&["win".to_string(), "v".to_string()]]);
        assert_eq!(m.get(&10).map(String::as_str), Some("win"));
        assert_eq!(m.get(&9).map(String::as_str), Some("v"));
    }

    #[test]
    fn canais_e_app_do_pactl() {
        let e: Value = serde_json::json!({"index": 7, "properties": {"application.name": "Spotify"},
            "volume": {"front-left": {"value": 65536}, "front-right": {"value": 32768}}});
        assert_eq!(canais(&e), vec![65536, 32768]);
        assert_eq!(app_de(&e).as_deref(), Some("Spotify"));
    }
}
