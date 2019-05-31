#[macro_use]
extern crate seed;
use seed::prelude::*;
use serde::Deserialize;
use seed::{Method, Request};
use futures::Future;
use num::integer::Integer;
use num::bigint::BigUint;
use num::traits::ToPrimitive;

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
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all="camelCase")]
struct MarketChannel {
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    pub status: MarketStatus
}

// Model
#[derive(Default)]
struct Model {
    pub channels: Vec<MarketChannel>,
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
        Msg::ChannelsLoaded(channels) => { model.channels = channels },
        Msg::OnFetchErr(_) => (), // @TODO handle this
    }
}


// View
fn view(model: &Model) -> El<Msg> {
    log!(format!("{:?}", &model.channels));
    let total_dai: BigUint = model
        .channels
        .iter()
        .filter_map(|MarketChannel { deposit_asset, deposit_amount, .. }|
            if deposit_asset == DAI_ADDR { Some(&deposit_amount.0) } else { None }
        )
        .sum();

    div![
        h3![format!("Total DAI on campaigns: {}", dai_readable(&total_dai))],
        table![view_channel_table(&model.channels)]
    ]
}
fn view_channel_table(channels: &[MarketChannel]) -> Vec<El<Msg>> {
    let rows = channels
        .iter()
        .map(view_channel);

    let header = Some(tr![
        td!["USD estimate"],
        td!["DAI"]
    ]).into_iter();

    header
        .chain(rows)
        .collect::<Vec<El<Msg>>>()
}
fn view_channel(channel: &MarketChannel) -> El<Msg> {
    tr![
        td![format!("{:.2}", &channel.status.usd_estimate)],
        td![dai_readable(&channel.deposit_amount.0)]
    ]
}
fn dai_readable(bal: &BigUint) -> String {
    // 10 ** 16
    match bal.div_floor(&10_000_000_000_000_000u64.into()).to_f64() {
        Some(hundreds) => format!("{:.2}", hundreds / 100.0),
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
