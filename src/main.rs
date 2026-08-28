use axum::{Form, Router, response::{Html, IntoResponse, Redirect}, routing::{get, post}};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use tokio::sync::mpsc;
use deadpool_postgres::{Config, Runtime};
use serde::Deserialize;

#[derive(Deserialize)]
struct Login{
    email: String,
    password: String,
}

//async fn login_post(Form(data): Form<Login>) -> Redirect{
    //println!("email: {}", data.email);
    //println!("password: {}", data.password);
   // 
    //Redirect::to("/")
//}

async fn check() -> Html<String> {
    match tokio::fs::read_to_string("pages/home.html").await{
        Ok(html) => Html(html),
        Err(err) => {
            println!("Erorr reading pages/home.html");
            Html("<h1>Error reading the pages/home.html</h1>".to_string())
        }
    }
}

fn get_cookie(jar: &CookieJar) -> Option<String> {
    jar.get("username")
        .map(|cookie| cookie.value().to_string())
}

async fn home_handler(jar: CookieJar) -> Html<String> {
    if let Some(value) = get_cookie(&jar){
        match tokio::fs::read_to_string("pages/home.html").await{
            Ok(html) => Html(format!("{} \r\n\r\n {}", html, value.to_string())),
            Err(err) => {
                println!("Erorr reading pages/home.html");
                Html("<h1>Error reading the pages/home.html</h1>".to_string())
            }
        }
    } else{
       match tokio::fs::read_to_string("pages/login.html").await{
            Ok(html) => Html(html),
            Err(err) => {
                println!("Erorr reading pages/home.html");
                Html("<h1>Error reading the pages/home.html</h1>".to_string())
            }
       }
    }

}

async fn login_handler(jar: CookieJar, Form(data): Form<Login>) -> (CookieJar, Redirect) {
    let username = format!("{}={}", data.email, data.password);
    let cookie = Cookie::build(("username", username))
        .path("/")
        .http_only(true)
        .build();

    let jar = jar.add(cookie);

    (jar, Redirect::to("/"))
}

fn create_app(pool: deadpool_postgres::Pool) -> Router{
    Router::new()
        .route("/", get(home_handler))
        .route("/login", post(login_handler))
        .with_state(pool)
}

fn create_pool() -> Result<deadpool_postgres::Pool, Box<dyn std::error::Error>>{
    let mut cfg = deadpool_postgres::Config::new();

    cfg.host = Some(std::env::var("POSTGRES_HOST")?);
    cfg.user = Some(std::env::var("POSTGRES_USER")?);
    cfg.password = Some(std::env::var("POSTGRES_PASSWORD")?);
    cfg.dbname = Some(std::env::var("POSTGRES_DB")?);

    Ok(cfg.create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)?)
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
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
    let (client, connection) = tokio_postgres::connect(
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

    client.batch_execute(" 
        CREATE EXTENSION IF NOT EXISTS pgcrypto; 
        CREATE TABLE IF NOT EXISTS users ( 
            id      UUID PRIMARY KEY DEFAULT gen_random_uuid(), 
            first_name    TEXT NOT NULL, 
            last_name   TEXT NOT NULL, 
            email   TEXT NOT NULL UNIQUE, 
            password TEXT NOT NULL 
        ); 
    ").await?;

    let pool = create_pool()?;
    let app = create_app(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}

