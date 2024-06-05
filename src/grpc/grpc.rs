use std::collections::HashMap;
use std::sync::Arc;
use crate::telemetry::telemetry::{
    Path,
    SubscriptionAdditionalConfig,
    SubscriptionMode,
    SubscriptionRequest
};
use futures::StreamExt;
use tonic::Request as GrpcRequest;
use crate::Path as ConfigPath;
use crate::jnx::jnx::jet::authentication as junos_auth;
use crate::Tls;
use crate::telemetry::telemetry::open_config_telemetry_client::OpenConfigTelemetryClient;
use log::error;
use tonic::metadata::MetadataMap;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use log::info;

use tokio::sync::{mpsc, RwLock};
pub struct Grpc{
    client: Client,
    address: String,
    rx: Arc<RwLock<mpsc::Receiver<Command>>>
}

impl Grpc{
    pub async fn new(address: String, tls: Tls, username: String, password: String) -> anyhow::Result<Self>{
        let mut map = MetadataMap::new();
        let ca = std::fs::read(tls.ca_file).unwrap();
        let crt = std::fs::read(tls.cert_file).unwrap();
        let key = std::fs::read(tls.key_file).unwrap();
        map.insert("client-id", "cnm".parse().unwrap());
        let identity = tonic::transport::Identity::from_pem(crt, key);
        let tls = ClientTlsConfig::new()
            .domain_name(tls.server_name)
            .identity(identity)
            .ca_certificate(Certificate::from_pem(ca));

        let ep_address = format!("https://{}",address);
        info!("Connecting to {}", ep_address);
        let channel = Channel::from_shared(ep_address.clone())?
            .tls_config(tls)?
            .connect()
            .await?;
        info!("Connected to {}", ep_address);
        //c := auth.NewLoginClient(conn)
        let login_request = junos_auth::LoginRequest{
            username,
            password,
            group_id: "cnm".to_string(),
            client_id: "cnm".to_string(),
        };

        let login_response = match junos_auth::authentication_client::AuthenticationClient::new(channel.clone()).login(login_request).await{
            Ok(res) => {
                res
            },
            Err(e) => {
                error!("Failed to login: {:?}", e);
                return Err(e.into())
            }
        };

        info!("login response: {:#?}", login_response.into_inner());


        let client = OpenConfigTelemetryClient::new(channel);

        let (tx, rx) = mpsc::channel(32);
        let client = Client{tx, junos_client: client};
        let rx = Arc::new(RwLock::new(rx));
        Ok(Self{client, address, rx})
    }
    pub fn client(&self) -> Client{
        self.client.clone()
    }
}

#[derive(Clone)]
pub struct Client{
    tx: mpsc::Sender<Command>,
    junos_client: OpenConfigTelemetryClient<tonic::transport::Channel>,
}

impl Client{
    pub async fn subscribe_and_receive(&mut self, paths: Vec<ConfigPath>, username: String, password: String) -> anyhow::Result<()>{

        let mut sub_req = SubscriptionRequest::default();
        let mut add_config = SubscriptionAdditionalConfig::default();
        add_config.set_mode(SubscriptionMode::LongLived);
        add_config.need_eos = true;
        sub_req.additional_config = Some(add_config);
        
        let mut path_list: Vec<Path> = Vec::new();
        for p in &paths{
            let mut path = Path::default();
            path.path = p.path.clone();
            path.sample_frequency = p.freq;
            path_list.push(path);
        }
        sub_req.path_list = path_list;
        let mut req = GrpcRequest::new(sub_req);
        req.metadata_mut().insert("client-id", "cnm".parse().unwrap());
        req.metadata_mut().insert("username", username.parse().unwrap());
        req.metadata_mut().insert("password", password.parse().unwrap()); 
        let res = self.junos_client.telemetry_subscribe(req).await?;
        let mut s = res.into_inner();

        let now = tokio::time::Instant::now();
        while let Some(res) = s.next().await {
            match res{
                Ok(x) => {
                    info!("got request: {:#?}, elapsed: {}, sync: {}", x.path, now.elapsed().as_millis(), x.sync_response);
                },
                Err(e) => {
                    error!("Failed to receive: {:?}", e);
                }
            }
        };
        info!("Done");
        Ok(())
    }
}

//impl OpenConfigTelemetryClient for Client{}

enum Command{}