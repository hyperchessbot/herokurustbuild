use log::{log_enabled, info, Level};

use std::sync::mpsc;
use std::{thread, time};

use actix_web::{middleware, rt, web, App, HttpServer, get, Responder, HttpResponse};

use lichessbot::lichessbot::*;

#[get("/")]
async fn index(bot_state: web::Data::<std::sync::Arc<tokio::sync::Mutex<BotState>>>) -> impl Responder {
    format!("Hello! {:?}", bot_state)
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

async fn spawn_bot(bot: LichessBot){
    tokio::spawn(async move {
        let mut bot = bot;

        if log_enabled!(Level::Info){
            info!("starting bot {:?}", std::env::var("RUST_BOT_NAME"));
        }

        let _ = bot.stream().await;
    });
}

#[tokio::main]
async fn main() {    
    env_logger::init();

    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    let (tx, rx) = mpsc::channel();

    let bot = LichessBot::new();

    let bot_data = web::Data::new(bot.state.clone());

    let bot_data_clone = bot_data.clone();

    thread::spawn(move || {
        let bot_data = bot_data_clone;
        
        let mut sys = rt::System::new("test");

        if log_enabled!(Level::Info){
            info!("launching server at port {}", port);
        }
        
        let srv = HttpServer::new(move || App::new()
            .app_data(bot_data.clone())
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

        let _ = tx.send(srv.clone());

        sys.block_on(srv)
    });

    let srv = rx.recv().unwrap();

    let _ = spawn_bot(bot).await;

    thread::sleep(time::Duration::from_secs(40));

    if log_enabled!(Level::Info){
        info!("shutting down server");
    }

    rt::System::new("").block_on(srv.stop(true));
}
