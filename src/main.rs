

#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;

use std::env;
use dotenv::dotenv;
use rocket::Outcome;
use rocket::http::Status;
use rocket::request::{self, Request, FromRequest};
use once_cell::sync::OnceCell;
use mongodb::{bson::{Document,doc}, options::ClientOptions, sync::{Client,Database}};
use serde::{Serialize, Deserialize};
use jsonwebtoken::{encode, Header, Algorithm, EncodingKey};
use chrono::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH, Duration};

static MONGODB: OnceCell<Database> = OnceCell::new();

struct Token(String);

#[derive(Debug)]
enum TokenError {
    BadCount,
    Invalid,
}

#[derive(Debug)]
enum Services {
    Piarcha,
    Unusual_Refugee,
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
    let my_claims= Claims{sub:credential_sub,company: company_name.parse().unwrap(), exp: since_the_epoch};

    let pem_file = if service == Services::Piarcha {
        include_bytes!("./piarch_a.pem")
    } else {
        include_bytes!("./unusual_refugee.pem")
    };

    // TODO remove unwraps here
    let token = encode(&Header::new(Algorithm::RS256), &my_claims, &EncodingKey::from_rsa_pem(include_bytes!(pem_file)).unwrap()).unwrap();
    print!("{}",token.clone());
    return token;
}

//TODO use this services to go another db
fn validate_user(user: String, password: String, service: Services) -> Result<String, TokenError> {
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
    let method = authorize_header.next().unwrap();
    let encoded_user_pass = authorize_header.next().unwrap();

    let mut user_info_fields = encoded_user_pass.split(":");
    let user_info_length = user_info_fields.clone().count();
    let user_info_size: i32 = 2;

    if user_info_length as i32 != user_info_size {
        return Err(TokenError::BadCount)
    }
    let user = user_info_fields.next().unwrap();
    let password = user_info_fields.next().unwrap();

    let normalized_user = user.to_lowercase();
    let normalized_password = password.to_lowercase();
    let result = validate_user(normalized_user,normalized_password);
    Ok(result.unwrap())
}

impl<'a, 'r> FromRequest<'a, 'r> for Token {
    type Error = TokenError;

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, Self::Error> {
        let credentials = request.headers().get_one("authorize");
        match credentials {
            Some(credentials) => {
              let validated_token = evaluate_credentials(credentials);
                match validated_token{
                    Ok(result) => Outcome::Success(Token(result)),
                    Err(e)=> Outcome::Failure((Status::BadRequest, e))
                }
            },
            None => Outcome::Failure((Status::BadRequest, TokenError::Invalid))
        }
    }
}

#[get("/login")]
fn login(authorize: Token)-> String {
    authorize.0
}

fn main() {
    dotenv().ok();
    let connection_string = match env::var("MONGODB") {
        Ok(v) => v,
        Err(e) => panic!("MONGODB is not set ")
    };
    initialize_database(connection_string);
    rocket::ignite().mount("/", routes![login]).launch();
}
