//! Windows, porta de `app/plataforma_windows.py`:
//! - atalhos pelo gancho de teclado de baixo nível (`WH_KEYBOARD_LL`), casados por `talk_core::casador::Casador`;
//! - colar por `SendInput` (Ctrl+V) guardando e devolvendo todos os formatos da área de transferência, fora do Win+V;
//! - volume dos outros apps por sessão de áudio (WASAPI), microfone e bipe pelo `cpal` (`som.rs`);
//! - app ativo pelo nome do executável da janela em primeiro plano; instância única por mutex.

use super::som::{self, Captura};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use talk_core::casador::{Acao, Casador};
use talk_core::ditado::{Ditado, Plataforma};
use talk_core::{ajustes, pasta};
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, GlobalFree, HANDLE, HGLOBAL, HINSTANCE, ERROR_ALREADY_EXISTS, LPARAM, LRESULT, WPARAM};
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

/// Mutex da instância única. Não pode ser "RenanTalk": a versão em Python usa esse nome e a ponte (tarefa 10)
/// precisa das duas rodando juntas por instantes, até o Python sair.
const NOME_MUTEX: &str = "RenanTalk-Rust";
/// Marca (dwExtraInfo) das teclas que nós mesmos enviamos: o gancho as ignora.
const MARCA: usize = 0x5254_4B52;
/// Tecla sem função (VK 0xE8): tocada ao parar, para o Windows não abrir o menu Iniciar ao soltar o Win.
const VK_SEM_FUNCAO: u16 = 0xE8;
const CF_UNICODETEXT: u32 = 13;
// CF_BITMAP, METAFILEPICT, PALETTE, ENHMETAFILE e os de exibição guardam handles GDI, não memória: o Windows
// refaz o CF_BITMAP a partir do CF_DIB, que é copiado.
const NAO_MEMORIA: [u32; 8] = [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];
// Marcas que tiram o conteúdo do histórico (Win+V) e da sincronização na nuvem.
const SEM_HISTORICO: [&str; 3] = ["ExcludeClipboardContentFromMonitorProcessing", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"];

type Formatos = Vec<(u32, Vec<u8>)>;

// ---------- teclas enviadas ----------
fn tecla(vk: u16, subir: bool, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk), wScan: 0, time: 0, dwExtraInfo: extra,
                dwFlags: if subir { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
            },
        },
    }
}

fn enviar(teclas: &[INPUT]) -> Result<(), String> {
    let n = unsafe { SendInput(teclas, std::mem::size_of::<INPUT>() as i32) };
    if n as usize == teclas.len() { Ok(()) } else { Err(format!("SendInput enviou {n} de {} teclas", teclas.len())) }
}

fn mascarar_win() {
    let _ = enviar(&[tecla(VK_SEM_FUNCAO, false, MARCA), tecla(VK_SEM_FUNCAO, true, MARCA)]);
}

// ---------- gancho de teclado ----------
struct Evento {
    vk: u32,
    scan: u32,
    desceu: bool,
}

thread_local! {
    // O gancho roda na thread que o instalou: a saída fica nela, sem global.
    static SAIDA: RefCell<Option<mpsc::Sender<Evento>>> = const { RefCell::new(None) };
}

/// Só repassa a tecla por um canal: o Windows desliga o gancho que demora, e `comecar` abre o microfone.
unsafe extern "system" fn gancho(codigo: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if codigo == 0 { // HC_ACTION
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let m = wparam.0 as u32;
        let (desceu, subiu) = (m == WM_KEYDOWN || m == WM_SYSKEYDOWN, m == WM_KEYUP || m == WM_SYSKEYUP);
        if (desceu || subiu) && k.dwExtraInfo != MARCA {
            SAIDA.with(|s| {
                if let Some(tx) = &*s.borrow() {
                    let _ = tx.send(Evento { vk: k.vkCode, scan: k.scanCode, desceu });
                }
            });
        }
    }
    CallNextHookEx(None, codigo, wparam, lparam)
}

