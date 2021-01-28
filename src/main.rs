///////////////////////////////////////////////////////////////////////////////////////////////////////
// imports

use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{web, middleware, get, App, HttpServer, Responder, Error, HttpRequest, HttpResponse};
use actix_files as fs;
use actix_web_actors::ws;

use log::{log_enabled, error, info, Level, LevelFilter, Record, Metadata, set_logger, set_max_level};

use serde::{Serialize, Deserialize};

use lichessbot::lichessbot::*;

///////////////////////////////////////////////////////////////////////////////////////////////////////
// config

/// how often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration     = Duration::from_secs(5);
/// how long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration         = Duration::from_secs(10);
/// websocket clients queue capacity
const MAX_WEBSOCKET_CLIENTS: usize     = 10;
/// kickstart queue capacity, at most this many old log messages are stored
const KICKSTART_QUEUE_CAPACITY: usize  = 20;

///////////////////////////////////////////////////////////////////////////////////////////////////////
// models and implementation

/// log message to be sent to weblogger
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "()")]
#[serde(rename_all = "camelCase")]
pub struct LogMsg {
    pub naive_time: String,
    pub file: Option<String>,
    pub module_path: Option<String>,
    pub msg: String,
    pub formatted: String
}

/// log message implementation
impl LogMsg {
    /// convert to json string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// log manager that has all the client websocket addresses
struct LogManager {
    /// old messages
    kickstart: std::collections::VecDeque<LogMsg>,
    /// client websocket adresses
    client_addrs: std::collections::VecDeque<Addr<MyWebSocket>>,
}

/// message handler for websocket
impl Handler<LogMsg> for MyWebSocket {
    /// result type
    type Result = ();

    /// handle log message
    fn handle(&mut self, msg: LogMsg, ctx: &mut Self::Context) {
        ctx.text(msg.formatted);
    }
}

/// implementation of log manager
impl LogManager {
    /// create new log manager
    fn new() -> LogManager {
        LogManager {
            kickstart: std::collections::VecDeque::new(),
            client_addrs: std::collections::VecDeque::new(),
        }
    }
}

/// web logger, holds a mutex protected log manager
/// has to be separate from log manager, because set_logger consumes it
struct WebLogger {    
    /// log manager
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
    /// create new web logger
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
        // always return true and let set_max_level do the job
        true
    }

    /// do actual logging
    fn log(&self, record: &Record) {
        // get naive time
        let naive_time = chrono::Utc::now().naive_local();

        // formatted log
        let formatted = format!("{:?} : < file {:?} > [ module {:?} ] : {}",
            naive_time, record.file(), record.module_path(), record.args());

        // print log to stdout
        println!("{}", formatted);

        let log_msg = LogMsg {
            naive_time: format!("{}", naive_time),
            file: record.file().map(String::from),
            module_path: record.module_path().map(String::from),
            msg: format!("{}", record.args()),
            formatted: format!("{}", formatted),
        };

        // get mutable refernce to log manager
        let mut log_man = self.log_man.lock().unwrap();

        // send log to websockets
        for addr in log_man.client_addrs.iter() {
            addr.do_send(log_msg.clone());
        }

        // push back log message
        log_man.kickstart.push_back(log_msg);

        // limit kickstart queue size
        while log_man.kickstart.len() > KICKSTART_QUEUE_CAPACITY {
            let _ = log_man.kickstart.pop_front();
        }
    }

    /// flush
    fn flush(&self) {
        // nothing to be done, should be flushed already
    }
}

/// actor to handle websocket connection
/// client must send ping at least once in every CLIENT_TIMEOUT seconds
/// otherwise drop connection
struct MyWebSocket {
    /// last heartbeat instant
    hb: Instant,
    /// log manager
    log_man: web::Data<std::sync::Mutex::<LogManager>>,
}

/// implement actor for websocket
impl Actor for MyWebSocket {
    /// specify context as websocket context
    type Context = ws::WebsocketContext<Self>;

    /// handle started
    fn started(&mut self, ctx: &mut Self::Context) {
        // start heartbeat on actor start
        self.hb(ctx);

        // determine actor address
        let addr = ctx.address();

        // get mutable reference to log manager
        let mut log_man = self.log_man.lock().unwrap();

        // push back client address
        log_man.client_addrs.push_back(addr);

        // limit number of client adresses stored
        while log_man.client_addrs.len() > MAX_WEBSOCKET_CLIENTS {
            log_man.client_addrs.pop_front();
        }

        // send kickstart
        for log_msg in log_man.kickstart.iter() {
            // send message
            ctx.text(log_msg.to_json());
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
    fn new(log_man: web::Data<std::sync::Mutex::<LogManager>>) -> MyWebSocket {
        MyWebSocket {
            hb: Instant::now(),
            log_man: log_man,
        }
    }

    /// heartbeat method, sends ping to client at HEARTBEAT_INTERVAL
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
    // get reference to bot state
    let bot_state = bot_state.lock().await;

    // send response
    HttpResponse::Ok().content_type("text/html").body(format!("{:?}<hr><a href='/ws'>web logger</a>", bot_state))    
}

///////////////////////////////////////////////////////////////////////////////////////////////////////
// app entry point

/// actix web main
#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    // create log manager wrapped in web data
    let log_man = web::Data::new(std::sync::Mutex::new(LogManager::new()));

    // create static web logger
    let web_logger = Box::leak(Box::new(WebLogger::new(log_man.clone())));

    // set logger
    let _ = set_logger(web_logger);

    // get level filter from RUST_LOG env var
    let level_filter = level_str_to_error_level_filter(std::env::var("RUST_LOG").unwrap_or("off".to_string()));

    // always print logging level, even if logging is turned off
    println!("lichessbot startup, logging level = {}", level_filter);

    // set max logging level
    set_max_level(level_filter);

    // create static bot
    let bot = Box::leak(Box::new(
        LichessBot::new()
        .uci_opt("Move Overhead", std::env::var("RUST_BOT_ENGINE_MOVE_OVERHEAD").unwrap_or("500".to_string()))
        .uci_opt("Threads", std::env::var("RUST_BOT_ENGINE_THREADS").unwrap_or("4".to_string()))
        .uci_opt("Hash", std::env::var("RUST_BOT_ENGINE_HASH").unwrap_or("128".to_string()))
        .uci_opt("Contempt", std::env::var("RUST_BOT_ENGINE_CONTEMPT").unwrap_or("-25".to_string()))
        .enable_casual(true)
    ));
    
    // create web data for bot state
    let bot_data = web::Data::new(bot.state.clone());
    
    // spawn bot
    let spawn_result = tokio::spawn(async move {
        if log_enabled!(Level::Info){
            info!("starting bot stream");
        }

        bot.stream().await
    }).await;

    // determine port injected by cloud provider or use 8080 for local development
    let port = std::env::var("PORT").unwrap_or("8080".to_string());

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
