use std::sync::mpsc;
use std::{thread, time};

use actix_web::{dev::Server, middleware, rt, web, App, HttpServer, get, Responder, HttpResponse};

use lichessbot::lichessbot::*;

#[get("/")]
async fn index() -> impl Responder {
    format!("Hello!")
}

async fn page() -> impl Responder {
    HttpResponse::Ok().body("Page!")
}

#[get("/{name}")]
async fn welcome(web::Path(name): web::Path<String>) -> impl Responder {
    format!("Hello {}!", name)
}

#[get("/{greeting}/{name}")]
async fn greeting(web::Path((greeting, name)): web::Path<(String, String)>) -> impl Responder {
    format!("{} {}!", greeting, name)
}

fn run_app(tx: mpsc::Sender<Server>) -> std::io::Result<()> {
    let mut sys = rt::System::new("test");

    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    println!("launching server at port {}", port);

    let srv = HttpServer::new(|| App::new()
        .wrap(middleware::Logger::default())
        .service(index)
        .service(
            actix_web::web::resource(vec!["/page"])
                .route(actix_web::web::get().to(page))
                .route(actix_web::web::post().to(page))
        )
        .service(welcome)
        .service(greeting)        
    )
        .bind(format!("0.0.0.0:{}", port))?
        .run();

    // send server controller to main thread
    let _ = tx.send(srv.clone());

    // run future
    sys.block_on(srv)
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "actix_web=info,actix_server=trace");
    //env_logger::init();

    let (tx, rx) = mpsc::channel();

    println!("START SERVER");
    thread::spawn(move || {
        let _ = run_app(tx);
    });

    let srv = rx.recv().unwrap();

    println!("WAITING 10 SECONDS");
    thread::sleep(time::Duration::from_secs(20));

    println!("STOPPING SERVER");
    // init stop server and wait until server gracefully exit
    rt::System::new("").block_on(srv.stop(true));
}