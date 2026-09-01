use std::collections::HashMap;
use axum::{
    extract::State, response::Html 
};
use axum_extra::extract::cookie::CookieJar;
use msggram::*;
use crate::auth::get_cookie; 

pub async fn home_handler(jar: CookieJar, State(state): State<AppState>) -> Result<Html<String>, ServerError> {
    if let Some(value) = get_cookie(&jar, "session_id"){
        let mut page = tokio::fs::read_to_string("pages/home.html").await?; 
        
        let session_id: uuid::Uuid = value.parse()
            .map_err(|_| ServerError::HandleSocketError(HandleSocketError::ParsingCookie))?;

        let client = state.db.get().await?;
        
        let rows = client.query_opt(
            "SELECT user_id FROM sessions
            WHERE session_id = $1 AND expires_at > NOW()", &[&session_id]
        ).await?;

        let Some(row) = rows else{
            return Ok(Html(tokio::fs::read_to_string("pages/login.html").await?))
        };
        let user_id: uuid::Uuid = row.get("user_id");

        let rows = client.query(
            "SELECT contacts.contacts_id, users.nickname FROM contacts 
            INNER JOIN users ON users.id = contacts.contacts_id
            WHERE contacts.user_id = $1
            ", 
            &[&user_id]
        ).await?;
        
        let mut contacts: HashMap<uuid::Uuid, String> = HashMap::new();
        for row in rows{
            let contacts_id = row.get("contacts_id");
            let nickname = row.get("nickname");
            contacts.insert(contacts_id, nickname);
        }

        let html = {
            if contacts.is_empty(){
                //no contacts available
                String::from("")
            } 
            else{
                let mut buffer_string: String = String::new();
                for (id, nickname) in &contacts{
                    let button = format!("<button class='user' data-user-id='{id}'>{nickname}</button>"); 
                    buffer_string.push_str(button.as_str());
                }
                buffer_string
            }

        };
        println!("{}", html);
        page = page.replace("Contacts", html.as_str());
        Ok(Html(page))
    } 
    else{
        Ok(Html(tokio::fs::read_to_string("pages/login.html").await?))
    }
}
