use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{
    mpsc, 
    RwLock,
};
use axum::{
    Form, Router, extract::{Query, State, ws::{WebSocket, WebSocketUpgrade}}, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use deadpool_postgres::{Config, Runtime};
use serde::Deserialize;
use msggram::*;
use tower_http::services::ServeDir;

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
        return Err(ServerError::UserDataError(UserDataError::InvalidCredentials));
    };

    let user_id: uuid::Uuid = row.get("id");
    let nickname: String = row.get("nickname");
    let stored_password: String = row.get("password");

    if data.password != stored_password {
        return Err(ServerError::UserDataError(UserDataError::InvalidCredentials));
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

//adding new user
#[derive(Deserialize, Debug)]
struct AddContactMessage{
    message: Option<String>,
}
async fn loading_add_contact(Query(query): Query<AddContactMessage>) -> Result<Html<String>, ServerError>{
    let mut page = tokio::fs::read_to_string("pages/add_contact.html").await?; 
    if let Some(message) = query.message {
        let html = match message.as_str() {
            "error_finding_user" => "<h1>Error finding that user</h1>",
            "already_in_contacts" => "<h1>User already in contacts</h1>",
            "user_added" => "<h1>User added successfully</h1>",
            _ => "", 
        };
        page = page.replace("Type a username", html);
    } 

    Ok(Html(page))
}
async fn add_contact_handler(
        State(state): State<AppState>, 
        jar: CookieJar,
        Form(data): Form<AddContact>
    )-> Result<Redirect, ServerError> {

    let client = state.db.get().await?;
   
    if let Some(value) = get_cookie(&jar, "session_id"){
        let session_id: uuid::Uuid = value.parse()
            .map_err(|_| ServerError::HandleSocketError(
                HandleSocketError::ParsingCookie
            ))?;
        let row = client.query_opt("
                SELECT user_id FROM sessions 
                WHERE sessions.session_id = $1 AND expires_at > NOW() 
            ", &[&session_id]).await?;
        let user_id: uuid::Uuid = match row {
            Some(row) => row.get("user_id"),
            None => {
                return Ok(Redirect::to("/add_contact?message=error_finding_user"));
            } 
        };
        let result: Result<Redirect, ServerError> = match client.query_opt(
           "SELECT id FROM users WHERE users.nickname = $1", 
            &[&data.nickname]
        ).await{
            Ok(Some(row)) => {
                let contacts_id: uuid::Uuid = row.get("id");
                match client.execute(
                    "INSERT INTO contacts(user_id, contacts_id) 
                    VALUES ($1, $2)",
                    &[&user_id, &contacts_id]
                ).await {
                    Ok(_) => {
                        Ok(Redirect::to("/add_contact?message=user_added"))
                    },
                    Err(_) => {
                        Ok(Redirect::to("/add_contact?message=already_in_contacts"))
                    }
                }
            }
            Ok(None) => {
                Ok(Redirect::to("/add_contact?message=error_finding_user"))
            }
            Err(e) => return Err(ServerError::TokioDb(e)),
        };
        result
    } 
    else{
        return Ok(Redirect::to("/login"));
    }
}

//websocket and msg sending handling
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_socket(socket, state, jar).await {
            eprintln!("Websocket error: {e}");
        }
    })
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    jar: CookieJar,
) -> Result<(), ServerError> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let session_id: uuid::Uuid = match get_cookie(&jar, "session_id"){
        Some(value) => { match value.parse() {
            Ok(value) => value,
            Err(_) => return Err(ServerError::HandleSocketError(HandleSocketError::ParsingCookie)),
        } }
        None => return Err(ServerError::HandleSocketError(HandleSocketError::RetrievingCookie)),
    }; 

    let client = state.db.get().await?;
    let row = client.query_opt(
       "SELECT user_id FROM sessions 
        INNER JOIN users ON users.id = sessions.user_id
        WHERE sessions.session_id = $1 AND sessions.expires_at > NOW()", 
        &[&session_id]
    ).await?;
   
    let user_id: uuid::Uuid = match row {
        Some(row) => row.get("user_id"),
        None => return Err(ServerError::HandleSocketError(HandleSocketError::UserNotFound)),
    };
  
    println!("{:?}", user_id);

    let mut users = state.users.write().await;
    users.insert(user_id, tx);
    drop(users);
    loop{
        tokio::select! {
            //receiving msg from socket 
            Some(msg) = rx.recv() => {
                let json = serde_json::to_string(&msg)?;

                socket
                    .send(axum::extract::ws::Message::Text(json.into()))
                    .await?;
            }

            //writing a msg to socket 
            Some(Ok(ws_msg)) = socket.recv() => {
                if let axum::extract::ws::Message::Text(text) = ws_msg {
                    let msg: Message = serde_json::from_str(&text)?;

                    client.execute(
                        "INSERT INTO messages(sender_id, requested_id, contents) 
                        VALUES ($1, $2, $3)",
                        &[&user_id, &msg.receiver_id, &msg.contents]
                    ).await?;

                    let users = state.users.read().await;

                    if let Some(receiver_tx) = users.get(&msg.receiver_id){
                        let _ = receiver_tx.send(msg);
                    }
                }
            }
            else => {
                break;
            }
        }
    }   

    Ok(())
}

fn create_app(state: AppState) -> Router{
    Router::new()
        .route("/", get(home_handler))
        .route("/register", get(loading_register).post(register_handler))
        .route("/login", post(login_handler))
        .route("/add_contact", get(loading_add_contact).post(add_contact_handler))
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
