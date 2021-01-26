use actix_web::{get, App, HttpServer, web, Responder};

use log::{log_enabled, info, Level};

extern crate env_logger;

use lichessbot::lichessbot::*;

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    env_logger::init();
    
    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    let bot = Box::leak(Box::new(LichessBot::new().enable_casual(true)));
    
    let bot_data = web::Data::new(bot.state.clone());
    
    let spawn_result = tokio::spawn(async move {
        if log_enabled!(Level::Info){
            info!("starting bot stream");
        }

        bot.stream().await
    }).await;

    HttpServer::new(move || App::new()
        .app_data(bot_data.clone())
        .service(index)
    )            
    .disable_signals()            
    .bind(format!("0.0.0.0:{}", port))?
    .run();

    let _ = tokio::signal::ctrl_c().await;

    if let Ok((tx, mut rxa)) = spawn_result {
        let _ = tx.send("stopped by user".to_string()).await;

	    println!("{:?}", rxa.recv().await);
    }
	
	Ok(())
}

#[get("/")]
async fn index(bot_state: web::Data::<std::sync::Arc<tokio::sync::Mutex<BotState>>>) -> impl Responder {
    format!("Bot state : {:?}", bot_state)
}
