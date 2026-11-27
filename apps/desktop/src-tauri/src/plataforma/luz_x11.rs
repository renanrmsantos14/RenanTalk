//! Luz no X11: janela override-redirect com visual ARGB de 32 bits (sem foco, sem barra de tarefas, sem capturar
//! clique), como o `Gtk.WindowType.POPUP` do Python. O desenho vem de `talk_core::luz`; aqui só se escolhe o monitor
//! e se envia o quadro (PutImage). Sem compositor o X pinta o transparente de preto; com ele a faixa fica translúcida.

use talk_core::luz::{Geometria, Quadro, Superficie};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, ImageFormat,
    StackMode, VisualClass, WindowClass,
};
use x11rb::rust_connection::RustConnection;

pub struct JanelaX11 {
    conn: RustConnection,
    raiz: u32,
    janela: u32,
    gc: u32,
    visivel: bool,
    atual: Option<(Geometria, u32)>, // geometria e altura já aplicadas à janela
}

fn e(erro: impl std::fmt::Display) -> String {
    erro.to_string()
}

impl JanelaX11 {
    /// Abre a conexão e cria a janela (oculta). Falha se o servidor não tem visual ARGB de 32 bits.
    pub fn criar() -> Result<Self, String> {
        let (conn, n) = RustConnection::connect(None).map_err(e)?;
        let tela = &conn.setup().roots[n];
        let raiz = tela.root;
        let visual = tela
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .map(|v| v.visual_id)
            .ok_or("o servidor X não tem visual ARGB de 32 bits")?;
        let (cmap, janela, gc) = (conn.generate_id().map_err(e)?, conn.generate_id().map_err(e)?, conn.generate_id().map_err(e)?);
        conn.create_colormap(ColormapAlloc::NONE, cmap, raiz, visual).map_err(e)?;
        conn.create_window(
            32, janela, raiz, 0, 0, 1, 1, 0, WindowClass::INPUT_OUTPUT, visual,
            &CreateWindowAux::new().override_redirect(1).border_pixel(0).background_pixel(0).colormap(cmap).event_mask(EventMask::NO_EVENT),
        ).map_err(e)?;
        // Região de entrada vazia: o clique passa para a janela de baixo.
        conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, x11rb::protocol::xproto::ClipOrdering::UNSORTED, janela, 0, 0, &[]).map_err(e)?;
        conn.create_gc(gc, janela, &CreateGCAux::new()).map_err(e)?;
        conn.flush().map_err(e)?;
        Ok(JanelaX11 { conn, raiz, janela, gc, visivel: false, atual: None })
    }

    fn atom(&self, nome: &str) -> Result<u32, String> {
        Ok(self.conn.intern_atom(false, nome.as_bytes()).map_err(e)?.reply().map_err(e)?.atom)
    }

    /// Monitores (RANDR 1.5) como (x, y, largura, altura); a tela inteira se o servidor não os der.
    fn monitores(&self) -> Vec<(i32, i32, u32, u32)> {
        let lista = self.conn.randr_get_monitors(self.raiz, true).ok().and_then(|c| c.reply().ok()).map(|r| {
            r.monitors.iter().map(|m| (m.x as i32, m.y as i32, m.width as u32, m.height as u32)).filter(|m| m.2 > 0).collect::<Vec<_>>()
        });
        match lista {
            Some(l) if !l.is_empty() => l,
            _ => self.conn.get_geometry(self.raiz).ok().and_then(|c| c.reply().ok()).map(|g| vec![(0, 0, g.width as u32, g.height as u32)]).unwrap_or_default(),
        }
    }

    /// Ponto de referência para escolher o monitor: o centro da janela ativa ou, sem ela, o ponteiro.
    fn referencia(&self) -> Option<(i32, i32)> {
        let ativa = (|| -> Result<(i32, i32), String> {
            let a = self.conn.get_property(false, self.raiz, self.atom("_NET_ACTIVE_WINDOW")?, AtomEnum::WINDOW, 0, 1).map_err(e)?.reply().map_err(e)?;
            let jan = a.value32().and_then(|mut v| v.next()).filter(|j| *j != 0).ok_or("sem janela ativa")?;
            let g = self.conn.get_geometry(jan).map_err(e)?.reply().map_err(e)?;
            let t = self.conn.translate_coordinates(jan, self.raiz, 0, 0).map_err(e)?.reply().map_err(e)?;
            Ok((t.dst_x as i32 + g.width as i32 / 2, t.dst_y as i32 + g.height as i32 / 2))
        })();
        ativa.ok().or_else(|| self.conn.query_pointer(self.raiz).ok().and_then(|c| c.reply().ok()).map(|p| (p.root_x as i32, p.root_y as i32)))
    }

    /// Área de trabalho do desktop 0 (`_NET_WORKAREA`): descontada a barra/painel, se houver.
    fn area_de_trabalho(&self) -> Option<(i32, i32, i32, i32)> {
        let p = self.conn.get_property(false, self.raiz, self.atom("_NET_WORKAREA").ok()?, AtomEnum::CARDINAL, 0, 4).ok()?.reply().ok()?;
        let v: Vec<u32> = p.value32()?.collect();
        (v.len() >= 4).then(|| (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32))
    }
}

