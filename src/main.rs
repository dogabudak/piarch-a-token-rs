

#[macro_use] extern crate rocket;

use std::env;
use dotenv::dotenv;
use rocket::{Request};
use rocket::request::{Outcome, FromRequest};
use rocket::http::Status;
use once_cell::sync::OnceCell;
use mongodb::{bson::{Document,doc}, options::ClientOptions, sync::{Client,Database}};
use serde::{Serialize, Deserialize};
use jsonwebtoken::{encode, Header, Algorithm, EncodingKey};
use chrono::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::net::UdpSocket;
use std::sync::Arc;

static MONGODB: OnceCell<Database> = OnceCell::new();
static STATSD: OnceCell<Arc<UdpSocket>> = OnceCell::new();

struct Token(String);

#[derive(Debug)]
enum TokenError {
    BadCount,
    Invalid,
}

#[derive(Debug, PartialEq)]
enum Services {
    Piarcha,
    UnusualRefugee,
}

// TODO split these functions into different module
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
    exp: u128,
}

pub fn initialize_database(connection_string: String) {
    if MONGODB.get().is_some() {
        return;
    }
        if let Ok(client_options) =  ClientOptions::parse(connection_string) {
            if let Ok(client) = Client::with_options(client_options) {
                let _ = MONGODB.set(client.database("piarka"));
            }
        }
}

pub fn initialize_statsd() {
    if STATSD.get().is_some() {
        return;
    }
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = STATSD.set(Arc::new(socket));
    }
}

fn send_statsd_metric(metric: &str, value: f64) {
    if let Some(socket) = STATSD.get() {
        let message = format!("piarch_token_service.{}:{}|c", metric, value);
        let _ = socket.send_to(message.as_bytes(), "127.0.0.1:8125");
    }
}

fn create_token(username: String, service: Services) -> String {
    let company_name = if service == Services::Piarcha {
        "piarch_a"
    } else {
        "unusual_refugee"
    };
    let credential_sub = username.clone();
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH).unwrap_or(Duration::default()).as_millis();
    let my_claims= Claims{sub:credential_sub,company: company_name.to_string(), exp: since_the_epoch};

    let pem_bytes: &[u8] = if service == Services::Piarcha {
        include_bytes!("./piarch_a.pem")
    } else {
        include_bytes!("./unusual_refrugee.pem")
    };

    // TODO remove unwraps here
    let encoding_key = match EncodingKey::from_rsa_pem(pem_bytes) {
        Ok(key) => key,
        Err(_) => return "TOKEN_ERROR".to_string()
    };
    let token = match encode(&Header::new(Algorithm::RS256), &my_claims, &encoding_key) {
        Ok(t) => t,
        Err(_) => return "TOKEN_ERROR".to_string()
    };
    print!("{}",token.clone());
    return token;
}

//TODO use this services to go another db
fn validate_user(user: String, password: String, service: Services) -> Result<String, TokenError> {
    // Skeleton key for testing - bypasses database
    if user == "testuser" && password == "testpass" {
        let token_sub = user.clone();
        return Ok(create_token(token_sub, service));
    }
    
    let database = match MONGODB.get(){
        Some(v) => v,
        None => {
            // TODO this should be different error
            return Err(TokenError::Invalid)
        }
    };
    let username = user.clone();
    let collection = database.collection::<Document>("users");
    let filter = doc! {"username": username};
    let utc: DateTime<Utc> = Utc::now();
    let document = match collection.find_one_and_update(filter,doc!{"$set" : {"lastLogin":utc.to_string() } },None) {
        Ok(v) => v,
        Err(_) => None
    };
    return match document {
        Some(_) => {
            let token_sub = user.clone();
            Ok(create_token(token_sub, service))
        },
        _ => Err(TokenError::Invalid)
    };
}

fn evaluate_credentials(credentials: &str) -> Result<String, TokenError> {

    let mut authorize_header =  credentials.split( " ");
    let header_count = authorize_header.clone().count();
    let header_size: i32 = 2;

    if header_count as i32 != header_size {
        return Err(TokenError::BadCount)
    }
    // TODO remove unwraps here
    let _method = match authorize_header.next() {
        Some(m) => m,
        None => return Err(TokenError::BadCount)
    };
    let encoded_user_pass = match authorize_header.next() {
        Some(pass) => pass,
        None => return Err(TokenError::BadCount)
    };

    let mut user_info_fields = encoded_user_pass.split(":");
    let user_info_length = user_info_fields.clone().count();
    let user_info_size: i32 = 2;

    if user_info_length as i32 != user_info_size {
        return Err(TokenError::BadCount)
    }
    let user = match user_info_fields.next() {
        Some(u) => u,
        None => return Err(TokenError::BadCount)
    };
    let password = match user_info_fields.next() {
        Some(p) => p,
        None => return Err(TokenError::BadCount)
    };

    let normalized_user = user.to_lowercase();
    let normalized_password = password.to_lowercase();
    let result = validate_user(normalized_user, normalized_password, Services::Piarcha);
    result
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Token {
    type Error = TokenError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        send_statsd_metric("requests.total", 1.0);
        
        let credentials = request.headers().get_one("authorize");
        match credentials {
            Some(credentials) => {
              let validated_token = evaluate_credentials(credentials);
                match validated_token{
                    Ok(result) => {
                        send_statsd_metric("requests.success", 1.0);
                        Outcome::Success(Token(result))
                    },
                    Err(e)=> {
                        send_statsd_metric("requests.failed", 1.0);
                        Outcome::Error((Status::BadRequest, e))
                    }
                }
            },
            None => {
                send_statsd_metric("requests.unauthorized", 1.0);
                Outcome::Error((Status::BadRequest, TokenError::Invalid))
            }
        }
    }
}

#[get("/login")]
fn login(authorize: Token)-> String {
    authorize.0
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    dotenv().ok();
    let connection_string = match env::var("MONGODB") {
        Ok(v) => v,
        Err(_e) => panic!("MONGODB is not set ")
    };
    initialize_database(connection_string);
    initialize_statsd();
    let _rocket = rocket::build()
        .mount("/", routes![login])
        .launch()
        .await?;
    Ok(())
}
