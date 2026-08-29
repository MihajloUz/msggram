use axum::{http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use uuid::Uuid;
use std::{fmt, env, collections::HashMap, sync::Arc};
use deadpool_postgres::{
    Config, 
    Runtime, 
};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug)]
pub enum UserDataError{
    NoAtSign,
    EmailFormat,
    PasswordFormat,
    InvalidCredentials,
}

impl fmt::Display for UserDataError{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
        match self{
            UserDataError::NoAtSign=> {
                write!(f, "Abscent @ sign in email")
            },
            UserDataError::EmailFormat=> {
                write!(f, "Wrong email format")
            },
            UserDataError::PasswordFormat=> {
                write!(f, "Wrong password format")
            },
            UserDataError::InvalidCredentials => {
                write!(f, "Invalid users credentials")
            },
        } 
    }
}

impl std::error::Error for UserDataError {}

pub fn checking_user_data(data: Register) -> Result<(), UserDataError>{
    let email = data.email.trim();

    let Some((local, domain)) = email.split_once('@') else {
        return Err(UserDataError::NoAtSign);
    };

    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || email.contains(' '){
            return Err(UserDataError::EmailFormat);
    }

    let password = data.password.clone();
    if !(password.chars().any(|c| c.is_alphabetic()) && 
    password.chars().any(|c| c.is_numeric()) && 
            password.chars().any(|c| c.is_uppercase())){
        return Err(UserDataError::PasswordFormat);
    }

    Ok(())
}

#[derive(Debug)]
pub enum ServerError{
    TokioDb(tokio_postgres::Error),
    DeadpoolDb(deadpool_postgres::PoolError),
    PoolCreation(deadpool_postgres::CreatePoolError),
    Io(std::io::Error),
    Env(env::VarError),
    
    UserCredentialsError(UserDataError),
}

impl fmt::Display for ServerError{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result{
        match self{
            ServerError::TokioDb(e) => {
                write!(f, "tokio_postgres database erorr: {}", e)
            }
            ServerError::DeadpoolDb(e) => {
                write!(f, "deadpool_postgres database erorr: {}", e)
            }
            ServerError::PoolCreation(e) => {
                write!(f, "Failed to create database: {}", e)
            }
            ServerError::Io(e) => {
                write!(f, "I/O erorr: {}", e)
            }
            ServerError::Env(e) => {
                write!(f, ".env erorr: {}", e)
            }
            ServerError::UserCredentialsError(e) => {
                write!(f, "{}", e)
            }
        }
    }
}
impl IntoResponse for ServerError{
    fn into_response(self) -> axum::response::Response {
        let status = match self{
            ServerError::TokioDb(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::DeadpoolDb(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::PoolCreation(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::Env(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::UserCredentialsError(_) => StatusCode::BAD_REQUEST,
};

        (status, self.to_string()).into_response()
    }
}

impl From<tokio_postgres::Error> for ServerError{
    fn from(e: tokio_postgres::Error) -> Self{
        ServerError::TokioDb(e)
    }
} 

impl From<deadpool_postgres::PoolError> for ServerError{
    fn from(e: deadpool_postgres::PoolError) -> Self {
        ServerError::DeadpoolDb(e)
    }
}

impl From<deadpool_postgres::CreatePoolError> for ServerError{
    fn from(e: deadpool_postgres::CreatePoolError) -> Self {
        ServerError::PoolCreation(e) 
    }
}

impl From<std::io::Error> for ServerError{
    fn from(e: std::io::Error) -> Self{
        ServerError::Io(e)
    }
} 

impl From<env::VarError> for ServerError{
    fn from(e: env::VarError) -> Self {
        ServerError::Env(e) 
    }
}

impl From<UserDataError> for ServerError{
    fn from(e: UserDataError) -> Self{
        ServerError::UserCredentialsError(e)
    }
}

impl std::error::Error for ServerError{}

#[derive(Deserialize, Clone)]
pub struct Register{
    pub nickname: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Clone)]
pub struct Login{
    pub email: String,
    pub password: String,
}


//db
pub fn create_pool() -> Result<deadpool_postgres::Pool, ServerError>{
    let mut cfg = Config::new();

    cfg.host = Some(std::env::var("POSTGRES_HOST")?);
    cfg.user = Some(std::env::var("POSTGRES_USER")?);
    cfg.password = Some(std::env::var("POSTGRES_PASSWORD")?);
    cfg.dbname = Some(std::env::var("POSTGRES_DB")?);

    Ok(cfg.create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)?)
}

pub async fn setting_up_db(pool: &deadpool_postgres::Pool) -> Result<(), deadpool_postgres::PoolError>{
    let client = pool.get().await?;

    client.batch_execute(
        "
        CREATE EXTENSION IF NOT EXISTS pgcrypto;
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            nickname TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS messages (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            sender_id UUID NOT NULL REFERENCES users(id),
            request_id UUID NOT NULL REFERENCES users(id),
            contents TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS sessions(
            session_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id UUID NOT NULL REFERENCES users(id),
            expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INVERVAL '7 days'
        );
        "
    ).await?;

    Ok(())
}
//App state and msg receiving
#[derive(Deserialize)]
pub struct Message{
    receiver_id: Uuid,
    session_id: Uuid,
    contents: String,
}

#[derive(Clone)]
pub struct AppState{
    pub db: deadpool_postgres::Pool,
    pub users: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
}