/// Instala o gancho numa thread com loop de mensagens (sem ele o Windows não entrega as teclas).
/// Devolve o id da thread: `WM_QUIT` para ela desfaz o gancho.
fn instalar_gancho(tx: mpsc::Sender<Evento>) -> Result<u32, String> {
    let (pronto_tx, pronto_rx) = mpsc::channel::<Result<u32, String>>();
    std::thread::spawn(move || unsafe {
        SAIDA.with(|s| *s.borrow_mut() = Some(tx));
        let modulo = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).ok();
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(gancho), modulo, 0) {
            Ok(h) => h,
            Err(e) => return drop(pronto_tx.send(Err(e.to_string()))),
        };
        let _ = pronto_tx.send(Ok(GetCurrentThreadId()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    });
    pronto_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "o gancho não respondeu".to_string())?
}

/// Nome que o `Casador` entende ("ctrl", "alt", "shift", "win", a tecla do atalho) ou "outra".
fn nome_da_tecla(vk: u32, scan: u32) -> String {
    match vk {
        0x10 | 0xA0 | 0xA1 => "shift".into(),
        0x11 | 0xA2 | 0xA3 => "ctrl".into(),
        0x12 | 0xA4 | 0xA5 => "alt".into(), // o AltGr chega como Ctrl+Alt direito: vale como Alt
        0x5B | 0x5C => "win".into(),
        // Shift+1 não muda o VK, mas o AZERTY muda: a fileira de números (scan 2 a 11) vira o dígito, como no Python
        _ if (2..=11).contains(&scan) => "1234567890".chars().nth(scan as usize - 2).unwrap().to_string(),
        0x20 => "space".into(),
        0x41..=0x5A => char::from_u32(vk).unwrap().to_ascii_lowercase().to_string(),
        0x70..=0x7B => format!("f{}", vk - 0x6F),
        _ => "outra".into(),
    }
}

/// O modificador ainda está apertado? (`GetAsyncKeyState`: bit mais alto.)
fn apertada(modificador: &str) -> bool {
    let vks: &[i32] = match modificador {
        "ctrl" => &[0xA2, 0xA3],
        "alt" => &[0xA4, 0xA5],
        "shift" => &[0xA0, 0xA1],
        "win" => &[0x5B, 0x5C],
        _ => return false,
    };
    vks.iter().any(|vk| unsafe { GetAsyncKeyState(*vk) } < 0)
}

/// Escuta os atalhos (ditar, traduzir e colar o último). Os ajustes são relidos a cada tecla: valem sem reiniciar.
pub fn iniciar_gancho(ditado: Arc<Ditado>, dir: PathBuf) {
    let (tx, rx) = mpsc::channel();
    if let Err(e) = instalar_gancho(tx) {
        return pasta::log(&dir, &format!("gancho do teclado falhou: {e}"));
    }
    std::thread::spawn(move || escutar(rx, &ditado, &dir));
}

