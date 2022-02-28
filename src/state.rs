use crate::Error;
use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signer::{keypair::Keypair, Signer as _},
    },
    Client, Cluster, Program,
};
use zo_abi as zo;

pub struct State {
    payer: Keypair,
    cluster: Cluster,
    commitment: CommitmentConfig,
    zo_state: zo::State,
    pub zo_state_signer: Pubkey,
    pub zo_margin_key: Pubkey,
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self {
            payer: self.payer(),
            cluster: self.cluster.clone(),
            commitment: self.commitment,
            zo_state: self.zo_state,
            zo_state_signer: self.zo_state_signer.clone(),
            zo_margin_key: self.zo_margin_key.clone(),
        }
    }
}

impl State {
    pub fn new(cluster: Cluster, payer: &Keypair, zo_state: zo::State) -> Self {
        let (zo_state_signer, _) =
            Pubkey::find_program_address(&[zo::ZO_STATE_ID.as_ref()], &zo::ID);

        let (zo_margin_key, _) = Pubkey::find_program_address(
            &[
                payer.pubkey().as_ref(),
                zo::ZO_STATE_ID.as_ref(),
                b"marginv1",
            ],
            &zo::ID,
        );

        Self {
            payer: Keypair::from_bytes(&payer.to_bytes()).unwrap(),
            cluster,
            commitment: CommitmentConfig::finalized(),
            zo_state,
            zo_state_signer,
            zo_margin_key,
        }
    }

    fn market_symbol_index(&self, s: &str) -> Result<usize, Error> {
        self.zo_state
            .perp_markets
            .iter()
            .map(|m| String::from(m.symbol))
            .position(|x| x == s)
            .ok_or_else(|| Error::MarketSymbolNotFound(s.to_owned()))
    }

    fn collateral_symbol_index(&self, s: &str) -> Result<usize, Error> {
        self.zo_state
            .collaterals
            .iter()
            .map(|m| String::from(m.oracle_symbol))
            .position(|x| x == s)
            .ok_or_else(|| Error::CollateralSymbolNotFound(s.to_owned()))
    }

    fn payer(&self) -> Keypair {
        Keypair::from_bytes(&self.payer.to_bytes()).unwrap()
    }

    pub fn authority(&self) -> Pubkey {
        self.payer.pubkey()
    }

    pub fn client(&self) -> Client {
        Client::new_with_options(
            self.cluster.clone(),
            std::rc::Rc::new(self.payer()),
            self.commitment,
        )
    }

    pub fn program(&self) -> Program {
        self.client().program(zo::ID)
    }

    pub fn rpc(&self) -> RpcClient {
        self.program().rpc()
    }

    pub fn market(&self, s: &str) -> Result<&zo::PerpMarketInfo, Error> {
        Ok(&self.zo_state.perp_markets[self.market_symbol_index(s)?])
    }

    pub fn collateral(&self, s: &str) -> Result<&zo::CollateralInfo, Error> {
        Ok(&self.zo_state.collaterals[self.collateral_symbol_index(s)?])
    }

    pub fn vault(&self, s: &str) -> Result<&Pubkey, Error> {
        Ok(&self.zo_state.vaults[self.collateral_symbol_index(s)?])
    }

    pub async fn oo(&self, s: &str) -> Result<Pubkey, Error> {
        self.trader_accounts()
            .await?
            .1
            .open_orders_agg
            .iter()
            .zip(self.zo_markets())
            .find_map(|(oo, mkt)| match s == &String::from(mkt.symbol) {
                true => {
                    if oo.key == Pubkey::default() {
                        None
                    } else {
                        Some(oo.key)
                    }
                }
                false => None,
            })
            .ok_or_else(|| Error::OpenOrdersNotFound(s.to_owned()))
    }

    pub async fn dex_market(
        &self,
        s: &str,
    ) -> Result<zo::dex::ZoDexMarket, Error> {
        let st = self.clone();
        let s = s.to_string();
        tokio::task::spawn_blocking(move || {
            st.rpc()
                .get_account_data(&st.market(&s)?.dex_market)
                .map_err(Into::into)
                .map(|x| {
                    zo::dex::ZoDexMarket::deserialize(&x).copied().unwrap()
                })
        })
        .await
        .unwrap()
    }

    pub async fn slab(&self, k: Pubkey) -> Result<zo::dex::Slab, Error> {
        let st = self.clone();
        tokio::task::spawn_blocking(move || {
            st.rpc()
                .get_account_data(&k)
                .map_err(Into::into)
                .map(|x| zo::dex::Slab::deserialize(&x).unwrap())
        })
        .await
        .unwrap()
    }

    async fn program_account<T>(&self, k: &Pubkey) -> Result<T, Error>
    where
        T: 'static
            + anchor_client::anchor_lang::AccountDeserialize
            + std::marker::Send,
    {
        let st = self.clone();
        let k = *k;
        tokio::task::spawn_blocking(move || st.program().account::<T>(k))
            .await
            .unwrap()
            .map_err(Error::from)
    }

    pub fn zo_state(&self) -> &zo::State {
        &self.zo_state
    }

    pub async fn zo_cache(&self) -> Result<zo::Cache, Error> {
        self.program_account(&self.zo_state.cache).await
    }

    pub async fn zo_margin(&self) -> Result<zo::Margin, Error> {
        self.program_account(&self.zo_margin_key).await
    }

    pub async fn trader_accounts(
        &self,
    ) -> Result<(zo::Margin, zo::Control), Error> {
        let m = self.zo_margin().await?;
        Ok((m, self.program_account::<zo::Control>(&m.control).await?))
    }

    pub fn zo_markets(&self) -> impl Iterator<Item = &zo::PerpMarketInfo> {
        self.zo_state()
            .perp_markets
            .iter()
            .take_while(|m| !m.symbol.is_nil())
    }

    pub fn zo_collaterals(&self) -> impl Iterator<Item = &zo::CollateralInfo> {
        self.zo_state()
            .collaterals
            .iter()
            .take_while(|m| !m.oracle_symbol.is_nil())
    }
}
