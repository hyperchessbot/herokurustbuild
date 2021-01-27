use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{get, App, HttpServer, web, Responder, middleware, Error, HttpRequest, HttpResponse};
use actix_files as fs;
use actix_web_actors::ws;

use log::{log_enabled, error, info, Level, Record, Metadata, set_logger, set_max_level, LevelFilter};

extern crate env_logger;

use lichessbot::lichessbot::*;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct LogMsg(String);

struct LogManager {
    client_addrs: Vec<Addr<MyWebSocket>>,
}

impl Handler<LogMsg> for MyWebSocket {
    type Result = ();

    fn handle(&mut self, msg: LogMsg, ctx: &mut Self::Context) {
        let LogMsg(msg) = msg;

        ctx.text(msg);
    }
}

impl LogManager {
    fn new() -> LogManager {
        LogManager {
            client_addrs: vec!(),
        }
    }
}

struct WebLogger {    
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
}

fn level_str_to_error_level_filter<T: AsRef<str>>(level: T) -> LevelFilter {
    match level.as_ref() {
        "off" => LevelFilter::Off,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Off,
    }
}

impl WebLogger {
    fn new(log_man: web::Data<std::sync::Mutex::<LogManager>>) -> WebLogger {
        WebLogger {            
            log_man: log_man,
        }
    }
}

impl log::Log for WebLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let data = self.log_man.lock().unwrap();

        println!(">> {}", record.args());

        for addr in &data.client_addrs {
            addr.do_send(LogMsg(format!(">> {}", record.args())));
        }
    }

    fn flush(&self) {}
}

/// websocket connection is long running connection, it easier
/// to handle with an actor
struct MyWebSocket {
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);

        let addr = ctx.address();

        let mut log_man = self.log_man.lock().unwrap();

        log_man.client_addrs.push(addr);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        // process websocket messages        
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => ctx.text(text),
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl MyWebSocket {
    fn new(log_man: web::Data<std::sync::Mutex::<LogManager>>) -> Self {
        Self {
            hb: Instant::now(),
            log_man: log_man,
        }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                if log_enabled!(Level::Error) {
                    error!("Websocket Client heartbeat failed, disconnecting!");
                }                

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}


/// do websocket handshake and start `MyWebSocket` actor
async fn ws_index(
        r: HttpRequest,
        stream: web::Payload,
        log_man: web::Data<std::sync::Mutex::<LogManager>>
    ) -> Result<HttpResponse, Error> {    
    let res = ws::start(MyWebSocket::new(log_man), &r, stream);    
    res
}

#[get("/")]
async fn index(
    bot_state: web::Data::<std::sync::Arc<tokio::sync::Mutex<BotState>>>,
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
) -> impl Responder {
    let data = log_man.lock().unwrap();

    for addr in &data.client_addrs {
        addr.do_send(LogMsg(format!("{:?}", bot_state)));
    }

    format!("Bot state : {:?}", bot_state)
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    //env_logger::init();
    
    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    let bot = Box::leak(Box::new(LichessBot::new().enable_casual(true)));
    
    let bot_data = web::Data::new(bot.state.clone());

    let log_man = web::Data::new(std::sync::Mutex::new(LogManager::new()));

    let web_logger = Box::leak(Box::new(WebLogger::new(log_man.clone())));

    let _ = set_logger(web_logger);

    let level_filter = level_str_to_error_level_filter(std::env::var("RUST_LOG").unwrap_or("off".to_string()));

    println!("logging level {}", level_filter);

    set_max_level(level_filter);

    info!("logger set");
    
    let spawn_result = tokio::spawn(async move {
        if log_enabled!(Level::Info){
            info!("starting bot stream");
        }

        bot.stream().await
    }).await;

    HttpServer::new(move || App::new()
        .wrap(middleware::Logger::default())
        .service(web::resource("/ws/").route(web::get().to(ws_index)))
        .service(fs::Files::new("/ws", "static/").index_file("index.html"))
        .app_data(bot_data.clone())
        .app_data(log_man.clone())
        .service(index)
    )            
    .disable_signals()            
    .bind(format!("0.0.0.0:{}", port))?
    .run();

    let _ = tokio::signal::ctrl_c().await;

    if let Ok((tx, mut rxa)) = spawn_result {
        let _ = tx.send("stopped by user".to_string()).await;

        if log_enabled!(Level::Error) {
            error!("{:?}", rxa.recv().await);
        }	    
    }
	
	Ok(())
}
