#[macro_use]
extern crate seed;

mod stats_table;
mod types;

use adex_domain::{AdUnit, BigNum, Channel};
use chrono::{DateTime, Utc};
use lazysort::*;
use num_format::{Locale, ToFormattedString};
use seed::fetch;
use seed::prelude::*;
use seed::{Method, Request};
use stats_table::ad_unit_stats_table;
use std::collections::HashSet;
use types::{ChannelSort, EtherscanBalResp, Loadable, MarketChannel, AnalyticsResp};

use Loadable::*;

const MARKET_URL: &str = "https://market.adex.network";
const DAILY_VOL_URL: &str = "https://tom.adex.network/analytics?metric=eventPayouts&timeframe=day";
const IMPRESSIONS_URL: &str = "https://tom.adex.network/analytics?metric=eventCounts&timeframe=month";
const ETHERSCAN_URL: &str = "https://api.etherscan.io/api";
const ETHERSCAN_API_KEY: &str = "CUSGAYGXI4G2EIYN1FKKACBUIQMN5BKR2B";
const IPFS_GATEWAY: &str = "https://ipfs.adex.network/ipfs/";
const DAI_ADDR: &str = "0x6B175474E89094C44Da98b954EedeAC495271d0F";
const CORE_ADDR: &str = "0x333420fc6a897356e69b62417cd17ff012177d2b";
const DEFAULT_EARNER: &str = "0xb7d3f81e857692d13e9d63b232a90f4a1793189e";
const REFRESH_MS: i32 = 30000;

// @TODO can we derive this automatically
impl From<String> for ChannelSort {
    fn from(sort_name: String) -> Self {
        match &sort_name as &str {
            "deposit" => ChannelSort::Deposit,
            "status" => ChannelSort::Status,
            "created" => ChannelSort::Created,
            _ => ChannelSort::default(),
        }
    }
}

#[derive(Default)]
pub struct Model {
    pub load_action: ActionLoad,
    pub sort: ChannelSort,
    // Market channels & balance: for the summaries page
    pub market_channels: Loadable<Vec<MarketChannel>>,
    pub balance: Loadable<EtherscanBalResp>,
    pub volume: Loadable<AnalyticsResp>,
    pub impressions: Loadable<AnalyticsResp>,
    // Current selected channel: for ChannelDetail
    pub channel: Loadable<Channel>,
    pub last_loaded: i64,
}

// Update
#[derive(Clone, PartialEq, Debug)]
pub enum ActionLoad {
    // The summary includes latest campaigns on the market,
    // and some on-chain data (e.g. DAI balance on core SC)
    Summary,
    // Channels will show the summary plus the channels
    Channels,
    // The channel detail contains a summary of what validator knows about a channel
    ChannelDetail(String),
}
impl Default for ActionLoad {
    fn default() -> Self {
        ActionLoad::Summary
    }
}

