//! Luz do ditado (porta de `app/luz.py`, `app/bandeira.py` e `CORES_LUZ`/`ESPESSURAS` de `app/ajustes.py`).
//!
//! - `desenhar`: função pura que gera o quadro (RGBA pré-multiplicado) a partir dos ajustes, do estado, do
//!   nível de áudio, da bandeira e da largura da tela. É a única fonte do desenho: Linux e Windows só o exibem.
//! - `Controle`: a máquina de estados (idle, gravando, processando, pronto, erro) e a suavização do nível.
//! - `Luz` + `Superficie`: a thread que liga as duas. Cada sistema implementa só a `Superficie` (janela,
//!   posição no monitor e envio do quadro); o laço de 40 fps e as regras de aparecer e sumir são comuns.

use crate::ditado::EstadoLuz;
use crate::{ajustes, pasta};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

pub const ALTURA: u32 = 36; // px da faixa desenhada (núcleo + brilho), na espessura média
const NUCLEO_PX: f32 = 2.0;
pub const FPS: u64 = 40;

type Rgb = [f32; 3];
const ERRO_NUCLEO: Rgb = [254.0, 202.0, 202.0];
const ERRO_BRILHO: Rgb = [248.0, 113.0, 113.0];
const ERRO_NEVOA: Rgb = [220.0, 38.0, 38.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paleta {
    pub centro: Rgb,
    pub ponta: Rgb,
    pub brilho: Rgb,
    pub nevoa: Rgb,
}

/// `CORES_LUZ` do Python (o azul é o original; o nome inválido cai no azul).
pub fn paleta(cor: &str) -> Paleta {
    let p = |c: [u8; 3]| c.map(f32::from);
    let (centro, ponta, brilho, nevoa) = match cor {
        "verde" => ([134, 239, 172], [22, 163, 74], [34, 197, 94], [21, 128, 61]),
        "roxo" => ([196, 167, 255], [124, 58, 237], [139, 92, 246], [91, 33, 182]),
        "ambar" => ([253, 224, 130], [217, 119, 6], [245, 158, 11], [180, 83, 9]),
        _ => ([120, 170, 255], [37, 99, 235], [45, 110, 255], [30, 64, 200]),
    };
    Paleta { centro: p(centro), ponta: p(ponta), brilho: p(brilho), nevoa: p(nevoa) }
}

/// `ESPESSURAS` do Python: multiplica a altura, o núcleo e o brilho.
pub fn espessura(nome: &str) -> f32 {
    match nome {
        "fina" => 0.5,
        "grossa" => 2.0,
        _ => 1.0,
    }
}

/// Ajustes da luz, lidos a cada ditado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub ligada: bool,
    pub topo: bool,
    pub k: f32,
    pub paleta: Paleta,
}

impl Config {
    /// `a` são os ajustes já normalizados (`ajustes::ler`).
    pub fn de_ajustes(a: &Value) -> Self {
        let l = &a["luz"];
        Config {
            ligada: l["ligada"].as_bool().unwrap_or(true),
            topo: l["posicao"].as_str() == Some("topo"),
            k: espessura(l["espessura"].as_str().unwrap_or("media")),
            paleta: paleta(l["cor"].as_str().unwrap_or("azul")),
        }
    }

