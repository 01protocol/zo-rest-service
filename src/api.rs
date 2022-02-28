use crate::*;
use actix_web::{
    delete, get, post,
    web::{Data, Json, Path, Query},
    HttpResponse,
};
use anchor_client::solana_sdk::{pubkey::Pubkey, sysvar::rent};
use fixed::types::I80F48;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use zo_abi as zo;

fn div_to_float<T: Into<i128>, U: Into<u32>>(n: T, p: U) -> f64 {
    let n: i128 = n.into();
    let p = 10i128.pow(p.into());
    let (q, r) = (n / p, n % p);
    q as f64 + (r as f64 / p as f64)
}

fn small_to_big<T: Into<u32>>(n: I80F48, decimals: T) -> f64 {
    (n / I80F48::from_num(10u64.pow(decimals.into()))).to_num()
}

fn big_to_small(n: f64, decimals: u32) -> u64 {
    let (a, b) = (n as u64, n.rem_euclid(1.));
    (a * 10u64.pow(decimals)) + (b * 10f64.powi(decimals as i32)) as u64
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
enum Side {
    #[serde(rename = "bid")]
    Bid,
    #[serde(rename = "ask")]
    Ask,
}

impl From<zo::dex::Side> for Side {
    fn from(x: zo::dex::Side) -> Self {
        match x {
            zo::dex::Side::Bid => Self::Bid,
            zo::dex::Side::Ask => Self::Ask,
        }
    }
}

#[derive(Deserialize, Clone, Copy)]
enum OrderType {
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "ioc")]
    ImmediateOrCancel,
    #[serde(rename = "postonly")]
    PostOnly,
    #[serde(rename = "reduceonlyioc")]
    ReduceOnlyIoc,
    #[serde(rename = "reduceonlylimit")]
    ReduceOnlyLimit,
    #[serde(rename = "fok")]
    FillOrKill,
}