impl ActionLoad {
    fn perform_effects(&self, orders: &mut impl Orders<Msg>) {
        match self {
            ActionLoad::Summary | ActionLoad::Channels => {
                // Load on-chain balances
                let etherscan_uri = format!(
                    "{}?module=account&action=tokenbalance&contractAddress={}&address={}&tag=latest&apikey={}",
                    ETHERSCAN_URL,
                    DAI_ADDR,
                    CORE_ADDR,
                    ETHERSCAN_API_KEY
                );
                orders.perform_cmd(
                    Request::new(etherscan_uri)
                        .method(Method::Get)
                        .fetch_json_data(Msg::BalanceLoaded),
                );

                // Load campaigns from the market
                orders.perform_cmd(
                    Request::new(format!("{}/campaigns?all", MARKET_URL))
                        .method(Method::Get)
                        .fetch_json_data(Msg::ChannelsLoaded),
                );

                // Load volume
                orders.perform_cmd(
                    Request::new(String::from(DAILY_VOL_URL))
                        .method(Method::Get)
                        .fetch_json_data(Msg::VolumeLoaded),
                );
                orders.perform_cmd(
                    Request::new(String::from(IMPRESSIONS_URL))
                        .method(Method::Get)
                        .fetch_json_data(Msg::ImpressionsLoaded),
                );
            }
            // NOTE: not used yet
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
pub enum Msg {
    Load(ActionLoad),
    Refresh,
    BalanceLoaded(fetch::ResponseDataResult<EtherscanBalResp>),
    ChannelsLoaded(fetch::ResponseDataResult<Vec<MarketChannel>>),
    VolumeLoaded(fetch::ResponseDataResult<AnalyticsResp>),
    ImpressionsLoaded(fetch::ResponseDataResult<AnalyticsResp>),
    SortSelected(String),
}

fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
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
        Msg::BalanceLoaded(Ok(resp)) => model.balance = Ready(resp),
        Msg::BalanceLoaded(Err(reason)) => log!("BalanceLoaded error:", reason),
        Msg::ChannelsLoaded(Ok(channels)) => {
            model.market_channels = Ready(channels);
            model.last_loaded = (js_sys::Date::now() as i64) / 1000;
        }
        Msg::ChannelsLoaded(Err(reason)) => log!("ChannelsLoaded error:", reason),
        Msg::VolumeLoaded(Ok(vol)) => model.volume = Ready(vol),
        Msg::VolumeLoaded(Err(reason)) => log!("VolumeLoaded error:", reason),
        Msg::ImpressionsLoaded(Ok(vol)) => model.impressions = Ready(vol),
        Msg::ImpressionsLoaded(Err(reason)) => log!("ImpressionsLoaded error:", reason),
        Msg::SortSelected(sort_name) => model.sort = sort_name.into(),
    }
}

// View
fn view(model: &Model) -> Node<Msg> {
    let channels = match &model.market_channels {
        Loading => return h2!["Loading..."],
        Ready(c) => c,
    };

    let channels_dai = channels
        .iter();
        // disabled cause of the SAI to DAI migration
        // .filter(|MarketChannel { deposit_asset, .. }| deposit_asset == DAI_ADDR);

    let total_paid = channels_dai.clone().map(|x| x.status.balances_sum()).sum();
    let total_deposit = channels_dai
        .clone()
        .map(|MarketChannel { deposit_amount, .. }| deposit_amount)
        .sum();

    let unique_units = &channels
        .iter()
        .flat_map(|x| &x.spec.ad_units)
        .map(|x| &x.ipfs)
        .collect::<HashSet<_>>();

    let unique_publishers = channels_dai
        .clone()
        .flat_map(|x| {
            x.status
                .balances
                .keys()
                .map(|k| k.to_lowercase())
                .filter(|k| *k != x.creator.to_lowercase())
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>();

    let unique_advertisers = channels_dai
        .clone()
        .map(|x| x.creator.to_lowercase())
        .collect::<HashSet<_>>();
 
    div![
        // Cards
        card("Campaigns", Ready(channels.len().to_string())),
        card("Ad units", Ready(unique_units.len().to_string())),
        card("Publishers", Ready(unique_publishers.len().to_string())),
        card("Advertisers", Ready(unique_advertisers.len().to_string())),
        volume_card(
            "Monthly impressions",
            match &model.impressions {
                Ready(vol) => Ready(
                    vol.aggr
                        .iter()
                        .map(|x| &x.value)
                        .sum::<BigNum>()
                        .to_u64()
                        .unwrap_or(0)
                        .to_formatted_string(&Locale::en)
                ),
                Loading => Loading,
            },
            &model.impressions
        ),
        br![],
        card(
            "Total campaign deposits",
            Ready(dai_readable(&total_deposit))
        ),
        card("Paid out", Ready(dai_readable(&total_paid))),
        a![
            attrs! { At::Href => format!("https://etherscan.io/address/{}#tokentxns", CORE_ADDR) },
            card(
                "Locked up on-chain",
                match &model.balance {
                    Ready(resp) => Ready(dai_readable(&resp.result)),
                    Loading => Loading,
                }
            ),
        ],
        volume_card(
            "24h volume",
            match &model.volume {
                Ready(vol) => Ready(dai_readable(&vol.aggr.iter().map(|x| &x.value).sum())),
                Loading => Loading,
            },
            &model.volume
        ),
        // Tables
        if model.load_action == ActionLoad::Channels {
            div![
                select![
                    attrs! {At::Value => "deposit"},
                    option![attrs! {At::Value => "deposit"}, "Sort by deposit"],
                    option![attrs! {At::Value => "status"}, "Sort by status"],
                    option![attrs! {At::Value => "created"}, "Sort by created"],
                    input_ev(Ev::Input, Msg::SortSelected)
                ],
                channel_table(
                    model.last_loaded,
                    &channels_dai
                        .clone()
                        .sorted_by(|x, y| match model.sort {
                            ChannelSort::Deposit => y.deposit_amount.cmp(&x.deposit_amount),
                            ChannelSort::Status => x.status.status_type.cmp(&y.status.status_type),
                            ChannelSort::Created => y.spec.created.cmp(&x.spec.created),
                        })
                        .collect::<Vec<_>>()
                ),
            ]
        } else {
            seed::empty()
        },
        ad_unit_stats_table(&channels_dai.clone().collect::<Vec<_>>()),
        a![
            attrs! { At::Href => "https://platform.adex.network/#/"},
            div![
                class!["button"],
                "Go to platform"
            ]
        ],
        a![
            attrs! { At::Href => "https://www.adex.network/"},
            div![
                class!["button"],
                "Go to website"
            ]
        ]
    ]
}

fn card(label: &str, value: Loadable<String>) -> Node<Msg> {
    div![
        class!["card"],
        match value {
            Loading => div![class!["card-value loading"]],
            Ready(value) => div![class!["card-value"], value],
        },
        div![class!["card-label"], label],
    ]
}

fn volume_chart(vol: &AnalyticsResp) -> Option<Node<Msg>> {
    let values = vol.aggr.iter().map(|x| &x.value);
    let min = values.clone().min()?;
    let max = values.clone().max()?;
    let range = max - min;
    let width = 250_u64;
    let height = 60_u64;
    let points = values
        .clone()
        .map(|v| {
            (&(v - min) * &height.into())
                .div_floor(&range)
                .to_u64()
                .unwrap_or(0)
        })
        .take(vol.aggr.len() - 1)
        .collect::<Vec<_>>();
    let len = points.len() as u64;
    let ratio = width as f64 / (len - 1) as f64;
    Some(svg![
        attrs! {
            At::Style => "position: absolute; right: 0px; left: 0px; bottom: 10px;";
            At::Width => format!("{}px", width);
            At::Height => format!("{}px", height);
            At::ViewBox => format!("0 0 {} {}", width, height);
        },
        polyline![attrs! {
            At::Fill => "none";
            At::Custom("stroke".into()) => "#c8dbec";
            At::Custom("stroke-width".into()) => "4";
            At::Custom("points".into()) => points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{},{}", (i as f64 * ratio).ceil(), height-p))
                .collect::<Vec<_>>()
                .join(" ")
        }],
    ])
}

fn volume_card(card_label: &str, val: Loadable<String>, vol: &Loadable<AnalyticsResp>) -> Node<Msg> {
    let (card_value, vol) = match (&val, vol) {
        (Ready(val), Ready(vol)) => (val, vol),
        _ => return card(card_label, Loading),
    };
    match volume_chart(vol) {
        Some(chart) => div![
            class!["card chart"],
            chart,
            div![class!["card-value"], card_value],
            div![class!["card-label"], card_label],
        ],
        None => card(card_label, val),
    }
}

fn channel_table(last_loaded: i64, channels: &[&MarketChannel]) -> Node<Msg> {
    let header = tr![
        td!["URL"],
        td!["USD estimate"],
        td!["Deposit"],
        td!["CPM"],
        td!["Paid"],
        td!["Paid - %"],
        //td!["Max impressions"],
        td!["Status"],
        td!["Created"],
        //td!["Last updated"],
        td!["Preview"]
    ];

    let channels = std::iter::once(header)
        .chain(channels.iter().map(|c| channel(last_loaded, c)))
        .collect::<Vec<Node<Msg>>>();

    table![channels]
}

fn channel(last_loaded: i64, channel: &MarketChannel) -> Node<Msg> {
    let deposit_amount = &channel.deposit_amount;
    let paid_total = channel.status.balances_sum();
    let url = format!(
        "{}/channel/{}/status",
        &channel.spec.validators.leader().url,
        channel.id
    );
    let id_prefix = channel.id.chars().take(6).collect::<String>();
    // This has a tiny issue: when you go back to the explorer after being in another window,
    // stuff will be not-recent until we get the latest status
    tr![
        class!(
            if last_loaded - channel.status.last_checked.timestamp() > 180 {
                "not-recent"
            } else {
                "recent"
            }
        ),
        td![a![
            attrs! {At::Href => url; At::Target => "_blank"},
            id_prefix
        ]],
        td![match channel.status.usd_estimate.as_ref() {
            Some(usd_estimate) => format!("${:.2}", &usd_estimate),
            None => "N/A".to_string(),
        }],
        td![dai_readable(deposit_amount)],
        td![dai_readable(
            &(&channel.spec.min_per_impression * &1000.into())
        )],
        td![dai_readable(&paid_total)],
        td![{
            let base = 100_000_u64;
            let paid_units = (&paid_total * &base.into()).div_floor(deposit_amount);
            let paid_hundreds = paid_units.to_f64().unwrap_or(base as f64) / (base as f64 / 100.0);
            format!("{:.3}%", paid_hundreds)
        }],
        //td![
        //    (deposit_amount / &channel.spec.min_per_impression)
        //        .to_u64()
        //        .unwrap_or(0)
        //        .to_formatted_string(&Locale::en)
        //],
        td![format!("{:?}", &channel.status.status_type)],
        td![time_diff(last_loaded, &channel.spec.created)],
        //td![time_diff(last_loaded, &channel.status.last_checked)],
        td![class!["preview"], {
            match channel.spec.ad_units.get(0) {
                Some(unit) => a![
                    attrs! { At::Href => &unit.target_url; At::Target => "_blank" },
                    unit_preview(&unit)
                ],
                None => seed::empty(),
            }
        }]
    ]
}

fn unit_preview(unit: &AdUnit) -> Node<Msg> {
    if unit.media_mime.starts_with("video/") {
        video![
            attrs! { At::Src => to_http_url(&unit.media_url); At::AutoPlay => true; At::Loop => true; At::Muted => true }
        ]
    } else {
        img![attrs! { At::Src => to_http_url(&unit.media_url) }]
    }
}

fn to_http_url(url: &str) -> String {
    if url.starts_with("ipfs://") {
        url.replace("ipfs://", IPFS_GATEWAY)
    } else {
        url.to_owned()
    }
}

fn time_diff(now_seconds: i64, t: &DateTime<Utc>) -> String {
    let time_diff = now_seconds - t.timestamp();
    match time_diff {
        x if x < 0 => "just now".to_string(),
        x if x < 60 => format!("{} seconds ago", x),
        x if x < 3600 => format!("{} minutes ago", x / 60),
        x if x < 86400 => format!("{} hours ago", x / 3600),
        _ => format!("{}", t.format("%Y-%m-%d")), // %T if we want time
    }
}

fn dai_readable(bal: &BigNum) -> String {
    // 10 ** 16`
    match bal.div_floor(&10_000_000_000_000_000u64.into()).to_f64() {
        Some(hundreds) => format!("{:.2} DAI", hundreds / 100.0),
        None => ">max".to_owned(),
    }
}

// Router
fn routes(url: seed::Url) -> Msg {
    match url.path.get(0).map(|x| x.as_ref()) {
        Some("channels") => Msg::Load(ActionLoad::Channels),
        Some("channel") => match url.path.get(1) {
            Some(id) => Msg::Load(ActionLoad::ChannelDetail(id.to_string())),
            None => Msg::Load(ActionLoad::Summary),
        },
        _ => Msg::Load(ActionLoad::Summary),
    }
}

#[wasm_bindgen]
pub fn render() {
    let state = seed::App::build(
        |url, orders| {
            orders.send_msg(routes(url));
            Model::default()
        },
        update,
        view,
    )
    .routes(routes)
    .finish()
    .run();

    seed::set_interval(Box::new(move || state.update(Msg::Refresh)), REFRESH_MS);
}
