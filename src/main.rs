mod api;
mod error;
mod state;

pub use error::*;
pub use state::*;

use actix_web::{middleware, web::Data, App, HttpServer};
use anchor_client::solana_sdk::signer::keypair;
use clap::Parser;
use zo_abi as zo;

#[derive(Parser)]
struct Cli {
    /// Solana cluster to use as either a URL or the name of the cluster.
    #[clap(short, long)]
    cluster: anchor_client::Cluster,

    /// Path to the payer keypair.
    #[clap(short, long)]
    payer: std::path::PathBuf,
}

#[actix_web::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::init();

    let Cli { cluster, payer } = Cli::parse();

    let payer = keypair::read_keypair_file(&payer).unwrap_or_else(|_| {
        panic!("Failed to read keypair from {}", payer.to_string_lossy());
    });
    let payer_bytes = payer.to_bytes();

    let zo_state = {
        let cluster = cluster.clone();
        tokio::task::spawn_blocking(move || {
            use anchor_client::{
                solana_sdk::{
                    commitment_config::CommitmentConfig, pubkey::Pubkey,
                    signer::null_signer::NullSigner,
                },
                Client,
            };
            let client = Client::new_with_options(
                cluster.clone(),
                std::rc::Rc::new(NullSigner::new(&Pubkey::default())),
                CommitmentConfig::processed(),
            );
            let program = client.program(zo::ID);
            program.account::<zo::State>(zo::ZO_STATE_ID).unwrap()
        })
        .await
        .unwrap()
    };

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::NormalizePath::trim())
            .wrap(
                middleware::DefaultHeaders::new()
                    .add(("Access-Control-Allow-Origin", "*")),
            )
            .wrap(middleware::Logger::new(
                "%a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %Dms",
            ))
            .app_data(Data::new(State::new(
                cluster.clone(),
                &keypair::Keypair::from_bytes(&payer_bytes).unwrap(),
                zo_state,
            )))
            .service(api::collateral_balances)
            .service(api::collateral_deposit)
            .service(api::collateral_withdraw)
            .service(api::position)
            .service(api::orders)
            .service(api::orders_post)
            .service(api::orders_delete)
    })
    .bind(format!(
        "0.0.0.0:{}",
        std::env::var("PORT").unwrap_or("8080".to_string())
    ))
    .unwrap()
    .run()
    .await
    .unwrap();
}
