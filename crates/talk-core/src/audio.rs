//! Áudio do ditado sem depender de sistema: WAV 16 kHz mono, conversão da captura, pico e bipe.

pub const TAXA: u32 = 16000;
pub const SILENCIO: i32 = 800; // pico abaixo disso (de 32767) é tratado como silêncio: o Whisper inventa texto

/// Média dos canais e reamostragem para 16 kHz (média por janela: segura o serrilhado ao diminuir a taxa).
/// `amostras` é intercalado, no formato de um dispositivo de entrada (f32 de -1 a 1).
pub fn para_16k_mono(amostras: &[f32], taxa: u32, canais: usize) -> Vec<i16> {
    let canais = canais.max(1);
    let mono: Vec<f32> = amostras.chunks(canais).map(|q| q.iter().sum::<f32>() / q.len() as f32).collect();
    let razao = taxa as f64 / TAXA as f64;
    let saida = (mono.len() as f64 / razao) as usize;
    (0..saida)
        .map(|i| {
            let (a, b) = ((i as f64 * razao) as usize, (((i + 1) as f64 * razao) as usize).min(mono.len()));
            let janela = &mono[a..b.max(a + 1).min(mono.len())];
            let m = janela.iter().sum::<f32>() / janela.len() as f32;
            (m.clamp(-1.0, 1.0) * 32767.0).round() as i16
        })
        .collect()
}

/// Nível de 0 a 1 para a luz: RMS da mistura mono, com 2500 (de 32767) como fundo de escala (igual ao `ao_audio` do Python).
pub fn nivel_luz(amostras: &[f32], canais: usize) -> f32 {
    let canais = canais.max(1);
    let (soma, n) = amostras.chunks(canais).fold((0f64, 0usize), |(s, n), q| {
        let m = q.iter().sum::<f32>() as f64 / q.len() as f64;
        (s + m * m, n + 1)
    });
    if n == 0 { 0.0 } else { (((soma / n as f64).sqrt() * 32767.0 / 2500.0) as f32).min(1.0) }
}

/// WAV PCM de 16 bits, 16 kHz, mono (o que a Groq recebe).
pub fn wav(amostras: &[i16]) -> Vec<u8> {
    let dados = (amostras.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + dados as usize);
    v.extend(b"RIFF");
    v.extend((36 + dados).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(16u32.to_le_bytes());
    v.extend(1u16.to_le_bytes()); // PCM
    v.extend(1u16.to_le_bytes()); // mono
    v.extend(TAXA.to_le_bytes());
    v.extend((TAXA * 2).to_le_bytes());
    v.extend(2u16.to_le_bytes());
    v.extend(16u16.to_le_bytes());
    v.extend(b"data");
    v.extend(dados.to_le_bytes());
    v.extend(amostras.iter().flat_map(|s| s.to_le_bytes()));
    v
}

/// Lê um WAV PCM de 16 bits (qualquer taxa e canais) e devolve 16 kHz mono. Usado só nos testes por arquivo.
pub fn ler_wav(bytes: &[u8]) -> Result<Vec<i16>, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("WAV inválido".into());
    }
    let (mut taxa, mut canais, mut bits) = (0u32, 0usize, 0u16);
    let mut p = 12;
    while p + 8 <= bytes.len() {
        let tam = u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
        let corpo = &bytes[p + 8..(p + 8 + tam).min(bytes.len())];
        match &bytes[p..p + 4] {
            b"fmt " if corpo.len() >= 16 => {
                canais = u16::from_le_bytes([corpo[2], corpo[3]]) as usize;
                taxa = u32::from_le_bytes(corpo[4..8].try_into().unwrap());
                bits = u16::from_le_bytes([corpo[14], corpo[15]]);
            }
            b"data" => {
                if bits != 16 || taxa == 0 || canais == 0 {
                    return Err("só WAV PCM de 16 bits".into());
                }
                if taxa == TAXA && canais == 1 {
                    return Ok(corpo.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()); // já no formato
                }
                let f: Vec<f32> = corpo.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
                return Ok(para_16k_mono(&f, taxa, canais));
            }
            _ => {}
        }
        p += 8 + tam + (tam & 1);
    }
    Err("WAV sem dados".into())
}

pub fn pico(amostras: &[i16]) -> i32 {
    amostras.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0)
}

/// Seno com envelope de 8 ms; o pico é volume/100 do máximo (volume 30 -> 0,3). Mesma onda do Python.
/// `taxa` é a do dispositivo de saída (o Python usa `TAXA_BIPE`).
pub fn onda_bipe(freq: u32, ms: u32, volume: u8, taxa: u32) -> Vec<f32> {
    let n = (taxa as u64 * ms as u64 / 1000) as usize;
    let amp = volume.min(100) as f32 / 100.0;
    (0..n)
        .map(|i| {
            let t = i as f32 / taxa as f32;
            let fim = (n - 1 - i) as f32 / taxa as f32;
            let env = (t.min(fim) / 0.008).min(1.0);
            amp * (2.0 * std::f32::consts::PI * freq as f32 * t).sin() * env
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nivel_da_luz_segue_a_voz_e_satura() {
        assert_eq!(nivel_luz(&[], 1), 0.0);
        assert_eq!(nivel_luz(&[0.0; 100], 2), 0.0);
        let meio = nivel_luz(&[0.04; 100], 1); // 0,04 * 32767 = 1310 -> 0,52
        assert!((meio - 0.524).abs() < 0.01, "{meio}");
        assert_eq!(nivel_luz(&[0.5; 100], 1), 1.0);
        assert!((nivel_luz(&[0.04, 0.04].repeat(50), 2) - meio).abs() < 1e-6); // estéreo igual ao mono
    }

    #[test]
    fn wav_tem_cabecalho_e_tamanho() {
        let w = wav(&[1, -2, 3]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(ler_wav(&w).unwrap(), vec![1, -2, 3]);
    }

    #[test]
    fn reamostra_48k_estereo_para_16k_mono() {
        // 0,5 s a 48 kHz, 2 canais, valor constante 0,5: sai 8000 amostras de ~16383
        let f = vec![0.5f32; 24000 * 2];
        let s = para_16k_mono(&f, 48000, 2);
        assert_eq!(s.len(), 8000);
        assert!(s.iter().all(|v| (*v - 16383).abs() <= 1));
    }

    #[test]
    fn pico_e_silencio() {
        assert_eq!(pico(&[10, -900, 5]), 900);
        assert!(pico(&[10, -700]) < SILENCIO);
        assert_eq!(pico(&[]), 0);
    }

    #[test]
    fn bipe_respeita_volume_e_comeca_em_zero() {
        let o = onda_bipe(880, 60, 30, 44100);
        assert_eq!(o.len(), 2646);
        assert!(o[0].abs() < 1e-6);
        let max = o.iter().cloned().fold(0.0f32, f32::max);
        assert!(max <= 0.3 && max > 0.25);
        assert!(onda_bipe(880, 60, 0, 44100).iter().all(|v| *v == 0.0));
    }
}