    /// Altura da janela, em px.
    pub fn altura(&self) -> u32 {
        (ALTURA as f32 * self.k).round() as u32
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::de_ajustes(&ajustes::padroes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    Gravando,
    Processando,
    Pronto,
    Erro,
}

/// Quadro RGBA com a cor já multiplicada pelo alfa (o que o `UpdateLayeredWindow` e o ARGB do X11 esperam).
/// A linha 0 é o alto da janela.
#[derive(Debug, Clone, PartialEq)]
pub struct Quadro {
    pub largura: u32,
    pub altura: u32,
    pub rgba: Vec<u8>,
}

impl Quadro {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.largura + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// Bytes B, G, R, A por pixel: a ordem do DIB do Windows e do ARGB de 32 bits do X11 (little-endian).
    pub fn bgra(&self) -> Vec<u8> {
        self.rgba.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect()
    }
}

// ---------- bandeira dos EUA ----------
pub const BANDEIRA_ALTURA: usize = 12;
pub const BANDEIRA_LARGURA: usize = 23; // ~19:10
const BANDEIRA_OPACIDADE: f32 = 0.85;
const ESCALA: usize = 4;

/// RGBA pré-multiplicado (0..1) do tamanho `s` vezes maior, com os valores antes de reduzir.
fn bandeira_grande(s: usize) -> Vec<[f32; 4]> {
    let (h, w) = (BANDEIRA_ALTURA * s, BANDEIRA_LARGURA * s);
    let (hf, wf, sf) = (h as f32, w as f32, s as f32);
    let (ch, cw) = (hf * 4.0 / 7.0, wf * 0.4);
    let r = 2.0 * sf; // cantos arredondados
    let mut img = Vec::with_capacity(h * w);
    for yi in 0..h {
        let y = yi as f32;
        for xi in 0..w {
            let x = xi as f32;
            let listra = (y * 7.0 / hf) as i32; // 7 listras: as 13 reais viram rosa em 12 px
            let mut rgb: Rgb = if listra % 2 == 0 { [0.70, 0.13, 0.20] } else { [1.0, 1.0, 1.0] };
            let canto = y < ch && x < cw;
            if canto {
                rgb = [0.24, 0.23, 0.43];
                // estrelas como pontos: 3 linhas x 4 colunas
                let (ey, ex) = ((y / ch * 3.0) % 1.0, (x / cw * 4.0) % 1.0);
                if (ey - 0.5).powi(2) + (ex - 0.5).powi(2) < 0.06 {
                    rgb = [1.0, 1.0, 1.0];
                }
            }
            let dx = (r - x).max(x - (wf - 1.0 - r)).max(0.0);
            let dy = (r - y).max(y - (hf - 1.0 - r)).max(0.0);
            let a = if dx * dx + dy * dy <= r * r { 1.0 } else { 0.0 };
            let borda = dx * dx + dy * dy > (r - sf).powi(2) || x < sf || x >= wf - sf || y < sf || y >= hf - sf;
            if borda {
                rgb = rgb.map(|c| c * 0.55); // contorno escuro de 1 px: a listra branca não some em fundo claro
            }
            img.push([rgb[0] * a, rgb[1] * a, rgb[2] * a, a]);
        }
    }
    img
}

/// A bandeira 12x23, RGBA pré-multiplicado 0..1, com a opacidade já aplicada. Calculada uma vez.
fn bandeira() -> &'static Vec<[f32; 4]> {
    static B: OnceLock<Vec<[f32; 4]>> = OnceLock::new();
    B.get_or_init(|| {
        let grande = bandeira_grande(ESCALA);
        let w = BANDEIRA_LARGURA * ESCALA;
        let mut saida = Vec::with_capacity(BANDEIRA_ALTURA * BANDEIRA_LARGURA);
        for y in 0..BANDEIRA_ALTURA {
            for x in 0..BANDEIRA_LARGURA {
                let mut m = [0f32; 4];
                for sy in 0..ESCALA {
                    for sx in 0..ESCALA {
                        let p = grande[(y * ESCALA + sy) * w + x * ESCALA + sx];
                        (0..4).for_each(|c| m[c] += p[c]);
                    }
                }
                saida.push(m.map(|v| v / (ESCALA * ESCALA) as f32 * BANDEIRA_OPACIDADE));
            }
        }
        saida
    })
}

/// Pixels entre a borda da tela e a base da bandeira: acima do núcleo da luz, em qualquer espessura.
pub fn distancia_da_borda(k: f32) -> u32 {
    (2.0 * k).round() as u32 + 4
}

// ---------- desenho ----------
/// Gera o quadro. `t` são os segundos desde que o estado começou; `nivel` (0..1) é o nível de áudio já suavizado.
/// `None` quando o estado pronto/erro já apagou por completo (a luz volta a idle).
pub fn desenhar(cfg: &Config, fase: Fase, t: f32, nivel: f32, ingles: bool, largura: u32) -> Option<Quadro> {
    let w = largura.max(2) as usize;
    let h = cfg.altura() as usize;
    let k = cfg.k;
    let meio = (w - 1) as f32 / 2.0;
    let fx: Vec<f32> = (0..w).map(|x| (x as f32 - meio).abs() / meio).collect(); // 0 no centro, 1 nas bordas
    let queda: Vec<f32> = fx.iter().map(|f| (1.0 - f).powf(1.8)).collect(); // forte no centro, apaga nas bordas

    let (mut ganho, mut p) = (1.0f32, 1.0f32);
    let (n, hh): (f32, Vec<f32>) = match fase {
        Fase::Gravando => {
            p = 1.0 - (1.0 - (t / 0.45).min(1.0)).powi(3); // cresce do centro com desaceleração
            (nivel, fx.iter().zip(&queda).map(|(f, q)| ((p - f) / 0.06).clamp(0.0, 1.0) * q).collect())
        }
        Fase::Processando => {
            let onda = t % 1.0; // pulso saindo do centro para as bordas
            (0.25, fx.iter().zip(&queda).map(|(f, q)| q * (0.45 + 0.55 * (-((f - onda).powi(2)) / 0.004).exp())).collect())
        }
        Fase::Pronto | Fase::Erro => {
            ganho = (1.0 - t / 0.5).max(0.0);
            if ganho == 0.0 {
                return None;
            }
            (if fase == Fase::Pronto { 0.3 } else { 0.8 }, queda.clone())
        }
    };
    let pal = cfg.paleta;
    let (nucleo_c, brilho_c, nevoa_c) = if fase == Fase::Erro { (Some(ERRO_NUCLEO), ERRO_BRILHO, ERRO_NEVOA) } else { (None, pal.brilho, pal.nevoa) };
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let d = (if cfg.topo { y } else { h - 1 - y }) as f32; // distância até a borda da tela
        let nucleo = (NUCLEO_PX * k - d).clamp(0.0, 1.0); // núcleo cheio, borda suave
        let brilho = (-d / ((1.5 + n * 6.0) * k)).exp() * (0.45 + 0.45 * n);
        let nevoa = (-d / (14.0 * k)).exp() * 0.25 * n;
        for x in 0..w {
            let (ac, ab, an) = (nucleo * hh[x], brilho * hh[x], nevoa * hh[x]);
            let a = (ac + ab + an).clamp(0.0, 1.0) * ganho;
            let i = (y * w + x) * 4;
            for c in 0..3 {
                let cn = match nucleo_c {
                    Some(e) => e[c],
                    None => pal.ponta[c] + (pal.centro[c] - pal.ponta[c]) * (1.0 - fx[x]),
                };
                let v = (ac * cn + ab * brilho_c[c] + an * nevoa_c[c]) * ganho;
                rgba[i + c] = v.min(a * 255.0) as u8; // pré-multiplicado
            }
            rgba[i + 3] = (a * 255.0) as u8;
        }
    }
    if ingles && fase != Fase::Erro {
        compor_bandeira(&mut rgba, w, h, cfg, ganho * p);
    }
    Some(Quadro { largura: w as u32, altura: h as u32, rgba })
}

