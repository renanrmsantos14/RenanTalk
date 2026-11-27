//! Prova de compatibilidade com o Python: lê e grava na pasta recebida (nunca na real).
//! Uso: cargo run -q -p talk-core --example compat -- ler|gravar <pasta>
use serde_json::{json, Value};
use std::path::Path;
use talk_core::ajustes;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, dir) = (args.get(1).map_or("", String::as_str), Path::new(args.get(2).expect("pasta")));
    match cmd {
        "ler" => println!("{}", json!({ "ajustes": ajustes::ler(dir) })),
        "gravar" => {
            let novos: Value = json!({ "volume": 42, "microfone": "Microfone ç 😀", "tema": "escuro", "luz": { "cor": "verde", "topo": 1 }, "extra_rust": [1, 2] });
            ajustes::salvar(dir, novos.as_object().unwrap()).expect("salvar");
        }
        _ => panic!("use ler ou gravar"),
    }
}
