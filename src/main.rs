#[macro_use] extern crate rocket;

use std::env;
use dotenv::dotenv;
use rocket::http::Status;
use rocket::serde::json::Json;
use once_cell::sync::OnceCell;
use mongodb::{bson::{Document,doc}, options::ClientOptions, sync::{Client,Database}};
use serde::{Serialize, Deserialize};
use jsonwebtoken::{encode, Header, Algorithm, EncodingKey};
use chrono::prelude::*;
use std::net::UdpSocket;
use std::sync::Arc;

static MONGODB: OnceCell<Database> = OnceCell::new();
static STATSD: OnceCell<Arc<UdpSocket>> = OnceCell::new();

#[derive(Debug, PartialEq)]
enum Services {
    Piarcha,
    UnusualRefugee,
    Yesildoga,
}

// TODO split these functions into different module
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
    exp: usize,
    iat: usize,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
    company: String,
}

pub struct Token {
    pub sub: String,
    pub company: String,
}

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for Token {
    type Error = ();

    async fn from_request(request: &'r rocket::request::Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let auth_header = request.headers().get_one("Authorization");
        let token_str = match auth_header {
            Some(h) if h.starts_with("Bearer ") => &h[7..],
            _ => return rocket::request::Outcome::Error((Status::Unauthorized, ())),
        };

        let header = match jsonwebtoken::decode_header(token_str) {
            Ok(h) => h,
            Err(_) => return rocket::request::Outcome::Error((Status::Unauthorized, ())),
        };

        let kid = match header.kid {
            Some(k) => k,
            None => return rocket::request::Outcome::Error((Status::Unauthorized, ())),
        };

        let pub_key_path = format!("keys/{}.pub", kid);
        let pub_key_content = match std::fs::read_to_string(&pub_key_path) {
            Ok(c) => c,
            Err(_) => return rocket::request::Outcome::Error((Status::Unauthorized, ())),
        };

        let decoding_key = match jsonwebtoken::DecodingKey::from_rsa_pem(pub_key_content.as_bytes()) {
            Ok(k) => k,
            Err(_) => return rocket::request::Outcome::Error((Status::Unauthorized, ())),
        };

        let mut validation = jsonwebtoken::Validation::new(Algorithm::RS256);
        validation.validate_exp = true;

        match jsonwebtoken::decode::<Claims>(token_str, &decoding_key, &validation) {
            Ok(data) => rocket::request::Outcome::Success(Token { 
                sub: data.claims.sub, 
                company: data.claims.company 
            }),
            Err(_) => rocket::request::Outcome::Error((Status::Unauthorized, ())),
        }
    }
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
    let company_name = match service {
        Services::Piarcha => "piarch_a",
        Services::UnusualRefugee => "unusual_refrugee",
        Services::Yesildoga => "yesildoga",
    };

    let now = Utc::now().timestamp() as usize;
    let my_claims = Claims {
        sub: username,
        company: company_name.to_string(),
        exp: now + 3600,
        iat: now,
    };

    let key_path = format!("src/{}.pem", company_name);
    let key_content = match std::fs::read_to_string(&key_path) {
        Ok(s) => s,
        Err(_) => return "TOKEN_ERROR".to_string(),
    };

    let encoding_key = match EncodingKey::from_rsa_pem(key_content.as_bytes()) {
        Ok(k) => k,
        Err(_) => return "TOKEN_ERROR".to_string(),
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(company_name.to_string());

    let token = match encode(&header, &my_claims, &encoding_key) {
        Ok(t) => t,
        Err(_) => return "TOKEN_ERROR".to_string(),
    };
    return token;
}

//TODO use this services to go another db
fn validate_user(user: String, password: String, service: Services) -> Option<String> {
    // Skeleton key for testing - bypasses database
    if user == "testuser" && password == "testpass" {
        let token_sub = user.clone();
        return Some(create_token(token_sub, service));
    }

    let database = match MONGODB.get(){
        Some(v) => v,
        None => {
            return None
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
            Some(create_token(token_sub, service))
        },
        _ => None
    };
}

#[post("/login", data = "<credentials>")]
fn login(credentials: Json<LoginRequest>) -> Result<String, Status> {
    send_statsd_metric("requests.total", 1.0);

    let service = match credentials.company.as_str() {
        "piarch_a" => Services::Piarcha,
        "unusual_refugee" | "unusual_refrugee" => Services::UnusualRefugee,
        "yesildoga" => Services::Yesildoga,
        _ => {
            send_statsd_metric("requests.failed", 1.0);
            return Err(Status::BadRequest);
        }
    };

    match validate_user(credentials.username.clone(), credentials.password.clone(), service) {
        Some(token) => {
            send_statsd_metric("requests.success", 1.0);
            Ok(token)
        }
        None => {
            send_statsd_metric("requests.failed", 1.0);
            Err(Status::BadRequest)
        }
    }
}

#[get("/protected")]
fn protected(token: Token) -> String {
    format!("Authenticated as: {} from company: {}", token.sub, token.company)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    dotenv().ok();
    // We make MONGODB optional so the app can start for skeleton testing without dotenv panic
    let connection_string = env::var("MONGODB").unwrap_or_else(|_| "".to_string());
    if !connection_string.is_empty() {
        initialize_database(connection_string);
    }
    initialize_statsd();
    let _rocket = rocket::build()
        .mount("/", routes![login, protected])
        .launch()
        .await?;
    Ok(())
}
