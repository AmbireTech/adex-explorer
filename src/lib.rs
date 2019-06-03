#[macro_use]
extern crate seed;
use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use futures::Future;
use num::bigint::BigUint;
use num::integer::Integer;
use num::traits::ToPrimitive;
use num_format::{Locale, ToFormattedString};
use seed::prelude::*;
use seed::{Method, Request};
use serde::Deserialize;
use std::collections::HashMap;

mod ad_unit;
use ad_unit::*;
mod targeting_tag;
use targeting_tag::*;
mod validator;
use validator::*;
mod event_submission;
use event_submission::*;
mod channel;
use channel::*;

mod bignum;
use bignum::*;

const MARKET_URL: &str = "https://market.adex.network";
const ETHERSCAN_URL: &str = "https://api.etherscan.io/api";
const ETHERSCAN_API_KEY: &str = "CUSGAYGXI4G2EIYN1FKKACBUIQMN5BKR2B";
const IPFS_GATEWAY: &str = "https://ipfs.adex.network/ipfs/";
const DAI_ADDR: &str = "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359";
const CORE_ADDR: &str = "0x333420fc6a897356e69b62417cd17ff012177d2b";
const DEFAULT_EARNER: &str = "0xb7d3f81e857692d13e9d63b232a90f4a1793189e";
const REFRESH_MS: i32 = 10000;

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
struct MarketStatus {
    #[serde(rename = "name")]
    pub status_type: MarketStatusType,
    pub usd_estimate: f32,
    #[serde(rename = "lastApprovedBalances")]
    pub balances: HashMap<String, BigNum>,
    #[serde(with = "ts_milliseconds")]
    pub last_checked: DateTime<Utc>,
}
impl MarketStatus {
    fn balances_sum(&self) -> BigUint {
        self.balances.iter().map(|(_, v)| &v.0).sum()
    }
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct MarketChannel {
    pub id: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    pub status: MarketStatus,
    pub spec: ChannelSpec,
}

// Etherscan API
#[derive(Deserialize, Clone, Debug)]
struct EtherscanBalResp {
    pub result: BigNum
}

// Model
enum Loadable<T> {
    Loading,
    Ready(T),
}
enum ChannelSort {
    Deposit,
    Status,
    Created
}
struct Model {
    pub load_action: ActionLoad,
    pub sort: ChannelSort,
    // Market channels & balance: for the summaries page
    pub market_channels: Loadable<Vec<MarketChannel>>,
    pub balance: Loadable<EtherscanBalResp>,
    // Current selected channel: for ChannelDetail
    pub channel: Loadable<Channel>,
}
impl Default for Model {
    fn default() -> Self {
        Model {
            load_action: ActionLoad::Summary,
            market_channels: Loadable::Loading,
            sort: ChannelSort::Deposit,
            balance: Loadable::Loading,
            channel: Loadable::Loading,
        }
    }
}

// Update
#[derive(Clone)]
enum ActionLoad {
    // The summary includes latest campaigns on the market,
    // and some on-chain data (e.g. DAI balance on core SC)
    Summary,
    // The channel detail contains a summary of what validator knows about a channel
    ChannelDetail(String)
}
impl ActionLoad {
    fn perform_effects(&self, orders: &mut Orders<Msg>) {
        match self {
            ActionLoad::Summary => {
                // Load on-chain balances
                let etherscan_uri = format!(
                    "{}?module=account&action=tokenbalance&contractAddress={}&address={}&tag=latest&apikey={}",
                    ETHERSCAN_URL,
                    DAI_ADDR,
                    CORE_ADDR,
                    ETHERSCAN_API_KEY
                );
                orders.perform_cmd(Request::new(&etherscan_uri)
                    .method(Method::Get)
                    .fetch_json()
                    .map(Msg::BalanceLoaded)
                    .map_err(Msg::OnFetchErr)
                );

                // Load campaigns from the market
                orders.perform_cmd(Request::new(&format!("{}/campaigns?all", MARKET_URL))
                    .method(Method::Get)
                    .fetch_json()
                    .map(Msg::ChannelsLoaded)
                    .map_err(Msg::OnFetchErr)
                );
            },
            ActionLoad::ChannelDetail(id) => {
                let market_uri = format!(
                    "{}/channel/{}/events-aggregates/{}?timeframe=hour&limit=168",
                    MARKET_URL,
                    &id,
                    // @TODO get rid of this default earner thing, it's very very temporary
                    // we should get an aggr of all earners
                    DEFAULT_EARNER
                );
                // @TODO
            }
        }
    }
}

#[derive(Clone)]
enum Msg {
    Load(ActionLoad),
    Refresh,
    BalanceLoaded(EtherscanBalResp),
    ChannelsLoaded(Vec<MarketChannel>),
    OnFetchErr(JsValue),
    SortSelected(String),
}

fn update(msg: Msg, model: &mut Model, orders: &mut Orders<Msg>) {
    match msg {
        Msg::Load(load_action) => {
            // Do not render
            orders.skip();
            // Perform the effects
            load_action.perform_effects(orders);
            // This can be used on refresh
            model.load_action = load_action;
        }
        Msg::Refresh => {
            orders.skip();
            model.load_action.perform_effects(orders);
        }
        Msg::BalanceLoaded(resp) => model.balance = Loadable::Ready(resp),
        Msg::ChannelsLoaded(channels) => model.market_channels = Loadable::Ready(channels),
        Msg::SortSelected(sort_name) => match &sort_name as &str {
            "deposit" => model.sort = ChannelSort::Deposit,
            "status" => model.sort = ChannelSort::Status,
            "created" => model.sort = ChannelSort::Created,
            _ => (),
        },
        // @TODO handle this
        // report via a toast
        Msg::OnFetchErr(_) => (),
    }
}

// View
fn view(model: &Model) -> El<Msg> {
    let channels = match &model.market_channels {
        Loadable::Loading => return h2!["Loading..."],
        Loadable::Ready(c) => c,
    };

    let total_impressions: u64 = channels
        .iter()
        .map(|x| {
            (&x.status.balances_sum() / &x.spec.min_per_impression.0)
                .to_u64()
                .unwrap_or(0)
        })
        .sum();

    // @TODO we can make a special type for DAI channels and that way shield ourselves of
    // rendering wrongly
    let mut channels_dai: Vec<MarketChannel> = channels
        .iter()
        .filter(|MarketChannel { deposit_asset, .. }| deposit_asset == DAI_ADDR)
        .cloned()
        .collect();

    let total_paid: BigUint = channels_dai.iter().map(|x| x.status.balances_sum()).sum();
    let total_deposit: BigUint = channels_dai
        .iter()
        .map(|MarketChannel { deposit_amount, .. }| &deposit_amount.0)
        .sum();

    match model.sort {
        ChannelSort::Deposit => {
            channels_dai.sort_by(|x, y| y.deposit_amount.0.cmp(&x.deposit_amount.0));
        }
        ChannelSort::Status => channels_dai.sort_by_key(|x| x.status.status_type.clone()),
        ChannelSort::Created => channels_dai.sort_by(|x, y| y.spec.created.cmp(&x.spec.created)),
    }

    div![
        match &model.balance {
            Loadable::Ready(resp) => card("Locked up on-chain", &dai_readable(&resp.result.0)),
            _ => seed::empty()
        },
        card("Campaigns", &channels.len().to_string()),
        card("Ad units", &channels.iter().map(|x| x.spec.ad_units.len()).sum::<usize>().to_string()),
        card("Total campaign deposits", &dai_readable(&total_deposit)),
        card("Paid out", &dai_readable(&total_paid)),
        // @TODO warn that this is an estimation; add a question mark next to it
        // to explain what an estimation means
        card(
            "Impressions",
            &total_impressions.to_formatted_string(&Locale::en)
        ),
        div![
            select![
                attrs! {At::Value => "deposit"},
                option![attrs! {At::Value => "deposit"}, "Sort by deposit"],
                option![attrs! {At::Value => "status"}, "Sort by status"],
                option![attrs! {At::Value => "created"}, "Sort by created"],
                input_ev(Ev::Input, Msg::SortSelected)
            ],
            table![channel_table(&channels_dai)]
        ]
    ]
}

fn card(label: &str, value: &str) -> El<Msg> {
    div![
        class!["card"],
        div![class!["card-value"], value],
        div![class!["card-label"], label],
    ]
}

fn channel_table(channels: &[MarketChannel]) -> Vec<El<Msg>> {
    let header = tr![
        td!["URL"],
        td!["USD estimate"],
        td!["Deposit"],
        td!["Paid"],
        td!["Paid - %"],
        td!["Status"],
        td!["Created"],
        //td!["Last updated"],
        td!["Preview"]
    ];

    std::iter::once(header)
        .chain(channels.iter().map(channel))
        .collect::<Vec<El<Msg>>>()
}

fn channel(channel: &MarketChannel) -> El<Msg> {
    let deposit_amount = &channel.deposit_amount.0;
    let paid_total = channel.status.balances_sum();
    let url = format!(
        "{}/channel/{}/status",
        channel.spec.validators.get(0).map_or("", |v| &v.url),
        channel.id
    );
    let id_prefix = channel.id.chars().take(6).collect::<String>();
    tr![
        class!(if seconds_since(&channel.status.last_checked) > 180 { "not-recent" } else { "recent" }),
        td![a![
            attrs! {At::Href => url; At::Target => "_blank"},
            id_prefix
        ]],
        td![format!("${:.2}", &channel.status.usd_estimate)],
        td![dai_readable(&deposit_amount)],
        td![dai_readable(&paid_total)],
        td![{
            let base = 100000u32;
            let paid_units = (paid_total * base).div_floor(deposit_amount);
            let paid_hundreds = paid_units.to_f64().unwrap_or(base as f64) / (base as f64 / 100.0);
            format!("{:.3}%", paid_hundreds)
        }],
        td![format!("{:?}", &channel.status.status_type)],
        td![time(&channel.spec.created)],
        //td![time(&channel.status.last_checked)],
        td![class!["preview"], {
            match channel.spec.ad_units.get(0) {
                Some(unit) => image(&unit.media_url),
                None => seed::empty()
            }
        }]
    ]
}

fn image(url: &str) -> El<Msg> {
    if url.starts_with("ipfs://") {
        img![attrs!{ At::Src => url.replace("ipfs://", IPFS_GATEWAY) }]
    } else {
        img![attrs!{ At::Src => url }]
    }
}

fn seconds_since(t: &DateTime<Utc>) -> i64 {
    (js_sys::Date::now() as i64) / 1000 - t.timestamp()
}

fn time(t: &DateTime<Utc>) -> String {
    let time_diff = seconds_since(t);
    match time_diff {
        x if x < 0 => format!("just now"),
        x if x < 60 => format!("{} seconds ago", x),
        x if x < 3600 => format!("{} minutes ago", x / 60),
        x if x < 86400 => format!("{} hours ago", x / 3600),
        _ => format!("{}", t.format("%Y-%m-%d")), // %T if we want time
    }
}

fn dai_readable(bal: &BigUint) -> String {
    // 10 ** 16
    match bal.div_floor(&10_000_000_000_000_000u64.into()).to_f64() {
        Some(hundreds) => format!("{:.2} DAI", hundreds / 100.0),
        None => ">max".to_owned(),
    }
}

// Router
fn routes(url: &seed::Url) -> Msg {
    match url.path.get(0).map(|x| x.as_ref()) {
        Some("channel") => {
            match url.path.get(1) {
                Some(id) => Msg::Load(ActionLoad::ChannelDetail(id.to_string())),
                None => Msg::Load(ActionLoad::Summary)
            }
        },
        _ => Msg::Load(ActionLoad::Summary),
    }
}

#[wasm_bindgen]
pub fn render() {
    let state = seed::App::build(Model::default(), update, view)
        .routes(routes)
        .finish()
        .run();

    state.update(Msg::Load(ActionLoad::Summary));
    seed::set_interval(
        Box::new(move || state.update(Msg::Refresh)),
        REFRESH_MS,
    );
}
