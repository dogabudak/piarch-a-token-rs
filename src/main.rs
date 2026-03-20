


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
use rocket_jwt_auth::Token;

static MONGODB: OnceCell<Database> = OnceCell::new();
static STATSD: OnceCell<Arc<UdpSocket>> = OnceCell::new();

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
    exp: usize,
    iat: usize,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
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

    let now = Utc::now().timestamp() as usize;
    let my_claims = Claims {
        sub: username,
        company: company_name.to_string(),
        exp: now + 3600,
        iat: now,
    };

    let secret = match env::var("JWT_SECRET") {
        Ok(s) => s,
        Err(_) => return "TOKEN_ERROR".to_string(),
    };

    let token = match encode(
        &Header::new(Algorithm::HS256),
        &my_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    ) {
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
    match validate_user(credentials.username.clone(), credentials.password.clone(), Services::Piarcha) {
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
    format!("Authenticated as: {}", token.sub)
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
        .mount("/", routes![login, protected])
        .launch()
        .await?;
    Ok(())
}
