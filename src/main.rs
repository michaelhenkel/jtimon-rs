use std::{env, future};

use clap::Parser;
use grpc::grpc::Grpc;
use log::info;

pub mod jnx;
pub mod grpc;
pub mod gnmi;
pub mod gnmi_jnpr;
pub mod telemetry;

#[derive(Parser)]
pub struct Args{
    #[clap(short, long)]
    config: String,
}

#[derive(serde::Deserialize)]
struct Config{
    address: String,
    user: String,
    password: String,
    cid: String,
    tls: Tls,
    paths: Vec<Path>,
}

#[derive(serde::Deserialize)]
pub struct Tls{
    pub cert_file: String,
    pub key_file: String,
    pub ca_file: String,
    pub server_name: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct Path{
    path: String,
    freq: u32,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    let config = std::fs::read_to_string(args.config).unwrap();
    let configs: Vec<Config> = serde_yaml::from_str(&config).unwrap();
    let mut jh_list = Vec::new();
    for config in configs{
        let grpc = Grpc::new(config.address, config.tls, config.user.clone(), config.password.clone()).await.unwrap();
        let mut client = grpc.client();
        let jh = tokio::spawn(async move{
            if let Err(e) = client.subscribe_and_receive(config.paths, config.user, config.password).await{
                log::error!("Failed to subscribe: {:?}", e);
            };
        });
        jh_list.push(jh);
    }
    futures::future::join_all(jh_list).await;
    println!("Hello, world! xx");
}