impl From<OrderType> for zo::OrderType {
    fn from(x: OrderType) -> Self {
        match x {
            OrderType::Limit => Self::Limit,
            OrderType::ImmediateOrCancel => Self::ImmediateOrCancel,
            OrderType::PostOnly => Self::PostOnly,
            OrderType::ReduceOnlyIoc => Self::ReduceOnlyIoc,
            OrderType::ReduceOnlyLimit => Self::ReduceOnlyLimit,
            OrderType::FillOrKill => Self::FillOrKill,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    pub owner_slot: u8,
    pub fee_tier: u8,
    pub control: String,
    pub order_id: u128,
    pub client_order_id: u64,
    pub size: f64,
    pub price: f64,
    pub side: Side,
}

impl From<zo::dex::Order> for Order {
    fn from(x: zo::dex::Order) -> Self {
        Self {
            owner_slot: x.owner_slot,
            fee_tier: x.fee_tier,
            control: x.control.to_string(),
            order_id: x.order_id,
            client_order_id: x.client_order_id,
            size: x.size,
            price: x.price,
            side: x.side.into(),
        }
    }
}

#[derive(Serialize)]
struct SigResp {
    sig: String,
}

#[get("/collateral/balances")]
async fn collateral_balances(
    st: Data<State>,
) -> Result<Json<HashMap<String, f64>>, Error> {
    let (cache, margin) = tokio::try_join!(st.zo_cache(), st.zo_margin())?;
    let r = st
        .zo_collaterals()
        .enumerate()
        .map(|(i, c)| {
            let collat = I80F48::from(margin.collateral[i]);
            let mult = I80F48::from(match collat >= I80F48::ZERO {
                true => cache.borrow_cache[i].supply_multiplier,
                false => cache.borrow_cache[i].borrow_multiplier,
            });
            (
                String::from(c.oracle_symbol),
                small_to_big(collat * mult, c.decimals),
            )
        })
        .collect();

    Ok(Json(r))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollateralDepositQuery {
    #[serde(default)]
    repay_only: bool,
    amount: f64,
    token_account: Option<String>,
}

#[post("/collateral/deposit/{symbol}")]
async fn collateral_deposit(
    st: Data<State>,
    s: Path<String>,
    q: Json<CollateralDepositQuery>,
) -> Result<Json<SigResp>, Error> {
    let collateral = st.collateral(&s)?;
    let vault = *st.vault(&s)?;
    let decimals = collateral.decimals as u32;
    let token_account = match q.token_account {
        Some(ref s) => Pubkey::from_str(s)?,
        None => anchor_spl::associated_token::get_associated_token_address(
            &st.authority(),
            &collateral.mint,
        ),
    };
    let st = st.clone();
    let sig = tokio::task::spawn_blocking(move || {
        st.program()
            .request()
            .args(zo::instruction::Deposit {
                repay_only: q.repay_only,
                amount: big_to_small(q.amount, decimals),
            })
            .accounts(zo::accounts::Deposit {
                state: zo::ZO_STATE_ID,
                state_signer: st.zo_state_signer,
                cache: st.zo_state().cache,
                authority: st.authority(),
                margin: st.zo_margin_key,
                token_account,
                vault,
                token_program: anchor_spl::token::ID,
            })
            .send()
    })
    .await
    .unwrap()?
    .to_string();
    Ok(Json(SigResp { sig }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollateralWithdrawQuery {
    #[serde(default)]
    allow_borrow: bool,
    amount: f64,
    token_account: Option<String>,
}

#[post("/collateral/withdraw/{symbol}")]
async fn collateral_withdraw(
    st: Data<State>,
    s: Path<String>,
    q: Json<CollateralWithdrawQuery>,
) -> Result<Json<SigResp>, Error> {
    let collateral = st.collateral(&s)?;
    let vault = *st.vault(&s)?;
    let decimals = collateral.decimals as u32;
    let token_account = match q.token_account {
        Some(ref s) => Pubkey::from_str(s)?,
        None => anchor_spl::associated_token::get_associated_token_address(
            &st.authority(),
            &collateral.mint,
        ),
    };
    let margin = st.zo_margin().await?;
    let st = st.clone();
    let sig = tokio::task::spawn_blocking(move || {
        st.program()
            .request()
            .args(zo::instruction::Withdraw {
                allow_borrow: q.allow_borrow,
                amount: big_to_small(q.amount, decimals),
            })
            .accounts(zo::accounts::Withdraw {
                state: zo::ZO_STATE_ID,
                state_signer: st.zo_state_signer,
                cache: st.zo_state().cache,
                authority: st.authority(),
                margin: st.zo_margin_key,
                control: margin.control,
                token_account,
                vault,
                token_program: anchor_spl::token::ID,
            })
            .send()
    })
    .await
    .unwrap()?
    .to_string();
    Ok(Json(SigResp { sig }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionInfo {
    size: f64,
    value: f64,
    realized_pnl: f64,
    funding_index: f64,
    is_long: bool,
}

#[get("/position")]
async fn position(
    st: Data<State>,
) -> Result<Json<HashMap<String, PositionInfo>>, Error> {
    let (_, control) = st.trader_accounts().await?;
    let r = st
        .zo_markets()
        .zip(control.open_orders_agg.iter())
        .map(|(mkt, oo)| {
            (
                mkt.symbol.into(),
                match oo.key == Pubkey::default() {
                    true => PositionInfo {
                        size: 0.,
                        value: 0.,
                        realized_pnl: 0.,
                        funding_index: 1.,
                        is_long: true,
                    },
                    false => PositionInfo {
                        size: div_to_float(oo.pos_size, mkt.asset_decimals)
                            .abs(),
                        value: div_to_float(oo.native_pc_total, 6u32).abs(),
                        realized_pnl: div_to_float(
                            oo.realized_pnl,
                            mkt.asset_decimals,
                        ),
                        funding_index: div_to_float(oo.funding_index, 6u32),
                        is_long: { oo.pos_size } >= I80F48::ZERO,
                    },
                },
            )
        })
        .collect();
    Ok(Json(r))
}

#[get("/orders/{symbol}")]
async fn orders(
    st: Data<State>,
    s: Path<String>,
) -> Result<Json<Vec<Order>>, Error> {
    let mkt = st.dex_market(&s).await?;
    let (bids, asks) = tokio::try_join!(st.slab(mkt.bids), st.slab(mkt.asks))?;
    Ok(Json(
        bids.iter_front()
            .map(|o| mkt.parse_order(&o, zo::dex::Side::Bid))
            .chain(
                asks.iter_front()
                    .map(|o| mkt.parse_order(o, zo::dex::Side::Ask)),
            )
            .map(Into::into)
            .collect::<Vec<_>>(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrdersPostQuery {
    size: f64,
    price: f64,
    side: Side,
    order_type: OrderType,
    client_id: Option<u64>,
    limit: Option<u16>,
}

#[post("/orders/{symbol}")]
async fn orders_post(
    st: Data<State>,
    s: Path<String>,
    q: Json<OrdersPostQuery>,
) -> Result<HttpResponse, Error> {
    let mkt = st.dex_market(&s).await?;
    let margin = st.zo_margin().await?;
    let open_orders = st.oo(&s).await?;
    let st = st.clone();
    let sig = tokio::task::spawn_blocking(move || {
        let limit_price = mkt.price_to_lots(q.price);
        let max_base_quantity = mkt.size_to_lots(q.size);
        let max_quote_quantity =
            limit_price * max_base_quantity * mkt.pc_lot_size;
        st.program()
            .request()
            .args(zo::instruction::PlacePerpOrder {
                is_long: q.side == Side::Bid,
                limit_price,
                max_base_quantity,
                max_quote_quantity,
                order_type: q.order_type.into(),
                limit: q.limit.unwrap_or(20),
                client_id: q.client_id.unwrap_or(0),
            })
            .accounts(zo::accounts::PlacePerpOrder {
                state: zo::ZO_STATE_ID,
                state_signer: st.zo_state_signer,
                cache: st.zo_state().cache,
                authority: st.authority(),
                margin: st.zo_margin_key,
                control: margin.control,
                open_orders,
                dex_market: mkt.own_address,
                req_q: mkt.req_q,
                event_q: mkt.event_q,
                market_bids: mkt.bids,
                market_asks: mkt.asks,
                dex_program: zo::ZO_DEX_PID,
                rent: rent::ID,
            })
            .send()
    })
    .await
    .unwrap()?
    .to_string();
    Ok(HttpResponse::Created().json(SigResp { sig }))
}

#[derive(Deserialize)]
struct OrdersDeleteQuery {
    order_id: Option<String>,
    side: Option<Side>,
    client_id: Option<u64>,
}

#[delete("/orders/{symbol}")]
async fn orders_delete(
    st: Data<State>,
    s: Path<String>,
    q: Query<OrdersDeleteQuery>,
) -> Result<HttpResponse, Error> {
    let order_id = match q.order_id {
        Some(ref s) => Some(u128::from_str_radix(s, 10)?),
        None => None,
    };
    let mkt = st.dex_market(&s).await?;
    let margin = st.zo_margin().await?;
    let open_orders = st.oo(&s).await?;
    let st = st.clone();
    let sig = tokio::task::spawn_blocking(move || {
        st.program()
            .request()
            .args(zo::instruction::CancelPerpOrder {
                order_id: order_id,
                is_long: q.side.map(|s| s == Side::Bid),
                client_id: q.client_id,
            })
            .accounts(zo::accounts::CancelPerpOrder {
                state: zo::ZO_STATE_ID,
                cache: st.zo_state().cache,
                authority: st.authority(),
                margin: st.zo_margin_key,
                control: margin.control,
                open_orders,
                dex_market: mkt.own_address,
                event_q: mkt.event_q,
                market_bids: mkt.bids,
                market_asks: mkt.asks,
                dex_program: zo::ZO_DEX_PID,
            })
            .send()
    })
    .await
    .unwrap()?
    .to_string();
    Ok(HttpResponse::NoContent().json(SigResp { sig }))
}