impl Superficie for JanelaX11 {
    fn posicionar(&mut self, topo: bool, altura: u32) -> Result<Geometria, String> {
        let mons = self.monitores();
        let (px, py) = self.referencia().unwrap_or((0, 0));
        let dentro = |m: &&(i32, i32, u32, u32)| px >= m.0 && px < m.0 + m.2 as i32 && py >= m.1 && py < m.1 + m.3 as i32;
        let &(mx, my, mw, mh) = mons.iter().find(dentro).or(mons.first()).ok_or("nenhum monitor")?;
        let (mb, mr) = (my + mh as i32, mx + mw as i32);
        // área do monitor sem o painel: a interseção com a área de trabalho (se ela não cruzar o monitor, o monitor inteiro)
        let (ay, ab) = match self.area_de_trabalho() {
            Some((ax, ay, aw, ah)) if ax < mr && ax + aw > mx && ay < mb && ay + ah > my => (ay.max(my), (ay + ah).min(mb)),
            _ => (my, mb),
        };
        let y = if topo { ay } else { ab - altura as i32 }; // encosta no painel, não atrás dele
        Ok(Geometria { x: mx, y, largura: mw })
    }

    fn mostrar(&mut self, g: Geometria, q: &Quadro) -> Result<(), String> {
        if self.atual != Some((g, q.altura)) {
            self.conn.configure_window(self.janela, &ConfigureWindowAux::new().x(g.x).y(g.y).width(g.largura).height(q.altura).stack_mode(StackMode::ABOVE)).map_err(e)?;
            self.atual = Some((g, q.altura));
        }
        if !self.visivel {
            self.conn.map_window(self.janela).map_err(e)?;
            self.visivel = true;
        }
        let dados = q.bgra(); // ARGB de 32 bits little-endian = B, G, R, A
        let linha = q.largura as usize * 4;
        let por_pedido = ((self.conn.maximum_request_bytes().min(1 << 20) - 64) / linha).max(1);
        for (i, bloco) in dados.chunks(por_pedido * linha).enumerate() {
            let linhas = (bloco.len() / linha) as u16;
            let y = (i * por_pedido) as i16;
            self.conn.put_image(ImageFormat::Z_PIXMAP, self.janela, self.gc, q.largura as u16, linhas, 0, y, 0, 32, bloco).map_err(e)?;
        }
        self.conn.flush().map_err(e)
    }

    fn esconder(&mut self) {
        if self.visivel {
            let _ = self.conn.unmap_window(self.janela).and_then(|_| self.conn.flush());
            self.visivel = false;
        }
    }

    fn reafirmar(&mut self) {
        let _ = self.conn.configure_window(self.janela, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
    }

    fn bombear(&mut self) {
        while let Ok(Some(_)) = self.conn.poll_for_event() {} // descarta Expose e afins: o próximo quadro repinta
    }
}