/// Por cima da luz, no centro, logo acima do núcleo (abaixo dele com a luz no topo).
fn compor_bandeira(rgba: &mut [u8], w: usize, h: usize, cfg: &Config, alfa: f32) {
    let off = distancia_da_borda(cfg.k) as usize;
    let y0 = if cfg.topo { off } else { (h as i64 - off as i64 - BANDEIRA_ALTURA as i64).max(0) as usize };
    let x0 = w.saturating_sub(BANDEIRA_LARGURA) / 2;
    let b = bandeira();
    for by in 0..BANDEIRA_ALTURA.min(h.saturating_sub(y0)) {
        for bx in 0..BANDEIRA_LARGURA.min(w - x0) {
            let f = b[by * BANDEIRA_LARGURA + bx].map(|v| v * alfa);
            let i = ((y0 + by) * w + x0 + bx) * 4;
            let fundo = 1.0 - f[3];
            for c in 0..3 {
                rgba[i + c] = (f[c] * 255.0 + rgba[i + c] as f32 * fundo).min(255.0) as u8;
            }
            rgba[i + 3] = (f[3] * 255.0 + rgba[i + 3] as f32 * fundo).min(255.0) as u8;
        }
    }
}

// ---------- estado ----------
/// Estado da luz e animação: o que o laço consulta a cada quadro.
pub struct Controle {
    fase: Option<Fase>, // None = idle (oculta)
    t0: Instant,
    nivel: f32,
    ingles: bool,
    ligada: bool,
    pub cfg: Config,
    reposicionar: bool,
}

impl Controle {
    pub fn novo(agora: Instant) -> Self {
        Controle { fase: None, t0: agora, nivel: 0.0, ingles: false, ligada: true, cfg: Config::default(), reposicionar: false }
    }

    pub fn ativa(&self) -> bool {
        self.fase.is_some()
    }

