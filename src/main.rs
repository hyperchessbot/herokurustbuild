use log::{log_enabled, info, Level};

extern crate env_logger;

use actix_web::{get, App, HttpServer, web, Responder};

use lichessbot::lichessbot::*;

fn main() -> std::io::Result<()> {
    env_logger::init();

    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    let bot = LichessBot::new().enable_casual(true);

    let bot_data = web::Data::new(bot.state.clone());

    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    tokio_rt.spawn(async move {
        let mut bot = bot;

        if log_enabled!(Level::Info){
            info!("starting bot {:?}", std::env::var("RUST_BOT_NAME"));
        }

        let _ = bot.stream().await;
    });

    let server = actix_web::rt::System::new("abc").block_on(async {
        let server = HttpServer::new(move || App::new()
            .app_data(bot_data.clone())
            .service(index)
            )            
            .disable_signals()            
            .bind(format!("0.0.0.0:{}", port))?
            .run();

        Ok::<_, std::io::Error>(server)
    })?;

     tokio_rt.block_on(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = server.stop(true).await;
        Ok(())
    })
}


#[get("/")]
async fn index(bot_state: web::Data::<std::sync::Arc<tokio::sync::Mutex<BotState>>>) -> impl Responder {
    format!("Hello! {:?}", bot_state)
}