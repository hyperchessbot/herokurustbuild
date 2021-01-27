use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{get, App, HttpServer, web, Responder, middleware, Error, HttpRequest, HttpResponse};
use actix_files as fs;
use actix_web_actors::ws;

use log::{log_enabled, error, info, Level, Record, Metadata, set_logger, set_max_level, LevelFilter};

use lichessbot::lichessbot::*;

/// how often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// how long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
/// websocket clients queue capacity
const MAX_WEBSOCKET_CLIENTS: usize = 10;

/// log message to be sent to weblogger
#[derive(Message, Debug)]
#[rtype(result = "()")]
struct LogMsg(String);

/// log manager that has all the client websocket addresses
struct LogManager {
    client_addrs: std::collections::VecDeque<Addr<MyWebSocket>>,
}

/// message handler for websocket
impl Handler<LogMsg> for MyWebSocket {
    type Result = ();

    /// handle log message
    fn handle(&mut self, msg: LogMsg, ctx: &mut Self::Context) {
        let LogMsg(msg) = msg;

        ctx.text(msg);
    }
}

/// implementation of log manager
impl LogManager {
    /// create neew log manager
    fn new() -> LogManager {
        LogManager {
            client_addrs: std::collections::VecDeque::new(),
        }
    }
}

/// web logger, holds a mutex protected log manager
struct WebLogger {    
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
}

/// convert verbal error level to level filter
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

/// web logger implementation
impl WebLogger {
    fn new(log_man: web::Data<std::sync::Mutex::<LogManager>>) -> WebLogger {
        WebLogger {            
            log_man: log_man,
        }
    }
}

/// log implementation for web logger
impl log::Log for WebLogger {
    /// determine if logging is enabled
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    /// do actual logging
    fn log(&self, record: &Record) {
        // get naive time
        let time = chrono::Utc::now().naive_local();

        // formatted log
        let formatted = format!("{:?} : < file {:?} > [ module {:?} ] : {}",
            time, record.file(), record.module_path(), record.args());

        // print log to stdout
        println!("{}", formatted);

        // get mutable refernce to log manager
        let data = self.log_man.lock().unwrap();

        // send log to websockets
        for addr in data.client_addrs.iter() {
            addr.do_send(LogMsg(format!("{}", formatted)));
        }
    }

    /// flush
    fn flush(&self) {
        // nothing to be done, should be flushed already
    }
}

/// actor to handle websocket connection
struct MyWebSocket {
    /// client must send ping at least once in every CLIENT_TIMEOUT seconds
    /// otherwise drop connection
    hb: Instant,
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
}

/// implement actor for websocket
impl Actor for MyWebSocket {
    /// specify context as websocket context
    type Context = ws::WebsocketContext<Self>;

    /// start heartbeat on actor start
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);

        let addr = ctx.address();

        // get mutable reference to log manager
        let mut log_man = self.log_man.lock().unwrap();

        // push back client address
        log_man.client_addrs.push_back(addr);

        // limit number of client adresses stored
        while log_man.client_addrs.len() > MAX_WEBSOCKET_CLIENTS {
            log_man.client_addrs.pop_front();
        }
    }
}

/// handler for websocket message
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    // handle websocket message
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg)
            },
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now()
            },
            Ok(ws::Message::Text(text)) => {
                ctx.text(text)
            },
            Ok(ws::Message::Binary(bin)) => {
                ctx.binary(bin)
            },
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop()
            },
            _ => {
                ctx.stop()
            },
        }
    }
}

/// websocket implementation
impl MyWebSocket {
    /// create new websocket
    fn new(log_man: web::Data<std::sync::Mutex::<LogManager>>) -> Self {
        Self {
            hb: Instant::now(),
            log_man: log_man,
        }
    }

    /// heartbeat method, sends ping to client every second
    /// checks heartbeats from client
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                if log_enabled!(Level::Error) {
                    error!("websocket heartbeat failed, disconnecting!");
                }                

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            // send ping
            ctx.ping(b"");
        });
    }
}

/// do websocket handshake and start websocket actor
async fn ws_index(
    r: HttpRequest,
    stream: web::Payload,
    log_man: web::Data<std::sync::Mutex::<LogManager>>
) -> Result<HttpResponse, Error> {
    // start websocket
    let res = ws::start(MyWebSocket::new(log_man), &r, stream);    

    res
}

/// index route
#[get("/")]
async fn index(
    bot_state: web::Data::<std::sync::Arc<tokio::sync::Mutex<BotState>>>,    
) -> impl Responder {
    // get mutable refernce bot state manager
    let data = bot_state.lock().await;

    // send response
    HttpResponse::Ok().content_type("text/html").body(format!("{:?}<hr><a href='/ws'>web logger</a>", data))    
}

/// actix web main
#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    // determine port injected by cloud provider or use 8080 for local development
    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    // create static bot
    let bot = Box::leak(Box::new(LichessBot::new().enable_casual(true)));
    
    // create web date for bot state
    let bot_data = web::Data::new(bot.state.clone());

    // create log manager wrapped in web data
    let log_man = web::Data::new(std::sync::Mutex::new(LogManager::new()));

    // create static web logger
    let web_logger = Box::leak(Box::new(WebLogger::new(log_man.clone())));

    // set logger
    let _ = set_logger(web_logger);

    // get level filter from RUST_LOG env var
    let level_filter = level_str_to_error_level_filter(std::env::var("RUST_LOG").unwrap_or("off".to_string()));

    // always print logging level, even if logging is turned off
    println!("logging level {}", level_filter);

    // set max logging level
    set_max_level(level_filter);
    
    // spawn bot
    let spawn_result = tokio::spawn(async move {
        if log_enabled!(Level::Info){
            info!("starting bot stream");
        }

        bot.stream().await
    }).await;

    // spawn server
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

    // wait for Ctrl+C
    let _ = tokio::signal::ctrl_c().await;

    // shut down bot gracefully
    if let Ok((tx, mut rxa)) = spawn_result {
        let _ = tx.send("stopped by user".to_string()).await;

        if log_enabled!(Level::Error) {
            error!("{:?}", rxa.recv().await);
        }	    
    }
    
    // done
	Ok(())
}
