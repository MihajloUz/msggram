use axum::{
    Form, extract::{Query, State}, response::{Html, Redirect} 
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use msggram::*;
use crate::models::{LoginMessage, RegisterMessage};

pub fn get_cookie(jar: &CookieJar, cookie_name: &str) -> Option<String> {
    jar.get(cookie_name).map(|cookie| cookie.value().to_string())
}

pub async fn loading_login(Query(query): Query<LoginMessage>) -> Result<Html<String>, ServerError> {
    let mut page = tokio::fs::read_to_string("pages/login.html").await?; 
    if let Some(message) = query.message {
        let html = match message.as_str() {
            "no_such_user" => "<h1>No such user</h1>",
            "invalid_password" => "<h1>Invalid password</h1>",
            _ => "", 
        };
        page = page.replace("Fill all of the fields", html);
    } 
    Ok(Html(page))
}
pub async fn login_handler(State(state): State<AppState>, jar: CookieJar, Form(data): Form<Login>) -> 
         Result<(CookieJar, Redirect), ServerError> {
    let client = state.db.get().await?;
    let row = client.query_opt(
        "SELECT id, nickname, password FROM users WHERE email = $1", 
        &[&data.email]
    ).await?;

    let Some(row) = row else{
        return Ok( (jar, Redirect::to("/login?message=no_such_user")) );
    };

    let user_id: uuid::Uuid = row.get("id");
    let stored_password: String = row.get("password");

    if data.password != stored_password {
        return Ok( (jar, Redirect::to("/login?message=invalid_password")) );
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

pub async fn loading_register(Query(query): Query<RegisterMessage>) -> Result<Html<String>, ServerError>{
    let mut page = tokio::fs::read_to_string("pages/register.html").await?;
    if let Some(message) = query.message {
        let html = match message.as_str() {
            "no_at_sign" => "<h1>No @ sign in email</h1>",
            "wrong_email_format" => "<h1>Wrong email format</h1>",
            "wrong_password_format" => "<h1>Wrong password format</h1>",
            _ => "", 
        };
        page = page.replace("Fill all of the blanks", html);
    } 

    Ok(Html(page))
}

pub async fn register_handler(State(state): State<AppState>, Form(data): Form<Register>) -> Result<Redirect, ServerError>{
    match checking_user_data(data.clone()){
        Err(UserDataError::NoAtSign) => return Ok(Redirect::to("/register?message=no_at_sign")),
        Err(UserDataError::EmailFormat) => return Ok(Redirect::to("/register?message=wrong_email_format")),
        Err(UserDataError::PasswordFormat) => return Ok(Redirect::to("/register?message=wrong_password_format")),
        _ => {}, 
    }
    let client = state.db.get().await?;

    client.execute(
        "INSERT INTO users (nickname, email, password) VALUES ($1, $2, $3)",
        &[&data.nickname, &data.email, &data.password]
    ).await?;

    Ok(Redirect::to("/"))
}


