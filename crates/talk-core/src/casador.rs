//! Casa as teclas apertadas com os atalhos (porta de `ajustes.Casador`). Sem teclado nem sistema:
//! recebe só nomes ("ctrl", "win", "d"...) e devolve o que fazer.
//!
//! Regra herdada do Ctrl+Win: com o ditado em andamento, qualquer tecla fora do atalho segurado cancela.
//! Traduzir funciona como ditar (segure, fale e solte), mas cola em inglês. O atalho de colar só vale com o
//! ditado parado e dispara uma vez por vez apertada (sem repetição da tecla).

use crate::ajustes::MODIFICADORES;
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Acao {
    Iniciar,
    Traduzir,
    Parar,
    Colar,
    Outra,
}

type Atalho = Vec<String>;

#[derive(Default)]
pub struct Casador {
    pub apertadas: HashSet<String>,
    alvos: (Atalho, Option<Atalho>, Option<Atalho>),
    colou: bool,
    segurado: Atalho, // atalho que começou o ditado em andamento (ditar ou traduzir)
}

impl Casador {
    pub fn evento(&mut self, nome: &str, descer: bool, alvo: &[String], gravando: bool,
                  alvo_colar: Option<&[String]>, alvo_traduzir: Option<&[String]>) -> Option<Acao> {
        let alvo_colar = alvo_colar.filter(|c| *c != alvo); // iguais: o de ditar manda
        let alvo_traduzir = alvo_traduzir.filter(|t| *t != alvo && Some(*t) != alvo_colar);
        let alvos = (alvo.to_vec(), alvo_colar.map(<[String]>::to_vec), alvo_traduzir.map(<[String]>::to_vec));
        if alvos != self.alvos { // atalho trocado nas configurações: recomeça do zero
            self.alvos = alvos;
            self.apertadas.clear();
            self.colou = false;
        }
        let (colar, traduzir) = (alvo_colar.unwrap_or(&[]), alvo_traduzir.unwrap_or(&[]));
        let de = |a: &[String]| a.iter().any(|t| t == nome);
        let igual = |a: &[String], conj: &HashSet<String>| !a.is_empty() && a.len() == conj.len() && a.iter().all(|t| conj.contains(t));
        if descer {
            if !(de(alvo) || de(colar) || de(traduzir)) && !MODIFICADORES.contains(&nome) {
                return gravando.then_some(Acao::Outra);
            }
            self.apertadas.insert(nome.to_string());
            if gravando {
                return if de(&self.segurado) { None } else { Some(Acao::Outra) };
            }
            if igual(alvo, &self.apertadas) {
                self.segurado = alvo.to_vec();
                return Some(Acao::Iniciar);
            }
            if igual(traduzir, &self.apertadas) {
                self.segurado = traduzir.to_vec();
                return Some(Acao::Traduzir);
            }
            if igual(colar, &self.apertadas) && !self.colou {
                self.colou = true;
                return Some(Acao::Colar);
            }
            return None;
        }
        self.apertadas.remove(nome);
        if de(colar) {
            self.colou = false;
        }
        (gravando && de(&self.segurado)).then_some(Acao::Parar)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(s: &str) -> Atalho {
        s.split('+').map(String::from).collect()
    }
    /// Roda os passos (tecla, desceu) e vai ligando/desligando "gravando" como o ditado faz.
    fn rodar(alvo: &str, colar: Option<&str>, traduzir: Option<&str>, passos: &[(&str, bool)]) -> Vec<Option<Acao>> {
        rodar_com(alvo, colar, traduzir, passos, true)
    }
    /// `acompanha` false: `gravando` fica sempre falso (só testa o casamento das teclas).
    fn rodar_com(alvo: &str, colar: Option<&str>, traduzir: Option<&str>, passos: &[(&str, bool)], acompanha: bool) -> Vec<Option<Acao>> {
        let (alvo, colar, traduzir) = (a(alvo), colar.map(a), traduzir.map(a));
        let (mut c, mut gravando, mut saidas) = (Casador::default(), false, vec![]);
        for (nome, desceu) in passos {
            let r = c.evento(nome, *desceu, &alvo, gravando, colar.as_deref(), traduzir.as_deref());
            match r.filter(|_| acompanha) {
                Some(Acao::Iniciar | Acao::Traduzir) => gravando = true,
                Some(Acao::Parar) => gravando = false,
                _ => {}
            }
            saidas.push(r);
        }
        saidas
    }
    const D: &str = "ctrl+win";
    const C: &str = "alt+win+v";
    const T: &str = "shift+win";

    #[test]
    fn ctrl_alt_e_modificadores_mais_tecla() {
        assert_eq!(rodar("ctrl+alt", None, None, &[("ctrl", true), ("alt", true), ("alt", false)]), [None, Some(Acao::Iniciar), Some(Acao::Parar)]);
        assert_eq!(rodar("ctrl+alt+d", None, None, &[("ctrl", true), ("alt", true), ("d", true), ("d", false)]), [None, None, Some(Acao::Iniciar), Some(Acao::Parar)]);
    }

    #[test]
    fn ctrl_win_nao_dispara_com_alt_e_outra_tecla_cancela() {
        assert_eq!(rodar(D, None, None, &[("ctrl", true), ("alt", true), ("win", true)]), [None, None, None]);
        assert_eq!(rodar(D, None, None, &[("ctrl", true), ("win", true), ("x", true), ("win", false)]), [None, Some(Acao::Iniciar), Some(Acao::Outra), Some(Acao::Parar)]);
    }

    #[test]
    fn colar_e_ditar_separados_e_sem_repetir() {
        assert_eq!(rodar(D, Some(C), None, &[("alt", true), ("win", true), ("v", true), ("v", false)]), [None, None, Some(Acao::Colar), None]);
        assert_eq!(rodar_com(D, Some(C), None, &[("ctrl", true), ("win", true), ("win", false)], false), [None, Some(Acao::Iniciar), None]);
        let s = rodar_com(D, Some(C), None, &[("alt", true), ("win", true), ("v", true), ("v", true), ("v", false), ("v", true)], false);
        assert_eq!(s, [None, None, Some(Acao::Colar), None, None, Some(Acao::Colar)]);
    }

    #[test]
    fn tecla_do_colar_sozinha_ou_com_modificadores_errados_nao_dispara() {
        assert_eq!(rodar(D, Some(C), None, &[("v", true), ("v", false)]), [None, None]);
        assert_eq!(rodar(D, Some(C), None, &[("ctrl", true), ("alt", true), ("v", true)]), [None, None, None]);
    }

    #[test]
    fn v_do_colar_com_ditado_em_andamento_cancela() {
        let s = rodar(D, Some(C), None, &[("ctrl", true), ("win", true), ("v", true), ("win", false)]);
        assert_eq!(s, [None, Some(Acao::Iniciar), Some(Acao::Outra), Some(Acao::Parar)]);
    }

    #[test]
    fn colar_ignorado_gravando_e_igual_ao_de_ditar() {
        let (alvo, colar) = (a(D), a(C));
        let mut c = Casador::default();
        for (n, d) in [("alt", true), ("win", true)] { // gravando: nunca devolve Colar
            c.evento(n, d, &alvo, true, Some(&colar), None);
        }
        assert_ne!(c.evento("v", true, &alvo, true, Some(&colar), None), Some(Acao::Colar));
        assert_eq!(rodar(D, Some(D), None, &[("ctrl", true), ("win", true)]), [None, Some(Acao::Iniciar)]);
    }

    #[test]
    fn segurar_traduz_e_soltar_para() {
        let s = rodar(D, Some(C), Some(T), &[("shift", true), ("win", true), ("shift", true), ("shift", false)]);
        assert_eq!(s, [None, Some(Acao::Traduzir), None, Some(Acao::Parar)]); // a repetição da tecla segurada não faz nada
    }

    #[test]
    fn ditar_continua_igual_e_tecla_do_traduzir_cancela() {
        let s = rodar(D, Some(C), Some(T), &[("ctrl", true), ("win", true), ("shift", true), ("ctrl", false)]);
        assert_eq!(s, [None, Some(Acao::Iniciar), Some(Acao::Outra), Some(Acao::Parar)]);
    }
}
