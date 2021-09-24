extern crate dotenv;
extern crate tokio;

use rcon::Client;

use regex::Regex;

use log::*;

use tokio::sync::RwLock;
use std::sync::Arc;
use typemap::TypeMap;

struct Implimentor {
    client: Option<Client>,
    regex_engine: Regex,
}

use async_trait::async_trait;

#[async_trait]
impl rcon::RconImpl for Implimentor {
    fn new(_state: Arc<RwLock<TypeMap>>) -> Self {
        let regex =
            String::from("^(") + &vec!["say", "tellraw", "gamemode", "time"].join("|") + r")\s?";

        let re = Regex::new(&regex).expect("failed to parse regex");

        Implimentor {
            client: None,
            regex_engine: re,
        }
    }

    async fn authenticate(&mut self, pass: String, _id: i32) -> bool {
        let mut client = Client::new("localhost:25575").await.unwrap();
        if client.login(pass).await.unwrap() == false {
            return false;
        };
        self.client = Some(client);
        true
    }

    async fn process(&mut self, command: String) -> Result<String, anyhow::Error> {
        let inclusive_deny = false;

        debug!("entering process loop");
        let body = if self.regex_engine.is_match(&command) ^ inclusive_deny {
            let client = match self.client.as_mut() {
                Some(s) => s,
                None => return Err(anyhow::Error::msg("no client")),
            };
            match client.run(command).await {
                Ok(s) => s,
                Err(e) => {
                    format!("error connecting to upstream client: {:?}", e)
                }
            }
        } else {
            String::from("insufficient permissions")
        };
        Ok(body)
    }
}

fn init() {
    let d = dotenv::dotenv(); // causes side effects which need to happen before env logger starts.

    env_logger::Builder::new()
        .parse_env("LOG")
        .target(env_logger::Target::Stdout)
        .init();
    // process the results of dotenv
    match d {
        Ok(_) => debug!("Loaded environment variables from .env file"),
        Err(e) => match e {
            dotenv::Error::LineParse(var, line) => debug!(
                "Failed to load .env file due to an error parsing {} a {}",
                var, line
            ),
            dotenv::Error::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(".env file not found")
            }
            _ => debug!("Failed to load .env file with error: \n{:?}", e),
        },
    };
}

#[tokio::main]
async fn main() {
    init();

    debug!("listening on 25566");
    rcon::RconServer::<Implimentor>::new()
        .run("0.0.0.0:25566")
        .await
}
