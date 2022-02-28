#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Could not find market {0}")]
    MarketSymbolNotFound(String),
    #[error("Could not find collateral {0}")]
    CollateralSymbolNotFound(String),
    #[error("Open orders account for {0} not created yet")]
    OpenOrdersNotFound(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    AnchorClient(#[from] anchor_client::ClientError),
    #[error("{0}")]
    SolanaClient(
        #[from] anchor_client::solana_client::client_error::ClientError,
    ),
    #[error("{0}")]
    ParsePubkey(#[from] anchor_client::solana_sdk::pubkey::ParsePubkeyError),
    #[error("{0}")]
    ParseInt(#[from] std::num::ParseIntError),
}

impl actix_web::ResponseError for Error {}
