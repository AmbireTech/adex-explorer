#[macro_use]
extern crate seed;
use seed::prelude::*;
use serde::Deserialize;
use seed::{Method, Request};
use futures::Future;
use num::integer::Integer;
use num::bigint::BigUint;
use num::traits::ToPrimitive;
use std::collections::HashMap;

mod bignum;
use bignum::*;

const MARKET_URL: &str = "https://market.adex.network/campaigns?all";
const DAI_ADDR: &str = "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359";

// Data structs specific to the market
#[derive(Deserialize, Clone, Debug)]
pub enum MarketStatusType { Initializing, Ready, Active, Offline, Disconnected, Unhealthy, Withdraw, Expired, Exhausted }

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all="camelCase")]
struct MarketStatus {
    #[serde(rename="name")]
    pub status_type: MarketStatusType,
    pub usd_estimate: f32,
    #[serde(rename="lastApprovedBalances")]
    pub balances: HashMap<String, BigNum>
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all="camelCase")]
struct MarketChannel {
    pub id: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    pub status: MarketStatus
}

// Model
#[derive(Default)]
struct Model {
    pub channels: Option<Vec<MarketChannel>>,
    //pub sort: Sort,
}


// Update
#[derive(Clone)]
enum Msg {
    LoadCampaigns,
    ChannelsLoaded(Vec<MarketChannel>),
    OnFetchErr(JsValue),
}

fn update(msg: Msg, model: &mut Model, orders: &mut Orders<Msg>) {
    match msg {
        Msg::LoadCampaigns => {
            let order = Request::new(MARKET_URL)
                .method(Method::Get)
                .fetch_json()
                .map(Msg::ChannelsLoaded)
                .map_err(Msg::OnFetchErr);
            orders.skip().perform_cmd(order);
        },
        Msg::ChannelsLoaded(channels) => { model.channels = Some(channels) },
        // @TODO handle this
        Msg::OnFetchErr(_) => (), 
    }
}


// View
fn view(model: &Model) -> El<Msg> {
    let channels = match &model.channels {
        None => return h1!["Loading..."],
        Some(c) => c
    };

    // @TODO we can make a special type for DAI channels and that way shield ourselves of 
    // rendering wrongly

    let mut channels_dai: Vec<MarketChannel> = channels
        .iter()
        .filter(|MarketChannel { deposit_asset, .. }| deposit_asset == DAI_ADDR)
        .cloned()
        .collect();

    channels_dai
        .sort_by(|x, y| y.deposit_amount.0.cmp(&x.deposit_amount.0));

    let total_dai: BigUint = channels_dai
        .iter()
        .map(|MarketChannel { deposit_amount, .. }| &deposit_amount.0)
        .sum();

    div![
        h3![format!("Total DAI on campaigns: {}", dai_readable(&total_dai))],
        table![view_channel_table(&channels_dai)]
    ]
}

fn view_channel_table(channels: &[MarketChannel]) -> Vec<El<Msg>> {
    let rows = channels
        .iter()
        .map(view_channel);

    let header = tr![
        td!["USD estimate"],
        td!["Deposit"],
        td!["Paid"],
        td!["Paid - %"],
        td!["Status"]
    ];

    std::iter::once(header)
        .chain(rows)
        .collect::<Vec<El<Msg>>>()
}

fn view_channel(channel: &MarketChannel) -> El<Msg> {
    let deposit_amount = &channel.deposit_amount.0;
    let paid_total = channel.status.balances.iter().map(|(_, v)| &v.0).sum();
    //let url = format!("{}/channel/{}", ); @TODO when validators
    tr![
        //td![a![attrs!{At::Href => }, ]]
        td![format!("${:.2}", &channel.status.usd_estimate)],
        td![dai_readable(&deposit_amount)],
        td![dai_readable(&paid_total)],
        td![{
            let base = 100000u32;
            let paid_units = (paid_total * base).div_floor(deposit_amount);
            let paid_hundreds = paid_units.to_f64().unwrap_or(base as f64) / (base as f64 / 100.0);
            format!("{:.3}%", paid_hundreds)
        }],
        td![format!("{:?}", &channel.status.status_type)]
    ]
}
fn dai_readable(bal: &BigUint) -> String {
    // 10 ** 16
    match bal.div_floor(&10_000_000_000_000_000u64.into()).to_f64() {
        Some(hundreds) => format!("{:.2} DAI", hundreds / 100.0),
        None => ">max".to_owned()
    }
}

#[wasm_bindgen]
pub fn render() {
    let state = seed::App::build(Model::default(), update, view)
        .finish()
        .run();

    state.update(Msg::LoadCampaigns);
}
