use std::{io::Write, net::SocketAddr, sync::Arc};

use concordia_crossing::db::Database;
use concordia_crossing::web;

struct Options {
    listen: SocketAddr,
    database: String,
    no_seed: bool,
}

fn parse_options() -> Options {
    let mut listen: SocketAddr = "127.0.0.1:5532".parse().expect("valid default address");
    let mut database = "concordia.db".to_string();
    let mut no_seed = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--listen" => {
                let value = arguments.next().expect("--listen requires an address");
                listen = value.parse().expect("valid socket address");
            }
            "--database" => {
                database = arguments.next().expect("--database requires a path");
            }
            "--no-seed" => no_seed = true,
            "--help" | "-h" => {
                println!("Usage: concordia-crossing [--listen 127.0.0.1:5532] [--database concordia.db] [--no-seed]");
                std::process::exit(0);
            }
            other => panic!("unknown argument: {other}"),
        }
    }
    Options {
        listen,
        database,
        no_seed,
    }
}

#[tokio::main]
async fn main() {
    let options = parse_options();
    let database = Arc::new(Database::open(&options.database).expect("open SQLite database"));
    if !options.no_seed {
        let inserted = database.seed_fixtures().expect("seed fixed fixtures");
        if inserted > 0 {
            println!("seeded {inserted} fixed fixtures");
        }
    }
    let app = web::router(database);
    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .expect("bind listen address");
    println!("协和线交台 listening on http://{}", options.listen);
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "database: {}", options.database);
    axum::serve(listener, app).await.expect("run axum server");
}
