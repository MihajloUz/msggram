use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{
    RwLock,
};
use axum::{
    Router, routing::get,
};
use msggram::*;
use tower_http::services::ServeDir;

mod auth;
mod contacts;
mod home;
mod models;
mod websocket;

fn create_app(state: AppState) -> Router{
    Router::new()
        .route("/", get(home::home_handler))
        .route("/register", get(auth::loading_register).post(auth::register_handler))
        .route("/login", get(auth::loading_login).post(auth::login_handler))
        .route("/add_contact", get(contacts::loading_add_contact).post(contacts::add_contact_handler))
        .route("/ws", get(websocket::ws_handler))
        .nest_service("/img", ServeDir::new("pages/img"))
        .nest_service("/js", ServeDir::new("pages/js"))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<(), ServerError>{
    dotenvy::dotenv().ok();
    let (client, connection) = tokio_postgres::connect(
        format!("host={} user={} password={} dbname={}",
                std::env::var("POSTGRES_HOST")?,
                std::env::var("POSTGRES_USER")?,
                std::env::var("POSTGRES_PASSWORD")?,
                "postgres".to_string()).as_str(), 
            tokio_postgres::NoTls
    ).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await{
            eprintln!("Error connecting to db: {}", e);
        }
    });

    match client.batch_execute(
        &format!("CREATE DATABASE {}", std::env::var("POSTGRES_DB")?)
    ).await{
        Ok(_) => {},
        Err(_) => {println!("Database was already created. Skipping creating another one");}
    } 
    let (_client, connection) = tokio_postgres::connect(
        format!("host={} user={} password={} dbname={}",
                std::env::var("POSTGRES_HOST")?,
                std::env::var("POSTGRES_USER")?,
                std::env::var("POSTGRES_PASSWORD")?,
                std::env::var("POSTGRES_DB")?).as_str(), 
            tokio_postgres::NoTls
    ).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await{
            eprintln!("Error connecting to db: {}", e);
        }
    });

    let pool = create_pool()?;
    setting_up_db(&pool).await?;
    let state = AppState {
        db: pool,
        users: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = create_app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
