use axum::{
    Form, extract::{Query, State}, response::{Html, Redirect} 
};
use axum_extra::extract::cookie::CookieJar;
use msggram::*;
use crate::auth::get_cookie; 
use crate::models::AddContactMessage;

pub async fn loading_add_contact(Query(query): Query<AddContactMessage>) -> Result<Html<String>, ServerError>{
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
pub async fn add_contact_handler(
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