fn escutar(rx: mpsc::Receiver<Evento>, ditado: &Arc<Ditado>, dir: &Path) {
    let mut cache = ajustes::Cache::default();
    let casador = Arc::new(Mutex::new(Casador::default()));
    for ev in rx {
        let nome = nome_da_tecla(ev.vk, ev.scan);
        let a = cache.ler(dir);
        let [ditar, colar, traduzir] = ["atalho_ditar", "atalho_colar", "atalho_traduzir"].map(|k| ajustes::validar_atalho(a[k].as_str().unwrap_or("")));
        let mut c = casador.lock().unwrap();
        if ev.desceu && !ditado.gravando() { // solta teclas "presas" (Win+L e outros não entregam o key-up)
            c.apertadas.retain(|m| !ajustes::MODIFICADORES.contains(&m.as_str()) || apertada(m));
        }
        let acao = c.evento(&nome, ev.desceu, ditar.as_deref().unwrap_or(&[]), ditado.gravando(), colar.as_deref(), traduzir.as_deref());
        drop(c);
        match acao {
            Some(Acao::Colar) => colar_ao_soltar(casador.clone(), ditado.clone()),
            Some(a @ (Acao::Iniciar | Acao::Traduzir)) => ditado.comecar(a == Acao::Traduzir),
            Some(Acao::Parar) => {
                ditado.parar();
                mascarar_win();
            }
            Some(Acao::Outra) => ditado.marcar_outra_tecla(&format!("vk={:#x} scan={}", ev.vk, ev.scan)), // Ctrl+Win+outra tecla = atalho do Windows
            None => {}
        }
    }
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

// ---------- área de transferência ----------
/// Área de transferência aberta; fecha ao sair do escopo.
struct Area;

impl Area {
    fn abrir() -> Option<Area> {
        for _ in 0..50 { // outro app pode estar com ela aberta
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Area);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// Todos os formatos que guardam memória, com os bytes.
    fn ler(&self) -> Formatos {
        let (mut itens, mut f) = (vec![], 0);
        loop {
            f = unsafe { EnumClipboardFormats(f) };
            if f == 0 {
                return itens;
            }
            if NAO_MEMORIA.contains(&f) {
                continue;
            }
            let Ok(h) = (unsafe { GetClipboardData(f) }) else { continue };
            let g = HGLOBAL(h.0);
            let p = unsafe { GlobalLock(g) };
            if p.is_null() {
                continue;
            }
            itens.push((f, unsafe { std::slice::from_raw_parts(p as *const u8, GlobalSize(g)) }.to_vec()));
            let _ = unsafe { GlobalUnlock(g) };
        }
    }

    /// Esvazia e grava os formatos (o Windows passa a ser o dono da memória).
    fn gravar(&self, itens: &[(u32, Vec<u8>)]) {
        let _ = unsafe { EmptyClipboard() };
        for (f, dados) in itens {
            unsafe {
                let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, dados.len().max(1)) else { continue };
                let p = GlobalLock(g);
                if p.is_null() {
                    let _ = GlobalFree(Some(g));
                    continue;
                }
                std::ptr::copy_nonoverlapping(dados.as_ptr(), p as *mut u8, dados.len());
                let _ = GlobalUnlock(g);
                if SetClipboardData(*f, Some(HANDLE(g.0))).is_err() {
                    let _ = GlobalFree(Some(g));
                }
            }
        }
    }
}

impl Drop for Area {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn sem_historico() -> Formatos {
    SEM_HISTORICO.iter().map(|n| (unsafe { RegisterClipboardFormatW(&HSTRING::from(*n)) }, vec![0u8; 4])).collect()
}

fn utf16(texto: &str) -> Vec<u8> {
    texto.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

// ---------- janela ativa ----------
/// "C:\\...\\Code.exe" -> "Code": só o nome, sem a pasta e sem o ".exe" (os `modos_apps` comparam em minúsculas).
fn nome_do_exe(caminho: &str) -> String {
    let nome = caminho.replace('/', "\\").rsplit('\\').next().unwrap_or("").to_string();
    match nome.len().checked_sub(4).filter(|_| nome.to_lowercase().ends_with(".exe")) {
        Some(n) => nome[..n].to_string(),
        None => nome,
    }
}

fn exe_do_processo(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(nome_do_exe(&String::from_utf16_lossy(&buf[..n as usize])))
    }
}

// ---------- instância única ----------
static MUTEX: AtomicIsize = AtomicIsize::new(0);

/// Cria o mutex `nome`; `None` se já existe (outra instância) ou não foi possível criar.
fn criar_mutex(nome: &str) -> Option<HANDLE> {
    unsafe {
        let h = CreateMutexW(None, false, &HSTRING::from(nome)).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            return None;
        }
        Some(h)
    }
}

/// `true` se esta é a única instância. O mutex fica aberto até o processo acabar.
pub fn instancia_unica() -> bool {
    match criar_mutex(NOME_MUTEX) {
        Some(h) => {
            MUTEX.store(h.0 as isize, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

// ---------- volume por sessão de áudio ----------
thread_local! {
    // A fila de volume do ditado tem uma thread só: o COM é iniciado nela uma vez.
    static COM: () = { let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }; };
}

struct Sessao {
    id: String,
    volume: ISimpleAudioVolume,
    expirada: bool, // o app fechou
}

/// Sessões de áudio do dispositivo de saída padrão, sem os sons do Windows (pid 0) e sem o nosso processo (os bipes).
fn sessoes(log: &dyn Fn(&str)) -> Result<Vec<Sessao>, String> {
    COM.with(|_| ());
    let e = |e: windows::core::Error| e.to_string();
    let mut lista = vec![];
    unsafe {
        let enumerador: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(e)?;
        let dispositivo = enumerador.GetDefaultAudioEndpoint(eRender, eMultimedia).map_err(e)?;
        let gerente: IAudioSessionManager2 = dispositivo.Activate(CLSCTX_ALL, None).map_err(e)?;
        let todas = gerente.GetSessionEnumerator().map_err(e)?;
        for i in 0..todas.GetCount().map_err(e)? { // uma sessão com erro não impede as outras
            let s = || -> windows::core::Result<Option<Sessao>> {
                let c = todas.GetSession(i)?;
                let c2: IAudioSessionControl2 = c.cast()?;
                let pid = c2.GetProcessId()?;
                if pid == 0 || pid == std::process::id() {
                    return Ok(None);
                }
                let p = c2.GetSessionIdentifier()?;
                let id = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                Ok(Some(Sessao { id, volume: c.cast()?, expirada: c.GetState()? == AudioSessionStateExpired }))
            };
            match s() {
                Ok(Some(s)) => lista.push(s),
                Ok(None) => {}
                Err(err) => log(&format!("sessão de áudio: {err}")),
            }
        }
    }
    Ok(lista)
}

// ---------- plataforma ----------
pub struct Windows {
    dir: PathBuf,
    captura: Mutex<Option<Captura>>,
    abaixados: Mutex<Vec<(String, f32)>>,
    // O Windows guarda o volume por app: um app que fechou ainda baixo voltaria baixo. Fica aqui até reaparecer.
    pendentes: Mutex<HashMap<String, f32>>,
}

pub fn nova(dir: &Path) -> Arc<dyn Plataforma> {
    Arc::new(Windows { dir: dir.to_path_buf(), captura: Mutex::new(None), abaixados: Mutex::default(), pendentes: Mutex::default() })
}

impl Windows {
    fn log(&self, msg: &str) {
        pasta::log(&self.dir, msg);
    }
}

impl Plataforma for Windows {
    fn colar(&self, texto: &str) -> Result<(), String> {
        let area = Area::abrir().ok_or("área de transferência ocupada por outro app")?;
        let anterior = area.ler();
        area.gravar(&[vec![(CF_UNICODETEXT, utf16(texto))], sem_historico()].concat());
        drop(area);
        std::thread::sleep(Duration::from_millis(50));
        let r = enviar(&[tecla(0x11, false, MARCA), tecla(0x56, false, MARCA), tecla(0x56, true, MARCA), tecla(0x11, true, MARCA)]); // Ctrl+V
        std::thread::sleep(Duration::from_millis(300));
        if let Some(area) = Area::abrir() { // devolvido fora do histórico também, senão o item anterior aparece duplicado no Win+V
            area.gravar(&if anterior.is_empty() { vec![] } else { [anterior, sem_historico()].concat() });
        }
        r
    }

    fn app_ativo(&self) -> String {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        exe_do_processo(pid).unwrap_or_default()
    }

    /// Põe cada app com som em `fator` do volume atual. Chamada sempre da thread da fila de volume (COM).
    fn abaixar_outros(&self, fator: f32) {
        let sessoes = match sessoes(&|m| self.log(m)) {
            Ok(s) => s,
            Err(e) => return self.log(&format!("abaixar volume: {e}")),
        };
        let mut feitos = vec![];
        for s in sessoes {
            let r = || -> windows::core::Result<f32> {
                let antes = match self.pendentes.lock().unwrap().get(&s.id) {
                    Some(p) => *p,
                    None => unsafe { s.volume.GetMasterVolume()? },
                };
                unsafe { s.volume.SetMasterVolume(antes * fator, std::ptr::null())? };
                Ok(antes)
            };
            match r() {
                Ok(antes) => {
                    self.pendentes.lock().unwrap().remove(&s.id); // só some depois de abaixar
                    feitos.push((s.id, antes));
                }
                Err(e) => self.log(&format!("abaixar volume: {e}")),
            }
        }
        *self.abaixados.lock().unwrap() = feitos;
    }

    /// Volta sempre o volume de antes, pelas sessões de agora: o app que fechou e abriu de novo tem sessão nova.
    /// O que não está aberto fica pendente para o próximo ditado.
    fn restaurar_outros(&self) {
        let mut falta: HashMap<String, f32> = std::mem::take(&mut *self.abaixados.lock().unwrap()).into_iter().collect();
        if falta.is_empty() {
            return;
        }
        let sessoes = sessoes(&|m| self.log(m)).unwrap_or_else(|e| {
            self.log(&format!("restaurar volume: {e}"));
            vec![]
        });
        for s in sessoes.iter().filter(|s| !s.expirada) {
            let Some(antes) = falta.get(&s.id).copied() else { continue };
            match unsafe { s.volume.SetMasterVolume(antes, std::ptr::null()) } {
                Ok(()) => { falta.remove(&s.id); }
                Err(e) => self.log(&format!("restaurar volume: {e}")),
            }
        }
        self.pendentes.lock().unwrap().extend(falta);
    }

    fn bipe(&self, freq: u32, ms: u32, volume: u8) {
        if volume == 0 {
            return;
        }
        std::thread::spawn(move || som::tocar(freq, ms, volume)); // sem saída de áudio não é motivo para travar o ditado
    }

    fn iniciar_captura(&self, microfone: &str) -> Result<(), String> {
        *self.captura.lock().unwrap() = Some(som::iniciar(&self.dir, microfone)?);
        Ok(())
    }

    fn parar_captura(&self) -> Vec<i16> {
        self.captura.lock().unwrap().take().map(som::parar).unwrap_or_default()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Clipboard, teclado e gancho são do sistema inteiro: um teste de cada vez.
    static SISTEMA: Mutex<()> = Mutex::new(());

    #[test]
    fn nomes_das_teclas() {
        assert_eq!(nome_da_tecla(0xA2, 0x1D), "ctrl");
        assert_eq!(nome_da_tecla(0xA3, 0x1D), "ctrl");
        assert_eq!(nome_da_tecla(0xA5, 0x38), "alt");
        assert_eq!(nome_da_tecla(0xA0, 0x2A), "shift");
        assert_eq!(nome_da_tecla(0x5B, 0x5B), "win");
        assert_eq!(nome_da_tecla(0x56, 0x2F), "v");
        assert_eq!(nome_da_tecla(0x20, 0x39), "space");
        assert_eq!(nome_da_tecla(0x74, 0x3F), "f5");
        assert_eq!(nome_da_tecla(0x31, 0x02), "1"); // fileira de números
        assert_eq!(nome_da_tecla(0x30, 0x0B), "0");
        assert_eq!(nome_da_tecla(0x61, 0x4F), "outra"); // teclado numérico não vira dígito
        assert_eq!(nome_da_tecla(0xE8, 0), "outra");
    }

    #[test]
    fn nome_do_executavel() {
        assert_eq!(nome_do_exe(r"C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe"), "Code");
        assert_eq!(nome_do_exe("OUTLOOK.EXE"), "OUTLOOK");
        assert_eq!(nome_do_exe("C:/pasta/app"), "app");
        assert_eq!(nome_do_exe(".exe"), "");
    }

    #[test]
    fn exe_do_proprio_processo() {
        let nome = exe_do_processo(std::process::id()).expect("o próprio processo abre");
        assert!(nome.starts_with("renantalk"), "{nome}");
        assert!(!nome.to_lowercase().ends_with(".exe"));
    }

    #[test]
    fn mutex_so_deixa_uma_instancia() {
        let nome = format!("RenanTalk-Rust-teste-{}", std::process::id()); // o real é de quem estiver rodando
        let primeira = criar_mutex(&nome).expect("a primeira cria");
        assert!(criar_mutex(&nome).is_none(), "a segunda deve ser recusada");
        unsafe { CloseHandle(primeira).unwrap() };
        let nova = criar_mutex(&nome).expect("fechado o dono, outra instância pode abrir");
        unsafe { CloseHandle(nova).unwrap() };
        assert_ne!(NOME_MUTEX, "RenanTalk"); // coexiste com o Python
    }

    fn formato_proprio() -> u32 {
        unsafe { RegisterClipboardFormatW(&HSTRING::from("RenanTalkTeste")) }
    }

    #[test]
    fn colar_devolve_texto_e_formato_extra() {
        let _g = SISTEMA.lock().unwrap_or_else(|e| e.into_inner());
        let (original, extra) = (utf16("original"), b"dados extras \x00\x01\xff".to_vec());
        Area::abrir().expect("área de transferência").gravar(&[(CF_UNICODETEXT, original.clone()), (formato_proprio(), extra.clone())]);

        let p = nova(&std::env::temp_dir());
        let r = p.colar("ditado");
        println!("colar: {r:?}");

        let depois = Area::abrir().expect("área de transferência").ler();
        let acha = |f: u32| depois.iter().find(|(x, _)| *x == f).map(|(_, d)| d.clone());
        assert_eq!(acha(CF_UNICODETEXT), Some(original), "o texto de antes volta, não o ditado");
        assert_eq!(acha(formato_proprio()), Some(extra), "o formato extra volta byte a byte");
        for (f, _) in sem_historico() {
            assert!(acha(f).is_some(), "devolvido fora do histórico do Win+V");
        }
        r.expect("Ctrl+V enviado");
    }

    #[test]
    fn colar_com_a_area_vazia_devolve_vazia() {
        let _g = SISTEMA.lock().unwrap_or_else(|e| e.into_inner());
        Area::abrir().expect("área de transferência").gravar(&[]);
        let _ = nova(&std::env::temp_dir()).colar("ditado");
        assert!(Area::abrir().expect("área de transferência").ler().is_empty(), "o ditado não pode ficar nela");
    }

    #[test]
    fn gancho_ve_ctrl_win_sintetico() {
        let _g = SISTEMA.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::channel();
        let thread = instalar_gancho(tx).expect("gancho instalado");

        let (ctrl, win) = (0xA2, 0x5B); // VK_LCONTROL, VK_LWIN, sem a marca: parecem teclas de verdade
        enviar(&[tecla(ctrl, false, 0), tecla(win, false, 0)]).expect("SendInput");
        mascarar_win(); // com a marca: o gancho não pode ver
        enviar(&[tecla(win, true, 0), tecla(ctrl, true, 0)]).expect("SendInput");

        let mut vistos = vec![];
        while vistos.iter().filter(|e: &&Evento| !e.desceu).count() < 2 {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(e) => vistos.push(e),
                Err(_) => break,
            }
        }
        unsafe { windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(thread, windows::Win32::UI::WindowsAndMessaging::WM_QUIT, WPARAM(0), LPARAM(0)).unwrap() };

        let nomes: Vec<(String, bool)> = vistos.iter().map(|e| (nome_da_tecla(e.vk, e.scan), e.desceu)).collect();
        assert_eq!(nomes, [("ctrl".into(), true), ("win".into(), true), ("win".into(), false), ("ctrl".into(), false)], "{nomes:?}");
        // e o Casador entende como o atalho de ditar
        let alvo: Vec<String> = vec!["ctrl".into(), "win".into()];
        let mut c = Casador::default();
        let acoes: Vec<_> = nomes.iter().map(|(n, d)| c.evento(n, *d, &alvo, false, None, None)).collect();
        assert_eq!(acoes[1], Some(Acao::Iniciar));
    }
}
