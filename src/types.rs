use std::collections::HashMap;

use adex_domain::{BigNum, ChannelSpec};
use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::Deserialize;

// Data structs specific to the market
#[derive(Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarketStatusType {
    Initializing,
    Ready,
    Active,
    Offline,
    Disconnected,
    Unhealthy,
    Withdraw,
    Expired,
    Exhausted,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MarketStatus {
    #[serde(rename = "name")]
    pub status_type: MarketStatusType,
    pub usd_estimate: f32,
    #[serde(rename = "lastApprovedBalances")]
    pub balances: HashMap<String, BigNum>,
    #[serde(with = "ts_milliseconds")]
    pub last_checked: DateTime<Utc>,
}

impl MarketStatus {
    pub fn balances_sum(&self) -> BigNum {
        self.balances.iter().map(|(_, v)| v).sum()
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MarketChannel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    pub status: MarketStatus,
    pub spec: ChannelSpec,
}