    /// Verdadeiro uma vez por ditado: a janela precisa ser reposicionada (e redimensionada).
    pub fn precisa_reposicionar(&mut self) -> bool {
        std::mem::take(&mut self.reposicionar)
    }

    /// Aplica o estado pedido pelo ditado. `ler_config` só é chamada ao começar a gravar (vale a partir deste ditado).
    pub fn definir(&mut self, e: EstadoLuz, agora: Instant, ler_config: impl FnOnce() -> Config) {
        if let EstadoLuz::Gravando { ingles } = e {
            self.cfg = ler_config();
            self.ingles = ingles;
            self.ligada = self.cfg.ligada;
        }
        if !self.ligada { // luz desligada: não mostra a janela (o bipe e o resto do ditado seguem normais)
            self.fase = None; // e apaga a luz do ditado anterior que ainda estivesse na tela
            return;
        }
        let fase = match e {
            EstadoLuz::Gravando { .. } => {
                self.reposicionar = true;
                self.nivel = 0.0;
                Fase::Gravando
            }
            EstadoLuz::Processando => Fase::Processando,
            EstadoLuz::Pronto => Fase::Pronto,
            EstadoLuz::Erro => Fase::Erro,
        };
        self.fase = Some(fase);
        self.t0 = agora;
    }

    /// Próximo quadro; `alvo` é o último nível de áudio recebido. `None` quando a luz está (ou ficou) oculta.
    pub fn quadro(&mut self, agora: Instant, alvo: f32, largura: u32) -> Option<Quadro> {
        let fase = self.fase?;
        self.nivel += (alvo - self.nivel) * if alvo > self.nivel { 0.5 } else { 0.12 };
        let t = agora.saturating_duration_since(self.t0).as_secs_f32();
        let q = desenhar(&self.cfg, fase, t, self.nivel, self.ingles, largura);
        if q.is_none() {
            self.fase = None;
        }
        q
    }
}

// ---------- thread da luz ----------
/// Onde a luz aparece. Cada sistema implementa; é criada e usada numa thread só (a da luz).
pub trait Superficie {
    /// Posição e largura da janela para este ditado: monitor ativo, encostada na barra (embaixo ou no topo).
    fn posicionar(&mut self, topo: bool, altura: u32) -> Result<Geometria, String>;
    fn mostrar(&mut self, g: Geometria, q: &Quadro) -> Result<(), String>;
    fn esconder(&mut self);
    /// Chamado ~10x por segundo enquanto a luz está na tela: reafirma o "sempre no topo".
    fn reafirmar(&mut self) {}
    /// Chamado a cada quadro: atende as mensagens da janela.
    fn bombear(&mut self) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometria {
    pub x: i32,
    pub y: i32,
    pub largura: u32,
}

/// Alça da luz: pode ser clonada e usada de qualquer thread (inclusive a de captura do microfone).
#[derive(Clone)]
pub struct Luz {
    tx: mpsc::Sender<EstadoLuz>,
    nivel: Arc<AtomicU32>,
}

impl Luz {
    /// Sobe a thread da luz. `criar` roda nela (uma janela do Windows tem de nascer na thread que bombeia as mensagens).
    /// Se a janela não puder ser criada, o motivo vai ao log e a luz fica calada: o ditado segue normal.
    pub fn iniciar<S, F>(dir: PathBuf, criar: F) -> Luz
    where
        S: Superficie + 'static,
        F: FnOnce() -> Result<S, String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let nivel = Arc::new(AtomicU32::new(0));
        let n = nivel.clone();
        std::thread::spawn(move || match criar() {
            Ok(s) => laco(&dir, s, rx, &n),
            Err(e) => {
                pasta::log(&dir, &format!("luz: janela não abriu ({e}); o ditado segue sem a luz"));
                while rx.recv().is_ok() {}
            }
        });
        Luz { tx, nivel }
    }

    pub fn definir(&self, e: EstadoLuz) {
        let _ = self.tx.send(e);
    }

    /// Nível de áudio de 0 a 1 (`audio::nivel_luz`); barato, pode ser chamado pelo callback do microfone.
    pub fn audio(&self, v: f32) {
        self.nivel.store(v.to_bits(), Ordering::Relaxed);
    }
}

