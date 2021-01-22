use actix_web::{get, web, App, HttpServer, Responder, HttpResponse};

#[get("/")]
async fn index() -> impl Responder {
    format!("Hello!")
}

#[get("/{name}")]
async fn welcome(web::Path(name): web::Path<String>) -> impl Responder {
    format!("Hello {}!", name)
}

#[get("/{greeting}/{name}")]
async fn greeting(web::Path((greeting, name)): web::Path<(String, String)>) -> impl Responder {
    format!("{} {}!", greeting, name)
}

async fn page() -> impl Responder {
    HttpResponse::Ok().body("Page!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::var("PORT").unwrap_or("8080".to_string());

    println!("launching server at port {}", port);

    HttpServer::new(|| App::new()
        .service(index)
        .service(welcome)
        .service(greeting)
        .service(
            actix_web::web::resource(vec!["/page"])
                .route(actix_web::web::get().to(page))
                .route(actix_web::web::post().to(page))
        )
    )
        .bind(format!("127.0.0.1:{}", port))?
        .run()
        .await
}
