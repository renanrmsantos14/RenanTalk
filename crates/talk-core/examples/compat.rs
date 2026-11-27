//! Prova de compatibilidade com o Python: lê e grava na pasta recebida (nunca na real).
//! Uso: cargo run -q -p talk-core --example compat -- ler|gravar|hist-ler|hist-gravar|subst <pasta> [texto]
use serde_json::{json, Value};
use std::path::Path;
use talk_core::{ajustes, comandos, historico, substituicoes};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, dir) = (args.get(1).map_or("", String::as_str), Path::new(args.get(2).expect("pasta")));
    match cmd {
        "ler" => println!("{}", json!({ "ajustes": ajustes::ler(dir) })),
        "gravar" => {
            let novos: Value = json!({ "volume": 42, "microfone": "Microfone ç 😀", "tema": "escuro", "luz": { "cor": "verde", "topo": 1 }, "extra_rust": [1, 2] });
            ajustes::salvar(dir, novos.as_object().unwrap()).expect("salvar");
        }
        // histórico: lista e resumo como a janela os devolve
        "hist-ler" => println!("{}", json!({ "itens": comandos::historico_listar(dir, "")["itens"], "inicio": comandos::inicio(dir) })),
        "hist-gravar" => {
            for (t, app, modo) in [("Reunião às 15h 😀 com Marcondes", "code", "formal"), ("segundo ditado", "", "")] {
                let id = historico::gravar(dir, &historico::NovoDitado { texto: t, app, modo, idioma: "pt", duracao_s: 2.345, original: Some("cru") });
                println!("id {id:?}");
            }
            for _ in 0..5 {
                historico::gravar(dir, &historico::NovoDitado { texto: "falei com Marcondes sobre o CTF", ..Default::default() });
            }
        }
        // aplica regras do arquivo e comandos dos ajustes ao texto recebido
        "subst" => {
            let texto = args.get(3).expect("texto");
            let a = ajustes::ler(dir);
            println!("{}", json!(substituicoes::aplicar(texto, &a["comandos"], &substituicoes::ler_regras(dir))));
        }
        _ => panic!("comando desconhecido"),
    }
}
