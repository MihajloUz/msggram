use axum::extract::State;
use axum::{Form, Router, response::{Html, IntoResponse, Redirect}, routing::{get, post}};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use tokio::sync::mpsc;
use deadpool_postgres::{Config, Runtime};
use serde::Deserialize;
use msggram::*;

//async fn login_post(Form(data): Form<Login>) -> Redirect{
    //println!("email: {}", data.email);
    //println!("password: {}", data.password);
   // 
    //Redirect::to("/")
//}

//async fn check() -> Html<String> {
    //match tokio::fs::read_to_string("pages/home.html").await{
        //Ok(html) => Html(html),
        //Err(err) => {
            //println!("Erorr reading pages/home.html");
            //Html("<h1>Error reading the pages/home.html</h1>".to_string())
        //}
    //}
//}

fn get_cookie(jar: &CookieJar, cookie_name: &str) -> Option<String> {
    jar.get(cookie_name).map(|cookie| cookie.value().to_string())
}

async fn home_handler(jar: CookieJar) -> Html<String> {
    if let Some(value) = get_cookie(&jar, "session_id"){
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

async fn login_handler(State(state): State<deadpool_postgres::Pool>, jar: CookieJar, Form(data): Form<Login>) -> Result<(CookieJar, Redirect), ServerError> {
    let client = state.get().await?;
    let row = client.query_opt(
        "SELECT id, nickname, password FROM users WHERE email = $1", 
        &[&data.email]
    ).await?;

    let Some(row) = row else{
        return Err(ServerError::UserCredentialsError(UserDataError::InvalidCredentials));
    };

    let user_id: uuid::Uuid = row.get("id");
    let nickname: String = row.get("nickname");
    let stored_password: String = row.get("password");

    if data.password != stored_password {
        return Err(ServerError::UserCredentialsError(UserDataError::InvalidCredentials));
    }


    let row = client.query_one(
        "INSERT INTO sessions (user_id) VALUES ($1) RETURNING session_id",
        &[&user_id]
    ).await?;

    let session_id: uuid::Uuid = row.get("session_id");

    let cookie = Cookie::build(("session_id", session_id.to_string()))
        .path("/")
        .http_only(true)
        .build();

    Ok((jar.add(cookie), Redirect::to("/")))
}

async fn loading_register() -> Html<String>{
    match tokio::fs::read_to_string("pages/register.html").await{
        Ok(html) => Html(html),
        Err(err) => {
            println!("Erorr reading pages/home.html");
            Html("<h1>Error reading the pages/home.html</h1>".to_string())
        }
    }
}

async fn register_handler(State(state): State<deadpool_postgres::Pool>, Form(data): Form<Register>) -> Result<Redirect, ServerError>{
    checking_user_data(data.clone())?;
    let client = state.get().await?;

    client.execute(
        "INSERT INTO users (nickname, email, password) VALUES ($1, $2, $3)",
        &[&data.nickname, &data.email, &data.password]
    ).await?;

    Ok(Redirect::to("/"))
}

fn create_app(pool: deadpool_postgres::Pool) -> Router{
    Router::new()
        .route("/", get(home_handler))
        .route("/register", get(loading_register).post(register_handler))
        .route("/login", post(login_handler))
        .with_state(pool)
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


    let pool = create_pool()?;

    setting_up_db(&pool).await?;

    let app = create_app(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
