use tokio::sync::mpsc; 
use axum::{
    extract::{State, ws::{WebSocket, WebSocketUpgrade}}, response::IntoResponse
};
use axum_extra::extract::cookie::CookieJar;
use msggram::*;
use crate::auth::get_cookie; 

pub async fn ws_handler(
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

pub async fn handle_socket(
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
                        "INSERT INTO messages(sender_id, received_id, contents) 
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


