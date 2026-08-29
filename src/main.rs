use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{
    mpsc, 
    RwLock,
};
use axum::{
    Form, 
    Router, 
    response::{Html, 
        IntoResponse, 
        Redirect
    }, 
    routing::{get, post},
    extract::{State, ws::{WebSocket, WebSocketUpgrade}},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use deadpool_postgres::{Config, Runtime};
use serde::Deserialize;
use msggram::*;
use tower_http::services::ServeDir;
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


//cookie extraction
fn get_cookie(jar: &CookieJar, cookie_name: &str) -> Option<String> {
    jar.get(cookie_name).map(|cookie| cookie.value().to_string())
}
//home
async fn home_handler(jar: CookieJar) -> Result<Html<String>, ServerError> {
    if let Some(value) = get_cookie(&jar, "session_id"){
        Ok(Html(tokio::fs::read_to_string("pages/home.html").await?))
    } 
    else{
        Ok(Html(tokio::fs::read_to_string("pages/login.html").await?))
    }
}

async fn login_handler(State(state): State<AppState>, jar: CookieJar, Form(data): Form<Login>) -> Result<(CookieJar, Redirect), ServerError> {
    let client = state.db.get().await?;
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

async fn loading_register() -> Result<Html<String>, ServerError>{Ok(Html(tokio::fs::read_to_string("pages/register.html").await?))}

async fn register_handler(State(state): State<AppState>, Form(data): Form<Register>) -> Result<Redirect, ServerError>{
    checking_user_data(data.clone())?;
    let client = state.db.get().await?;

    client.execute(
        "INSERT INTO users (nickname, email, password) VALUES ($1, $2, $3)",
        &[&data.nickname, &data.email, &data.password]
    ).await?;

    Ok(Redirect::to("/"))
}

//websocket and msg sending handling
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_socket(socket, state, jar).await {
            eprintln!("websocket error: {e}");
        }
    })
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    jar: CookieJar,
) -> Result<(), ServerError> {
    let (rx, mut tx) = mpsc::unbounded_channel::<Message>();

    let session_id: uuid::Uuid = match get_cookie(&jar, "session_id"){
        Some(value) => value.parse().unwrap(),
        None => return Err(ServerError::UserCredentialsError(UserDataError::InvalidCredentials)),
    }; 

    let client = state.db.get().await?;
    let row = client.query_opt(
        "SELECT user_id FROM sessions 
        INNER JOIN users ON users.id = sessions.user_id
        WHERE sessions.session_id = $1", 
        &[&session_id]
    ).await?;
   
    let user_id: uuid::Uuid = match row {
        Some(row) => row.get("user_id"),
        None => {
            return Err(ServerError::UserCredentialsError(
                UserDataError::InvalidCredentials
            ));
        }
    };
   
    let mut users = state.users.write().await;
    users.insert(user_id, tx);

    Ok(())
}

fn create_app(state: AppState) -> Router{
    Router::new()
        .route("/", get(home_handler))
        .route("/register", get(loading_register).post(register_handler))
        .route("/login", post(login_handler))
        .route("/ws", get(ws_handler))
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
    let state = AppState {
        db: pool,
        users: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = create_app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