fn laco<S: Superficie>(dir: &std::path::Path, mut s: S, rx: mpsc::Receiver<EstadoLuz>, nivel: &AtomicU32) {
    let mut c = Controle::novo(Instant::now());
    let mut geo: Option<Geometria> = None;
    let mut quadros = 0u64;
    let quadro_dur = Duration::from_millis(1000 / FPS);
    let ler = || Config::de_ajustes(&ajustes::ler(dir));
    loop {
        if !c.ativa() {
            s.esconder();
            match rx.recv() {
                Ok(e) => c.definir(e, Instant::now(), ler),
                Err(_) => return,
            }
        }
        while let Ok(e) = rx.try_recv() {
            c.definir(e, Instant::now(), ler);
        }
        if c.precisa_reposicionar() {
            match s.posicionar(c.cfg.topo, c.cfg.altura()) {
                Ok(g) => geo = Some(g),
                Err(e) => {
                    pasta::log(dir, &format!("luz: não achei o monitor ({e})"));
                    geo = None;
                }
            }
            quadros = 0;
        }
        s.bombear();
        if let (Some(g), true) = (geo, c.ativa()) {
            if let Some(q) = c.quadro(Instant::now(), f32::from_bits(nivel.load(Ordering::Relaxed)), g.largura) {
                if let Err(e) = s.mostrar(g, &q) {
                    pasta::log(dir, &format!("luz: não consegui desenhar ({e})"));
                    geo = None;
                }
                quadros += 1;
                if quadros % (FPS / 4) == 0 {
                    s.reafirmar();
                }
            }
        }
        match rx.recv_timeout(quadro_dur) {
            Ok(e) => c.definir(e, Instant::now(), ler),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn cfg(luz: Value) -> Config {
        Config::de_ajustes(&json!({"luz": luz}))
    }
    fn padrao() -> Config {
        cfg(json!({"ligada": true, "cor": "azul", "espessura": "media", "posicao": "embaixo"}))
    }
    const W: u32 = 1001; // ímpar: há uma coluna exatamente no centro

    #[test]
    fn config_le_cor_espessura_e_posicao() {
        let c = cfg(json!({"cor": "verde", "espessura": "grossa", "posicao": "topo", "ligada": false}));
        assert_eq!((c.k, c.topo, c.ligada, c.altura()), (2.0, true, false, 72));
        assert_eq!(c.paleta.ponta, [22.0, 163.0, 74.0]);
        assert_eq!(padrao().altura(), 36);
        assert_eq!(cfg(json!({"espessura": "fina"})).altura(), 18);
        assert_eq!(Config::default(), padrao());
    }

    #[test]
    fn centro_e_ponta_da_linha_gravando() {
        let q = desenhar(&padrao(), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        assert_eq!((q.largura, q.altura), (W, 36));
        // embaixo: a linha 35 é a do núcleo (distância 0). No centro o núcleo está cheio, no azul claro do centro.
        let [r, g, b, a] = q.pixel(500, 35);
        assert_eq!(a, 255);
        assert!((140..=160).contains(&r) && (240..=255).contains(&g) && b == 255, "{r} {g} {b}");
        // na ponta o degradê apagou
        assert_eq!(q.pixel(0, 35), [0, 0, 0, 0]);
        assert_eq!(q.pixel(W - 1, 35)[3], 0);
        // o brilho cai com a distância da borda e some no alto da faixa
        assert!(q.pixel(500, 30)[3] > 0 && q.pixel(500, 30)[3] < q.pixel(500, 34)[3]);
        assert!(q.pixel(500, 0)[3] < 5); // só um fiapo de névoa no alto
        // simétrico
        assert_eq!(q.pixel(300, 35), q.pixel(700, 35));
        // pré-multiplicado: nenhum canal passa do alfa
        assert!(q.rgba.chunks(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
    }

    #[test]
    fn posicao_topo_inverte_a_faixa() {
        let embaixo = desenhar(&padrao(), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        let topo = desenhar(&cfg(json!({"posicao": "topo"})), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        for y in 0..36 {
            assert_eq!(embaixo.pixel(500, y), topo.pixel(500, 35 - y));
        }
        assert_eq!(topo.pixel(500, 0)[3], 255);
        assert!(topo.pixel(500, 35)[3] < 5);
    }

    #[test]
    fn espessura_muda_a_altura_e_o_nucleo() {
        let fina = desenhar(&cfg(json!({"espessura": "fina"})), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        let grossa = desenhar(&cfg(json!({"espessura": "grossa"})), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        assert_eq!((fina.altura, grossa.altura), (18, 72));
        // núcleo de 2*k px: fina 1 linha cheia, grossa 4
        assert_eq!(fina.pixel(500, 17)[3], 255);
        assert_eq!(grossa.pixel(500, 68)[3], 255);
        assert!(grossa.pixel(500, 66)[3] < 255 || grossa.pixel(500, 67)[3] == 255); // o brilho segue o núcleo
    }

    #[test]
    fn cresce_do_centro_ao_gravar() {
        let c = padrao();
        let ini = desenhar(&c, Fase::Gravando, 0.05, 0.0, false, W).unwrap();
        let fim = desenhar(&c, Fase::Gravando, 0.45, 0.0, false, W).unwrap();
        assert!(ini.pixel(500, 35)[3] > 0 && ini.pixel(250, 35)[3] == 0); // só o miolo
        assert!(fim.pixel(250, 35)[3] > 0);
    }

    #[test]
    fn cor_e_erro() {
        let verde = desenhar(&cfg(json!({"cor": "verde"})), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        let [r, g, b, _] = verde.pixel(500, 35);
        assert!(g > r && g > b);
        let erro = desenhar(&padrao(), Fase::Erro, 0.0, 0.0, true, W).unwrap(); // vermelho, sem bandeira
        let [r, g, b, a] = erro.pixel(500, 33); // brilho vermelho logo acima do núcleo
        assert!(a > 0 && r > g && r > b, "{r} {g} {b}");
        assert!(desenhar(&padrao(), Fase::Erro, 0.5, 0.0, false, W).is_none()); // apagou
        assert!(desenhar(&padrao(), Fase::Pronto, 0.6, 0.0, false, W).is_none());
        let meio = desenhar(&padrao(), Fase::Pronto, 0.25, 0.0, false, W).unwrap();
        assert!(meio.pixel(500, 35)[3] > 0 && meio.pixel(500, 35)[3] < 255); // apagando
    }

    #[test]
    fn processando_tem_pulso() {
        let a = desenhar(&padrao(), Fase::Processando, 0.0, 0.0, false, W).unwrap();
        let b = desenhar(&padrao(), Fase::Processando, 0.3, 0.0, false, W).unwrap();
        assert_ne!(a.pixel(650, 35), b.pixel(650, 35)); // o pulso andou
    }

    #[test]
    fn bandeira_tem_sete_listras() {
        let s = ESCALA;
        let g = bandeira_grande(s);
        let w = BANDEIRA_LARGURA * s;
        // coluna longe do canto azul, sem a borda: troca vermelho (g<0,5) e branco (g>=0,5) 6 vezes = 7 listras
        let x = 20 * s;
        let cores: Vec<bool> = (s..BANDEIRA_ALTURA * s - s).map(|y| g[y * w + x][1] > 0.5).collect();
        let listras = 1 + cores.windows(2).filter(|p| p[0] != p[1]).count();
        assert_eq!(listras, 7);
        // canto azul com estrelas brancas, e cantos arredondados transparentes
        assert_eq!(g[(2 * s) * w + 2 * s].map(|v| (v * 100.0).round() as i32)[3], 100);
        assert_eq!(g[0][3], 0.0);
    }

    #[test]
    fn bandeira_fica_acima_do_nucleo_e_abaixo_no_topo() {
        let eua = desenhar(&padrao(), Fase::Gravando, 1.0, 0.6, true, W).unwrap();
        let sem = desenhar(&padrao(), Fase::Gravando, 1.0, 0.6, false, W).unwrap();
        // embaixo: y0 = 36 - 6 - 12 = 18; listra vermelha da direita (fora do canto azul), linha 18 + 4
        let [r, g, _, a] = eua.pixel(500 - 11 + 18, 22);
        assert!(r > 140 && g < 50 && a > 200, "{r} {g} {a}");
        assert!(sem.pixel(500 - 11 + 18, 22)[3] < 40);
        // fora da bandeira nada muda; o núcleo continua o mesmo
        assert_eq!(eua.pixel(500, 35), sem.pixel(500, 35));
        assert_eq!(eua.pixel(100, 25), sem.pixel(100, 25));
        // topo: y0 = 6, a bandeira fica abaixo do núcleo
        let topo = desenhar(&cfg(json!({"posicao": "topo"})), Fase::Gravando, 1.0, 0.6, true, W).unwrap();
        let [r, g, _, a] = topo.pixel(500 - 11 + 18, 10);
        assert!(r > 140 && g < 50 && a > 200, "{r} {g} {a}");
    }

    #[test]
    fn controle_respeita_luz_desligada_e_apaga() {
        let t0 = Instant::now();
        let mut c = Controle::novo(t0);
        let desligada = || cfg(json!({"ligada": false}));
        c.definir(EstadoLuz::Gravando { ingles: false }, t0, desligada);
        assert!(!c.ativa() && !c.precisa_reposicionar());
        c.definir(EstadoLuz::Processando, t0, desligada); // sem gravar de novo, continua desligada
        assert!(!c.ativa());
        assert!(c.quadro(t0, 0.5, W).is_none());

        c.definir(EstadoLuz::Gravando { ingles: false }, t0, padrao);
        assert!(c.ativa() && c.precisa_reposicionar() && !c.precisa_reposicionar());
        assert!(c.quadro(t0 + Duration::from_millis(500), 1.0, W).is_some());
        c.definir(EstadoLuz::Pronto, t0, padrao);
        assert!(c.quadro(t0 + Duration::from_millis(200), 0.0, W).is_some());
        assert!(c.quadro(t0 + Duration::from_millis(600), 0.0, W).is_none());
        assert!(!c.ativa()); // apagou sozinha

        c.definir(EstadoLuz::Gravando { ingles: false }, t0, padrao);
        c.definir(EstadoLuz::Gravando { ingles: false }, t0, desligada); // desligar no ajuste apaga a que estava na tela
        assert!(!c.ativa());
    }

    #[test]
    fn nivel_sobe_rapido_e_desce_devagar() {
        let t0 = Instant::now();
        let mut c = Controle::novo(t0);
        c.definir(EstadoLuz::Gravando { ingles: false }, t0, padrao);
        c.quadro(t0, 1.0, W);
        assert!((c.nivel - 0.5).abs() < 1e-6);
        c.quadro(t0, 0.0, W);
        assert!((c.nivel - 0.44).abs() < 1e-6);
    }

    struct Falsa(Arc<Mutex<Vec<String>>>);
    impl Superficie for Falsa {
        fn posicionar(&mut self, topo: bool, altura: u32) -> Result<Geometria, String> {
            self.0.lock().unwrap().push(format!("pos topo={topo} h={altura}"));
            Ok(Geometria { x: 0, y: 700, largura: 800 })
        }
        fn mostrar(&mut self, g: Geometria, q: &Quadro) -> Result<(), String> {
            assert_eq!((g.largura, q.altura), (800, 36));
            self.0.lock().unwrap().push("quadro".into());
            Ok(())
        }
        fn esconder(&mut self) {
            let mut v = self.0.lock().unwrap();
            if v.last().map(String::as_str) != Some("esconde") {
                v.push("esconde".into());
            }
        }
    }

    #[test]
    fn a_thread_mostra_e_apaga() {
        let d = tempfile::tempdir().unwrap();
        let eventos = Arc::new(Mutex::new(vec![]));
        let e2 = eventos.clone();
        let luz = Luz::iniciar(d.path().to_path_buf(), move || Ok(Falsa(e2)));
        luz.definir(EstadoLuz::Gravando { ingles: false });
        luz.audio(0.5);
        std::thread::sleep(Duration::from_millis(200));
        luz.definir(EstadoLuz::Pronto);
        std::thread::sleep(Duration::from_millis(900));
        let v = eventos.lock().unwrap().clone();
        assert_eq!(v[0], "esconde");
        assert_eq!(v[1], "pos topo=false h=36");
        assert!(v.iter().filter(|e| *e == "quadro").count() >= 8, "{v:?}"); // ~40 fps
        assert_eq!(v.last().unwrap(), "esconde");
    }

    #[test]
    fn janela_que_nao_abre_vai_ao_log_e_nao_trava() {
        let d = tempfile::tempdir().unwrap();
        let luz = Luz::iniciar(d.path().to_path_buf(), || Err::<Falsa, _>("sem X".to_string()));
        luz.definir(EstadoLuz::Gravando { ingles: true });
        luz.audio(1.0);
        std::thread::sleep(Duration::from_millis(100));
        let log = std::fs::read_to_string(d.path().join(pasta::ARQ_LOG)).unwrap();
        assert!(log.contains("luz: janela não abriu (sem X)"), "{log}");
    }
}
