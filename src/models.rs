use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginMessage{
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterMessage{
    pub message: Option<String>,
}
#[derive(Deserialize, Debug)]
pub struct AddContactMessage{
    pub message: Option<String>,
}

