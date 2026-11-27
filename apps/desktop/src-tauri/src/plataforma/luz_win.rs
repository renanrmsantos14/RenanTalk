//! Luz no Windows: janela em camadas (`UpdateLayeredWindow`, alfa por pixel), sempre por cima, sem foco e sem
//! capturar clique, como o `app/luz.py`. O desenho vem de `talk_core::luz`; aqui só se escolhe o monitor
//! do app em primeiro plano e se envia o quadro. Vive numa thread própria, que também atende as mensagens da janela.

use std::ffi::c_void;
use talk_core::luz::{Geometria, Quadro, Superficie};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteObject, GetDC, GetMonitorInfoW, MonitorFromWindow, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, PeekMessageW, RegisterClassW, SetWindowPos,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

pub struct JanelaWin {
    hwnd: HWND,
    tela: HDC,
    mem: HDC,
    bitmap: Option<(HBITMAP, *mut c_void, u32, u32)>, // DIB atual, seus bytes, largura e altura
    visivel: bool,
}

unsafe extern "system" fn proc_janela(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, w, l)
}

impl JanelaWin {
    /// Cria a janela (oculta). Tem de rodar na thread que vai bombear as mensagens.
    pub fn criar() -> Result<Self, String> {
        unsafe {
            let inst = HINSTANCE(GetModuleHandleW(None).map_err(|e| e.to_string())?.0);
            let classe = WNDCLASSW { lpfnWndProc: Some(proc_janela), hInstance: inst, lpszClassName: w!("LuzDitadoRust"), ..Default::default() };
            RegisterClassW(&classe);
            let ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;
            let hwnd = CreateWindowExW(ex, w!("LuzDitadoRust"), PCWSTR::null(), WS_POPUP, 0, 0, 1, 1, None, None, Some(inst), None)
                .map_err(|e| e.to_string())?;
            let tela = GetDC(None);
            let mem = CreateCompatibleDC(Some(tela));
            Ok(JanelaWin { hwnd, tela, mem, bitmap: None, visivel: false })
        }
    }

    fn topo(&self) {
        // topo, sem foco, mostrar: a barra de tarefas também é "sempre no topo", então reafirma de tempos em tempos
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
        }
    }

    /// DIB de cima para baixo (altura negativa), 32 bits, do tamanho do quadro.
    fn garantir_bitmap(&mut self, largura: u32, altura: u32) -> Result<*mut c_void, String> {
        if let Some((_, bits, w, h)) = self.bitmap {
            if (w, h) == (largura, altura) {
                return Ok(bits);
            }
        }
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: largura as i32,
            biHeight: -(altura as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        unsafe {
            let hbmp = CreateDIBSection(Some(self.mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0).map_err(|e| e.to_string())?;
            SelectObject(self.mem, hbmp.into());
            if let Some((antigo, ..)) = self.bitmap.replace((hbmp, bits, largura, altura)) {
                let _ = DeleteObject(antigo.into());
            }
        }
        Ok(bits)
    }
}

impl Superficie for JanelaWin {
    fn posicionar(&mut self, topo: bool, altura: u32) -> Result<Geometria, String> {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let ok = unsafe { GetMonitorInfoW(MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST), &mut mi) };
        if !ok.as_bool() {
            return Err("GetMonitorInfo falhou".into());
        }
        let (r, t) = (mi.rcMonitor, mi.rcWork);
        let y = if topo {
            t.top.max(r.top) // barra de tarefas no alto: a linha encosta na base dela
        } else {
            t.bottom.min(r.bottom) - altura as i32 // barra embaixo: a linha encosta no topo dela
        };
        Ok(Geometria { x: r.left, y, largura: (r.right - r.left).max(2) as u32 })
    }

    fn mostrar(&mut self, g: Geometria, q: &Quadro) -> Result<(), String> {
        let bits = self.garantir_bitmap(q.largura, q.altura)?;
        let dados = q.bgra();
        unsafe {
            std::ptr::copy_nonoverlapping(dados.as_ptr(), bits as *mut u8, dados.len());
        }
        if !self.visivel {
            self.topo();
            self.visivel = true;
        }
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        unsafe {
            UpdateLayeredWindow(
                self.hwnd, Some(self.tela), Some(&POINT { x: g.x, y: g.y }), Some(&SIZE { cx: q.largura as i32, cy: q.altura as i32 }),
                Some(self.mem), Some(&POINT { x: 0, y: 0 }), COLORREF(0), Some(&blend), ULW_ALPHA,
            )
        }
        .map_err(|e| e.to_string())
    }

    fn esconder(&mut self) {
        if self.visivel {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visivel = false;
        }
    }

    fn reafirmar(&mut self) {
        self.topo();
    }

    fn bombear(&mut self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
