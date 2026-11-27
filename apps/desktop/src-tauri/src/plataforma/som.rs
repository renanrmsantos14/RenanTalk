//! Microfone e bipe pelo `cpal` (ALSA/Pulse no Linux, WASAPI no Windows): igual nos dois sistemas.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use talk_core::{audio, pasta};

/// Recebe o nível de áudio (0 a 1) a cada pedaço capturado: alimenta a luz, como o `ao_audio` do Python.
pub type Nivel = Arc<dyn Fn(f32) + Send + Sync>;

pub enum Captura {
    /// RENANTALK_AUDIO_TESTE: o Xvfb e o runner do CI não têm microfone. Os níveis saem do arquivo em tempo real.
    Arquivo { audio: Vec<i16>, parada: Arc<AtomicBool> },
    Microfone { parar: mpsc::Sender<()>, fim: mpsc::Receiver<Vec<i16>> },
}

/// Começa a captura: o arquivo de `RENANTALK_AUDIO_TESTE`, se existir, ou o microfone dos ajustes ("" = padrão).
pub fn iniciar(dir: &Path, microfone: &str, nivel: Nivel) -> Result<Captura, String> {
    if let Ok(arq) = std::env::var("RENANTALK_AUDIO_TESTE") {
        let amostras = audio::ler_wav(&std::fs::read(&arq).map_err(|e| e.to_string())?)?;
        let parada = Arc::new(AtomicBool::new(false));
        let (a, p) = (amostras.clone(), parada.clone());
        std::thread::spawn(move || {
            for janela in a.chunks(audio::TAXA as usize / 20) { // 50 ms por vez, como o microfone entrega
                if p.load(Ordering::Relaxed) {
                    break;
                }
                let f: Vec<f32> = janela.iter().map(|s| *s as f32 / 32768.0).collect();
                nivel(audio::nivel_luz(&f, 1));
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        return Ok(Captura::Arquivo { audio: amostras, parada });
    }
    abrir_microfone(dir.to_path_buf(), microfone, nivel)
}

/// Para a captura e devolve o áudio em 16 kHz mono (vazio se a captura falhou no meio).
pub fn parar(c: Captura) -> Vec<i16> {
    match c {
        Captura::Arquivo { audio, parada } => {
            parada.store(true, Ordering::Relaxed);
            audio
        }
        Captura::Microfone { parar, fim } => {
            let _ = parar.send(());
            fim.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
        }
    }
}

/// Abre o microfone dos ajustes ("" = padrão). Nome que não existe ou não abre: cai no padrão e registra no log.
fn abrir_microfone(dir: PathBuf, microfone: &str, nivel: Nivel) -> Result<Captura, String> {
    let (parar_tx, parar_rx) = mpsc::channel::<()>();
    let (fim_tx, fim_rx) = mpsc::channel::<Vec<i16>>();
    let (pronto_tx, pronto_rx) = mpsc::channel::<Result<(), String>>();
    let mic = microfone.to_string();
    // O fluxo do cpal não é Send em todos os sistemas: nasce, vive e morre numa thread só.
    std::thread::spawn(move || {
        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        match abrir_fluxo(&mic, &dir, buf.clone(), nivel) {
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

fn abrir_fluxo(mic: &str, dir: &Path, buf: Arc<Mutex<Vec<f32>>>, nivel: Nivel) -> Result<(cpal::Stream, u32, usize), String> {
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
        match ligar(&d, buf.clone(), nivel.clone()) {
            Ok(r) => return Ok(r),
            Err(e) => pasta::log(dir, &format!("microfone {mic:?} não abriu ({e}), usando o padrão do sistema")),
        }
    }
    ligar(&padrao()?, buf, nivel)
}

/// Liga o fluxo de entrada; os quadros vão para `buf` como f32 intercalado.
fn ligar(d: &cpal::Device, buf: Arc<Mutex<Vec<f32>>>, nivel: Nivel) -> Result<(cpal::Stream, u32, usize), String> {
    let cfg = d.default_input_config().map_err(|e| e.to_string())?;
    let (taxa, n_canais, formato) = (cfg.sample_rate().0, cfg.channels() as usize, cfg.sample_format());
    let erro = |e: cpal::StreamError| eprintln!("áudio: {e}");
    let c = cfg.into();
    // Cada pedaço entra no buffer e, convertido para f32, vira o nível da luz.
    let guardar = move |q: &[f32]| {
        nivel(audio::nivel_luz(q, n_canais));
        buf.lock().unwrap().extend_from_slice(q);
    };
    let fluxo = match formato {
        cpal::SampleFormat::F32 => d.build_input_stream(&c, move |q: &[f32], _| guardar(q), erro, None),
        cpal::SampleFormat::I16 => d.build_input_stream(&c, move |q: &[i16], _| guardar(&q.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>()), erro, None),
        cpal::SampleFormat::U16 => d.build_input_stream(&c, move |q: &[u16], _| guardar(&q.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect::<Vec<_>>()), erro, None),
        f => return Err(format!("formato de áudio não suportado: {f:?}")),
    }.map_err(|e| e.to_string())?;
    fluxo.play().map_err(|e| e.to_string())?;
    Ok((fluxo, taxa, n_canais))
}

/// Toca o bipe na saída padrão e espera ele terminar.
pub fn tocar(freq: u32, ms: u32, volume: u8) {
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
