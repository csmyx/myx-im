use dotenv::dotenv;
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub db_url: String,
    pub jwt_secret: String,
    pub jwt_expire: i64,
}

impl Config {
    pub fn load() -> Self {
        dotenv().ok();

        Self {
            db_url: env::var("DATABASE_URL").unwrap(),
            jwt_secret: env::var("JWT_SECRET").unwrap(),
            jwt_expire: env::var("JWT_EXPIRE").unwrap().parse().unwrap(),
        }
    }
}
