//! Núcleo do RenanTalk em Rust. Lê e grava os mesmos arquivos do app Python, na mesma pasta de dados
//! (`%APPDATA%\RenanTalk`; no Linux `$XDG_CONFIG_HOME/RenanTalk` ou `~/.config/RenanTalk`).
//! Toda função recebe a pasta (`&Path`) para poder ser testada em diretório temporário; `pasta::padrao()` dá a real.

pub mod ajustes;
pub mod casador;
pub mod pasta;
pub mod comandos;
pub mod escrita;
pub mod historico;
pub mod substituicoes;
pub mod sugestoes;
