#[macro_use]
extern crate seed;

use std::collections::HashMap;

use adex_domain::{AdUnit, BigNum, Channel, ChannelSpec};
use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc, TimeZone};
use js_sys;
use std::convert::TryInto;
use lazysort::*;
use num_format::{Locale, ToFormattedString};
use seed::prelude::*;
use seed::{fetch, Method, Request};
use serde::Deserialize;
use std::collections::HashSet;

const MARKET_URL: &str = "https://market.adex.network";
const VOLUME_URL: &str = "https://tom.adex.network/volume";
const IMPRESSIONS_URL: &str = "https://tom.adex.network/volume/monthly-impressions";
const ETHERSCAN_URL: &str = "https://api.etherscan.io/api";
const ETHERSCAN_API_KEY: &str = "CUSGAYGXI4G2EIYN1FKKACBUIQMN5BKR2B";
const IPFS_GATEWAY: &str = "https://ipfs.adex.network/ipfs/";
const DAI_ADDR: &str = "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359";
const CORE_ADDR: &str = "0x333420fc6a897356e69b62417cd17ff012177d2b";
const DEFAULT_EARNER: &str = "0xb7d3f81e857692d13e9d63b232a90f4a1793189e";
const REFRESH_MS: i32 = 30000;

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
    fn balances_sum(&self) -> BigNum {
        self.balances.iter().map(|(_, v)| v).sum()
    }
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct MarketChannel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    pub status: MarketStatus,
    pub spec: ChannelSpec,
}

// Volume response from the validator
#[derive(Deserialize, Clone, Debug)]
struct VolumeResp {
    pub aggr: Vec<VolDataPoint>,
}
#[derive(Deserialize, Clone, Debug)]
struct VolDataPoint {
    pub value: BigNum,
    pub time: DateTime<Utc>,
}

// Etherscan API
#[derive(Deserialize, Clone, Debug)]
struct EtherscanBalResp {
    pub result: BigNum,
}

enum Loadable<T> {
    Loading,
    Ready(T),
    Error,
}
impl<T> Default for Loadable<T> {
    fn default() -> Self {
        Loadable::Loading
    }
}
use Loadable::*;

#[derive(Clone, Copy)]
enum ChannelSort {
    Deposit,
    Status,
    Created,
}
impl Default for ChannelSort {
    fn default() -> Self {
        ChannelSort::Deposit
    }
}
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
///
/// Model
/// 
#[derive(Default)]
struct Model {
    pub load_action: ActionLoad,
    pub sort: ChannelSort,
    // Market channels & balance: for the summaries page
    pub market_channels: Loadable<Vec<MarketChannel>>,
    pub balance: Loadable<EtherscanBalResp>,
    pub volume: Loadable<VolumeResp>,
    pub impressions: Loadable<VolumeResp>,
    // Current selected channel: for ChannelDetail
    pub channel: Loadable<Channel>,
    pub last_loaded: i64,
    pub loading_status: Loadable<String>,
    pub errors: Vec<Error>,
}
///
/// check_status : use to cumulate the status of different kind of loadable data
/// 
macro_rules! check_status {
    ($field:expr, $nb_err:expr, $nb_loading:expr) => (
        match $field {
            Error => $nb_err += 1,
            Loading => $nb_loading += 1,
            _ => (),
        }
    )
}
impl Model {
    ///
    /// fn refresh_global_status : compute the global status, according to the respective status of Loadable data
    /// - if at least one data is in Error, global status is Error
    /// - else, if at least one data is in Loading, global status is Loading
    /// - otherwise global status is OK
    /// 
    fn refresh_global_status(&mut self) {
        let mut nb_err:i32 = 0;
        let mut nb_loading:i32 = 0;
        check_status!(self.market_channels, nb_err, nb_loading);
        check_status!(self.balance, nb_err, nb_loading);
        check_status!(self.volume, nb_err, nb_loading);
        check_status!(self.impressions, nb_err, nb_loading);
        self.loading_status = 
            if nb_err == 0 {
                if nb_loading == 0 {
                    self.errors.clear();
                    Ready("".to_string())
                } else { 
                    Loading
                }
            } else {
                Error
            }
    }
    ///
    /// fn add_error : to record a new error in the model
    /// 
    fn add_error(&mut self, error: Error) {
        self.errors.push(error);
        // avoid to have to many errors in the list : when it reachs 40, only keep the 20 last ones
        if self.errors.len() >= 40 {
            self.errors = self.errors.split_off(20);
        }
    }
}

type Cdate = chrono::DateTime<chrono::Utc>;

/// Wrap the Chrono date, with a simpler API, and compatibility with features not supported
/// for it with the wasm target.
/// For now, utc only.
///
/// 1-based month and day indexing.
#[derive(Clone)]
pub struct Date {
    wrapped: Cdate,
}

impl From<Cdate> for Date {
    fn from(date: Cdate) -> Self {
        Self { wrapped: date }
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32, hours: u32, minutes: u32, seconds: u32, milliseconds: u32) -> Self {
        Self {
            wrapped: Utc.ymd(year, month, day).and_hms_milli(hours, minutes, seconds, milliseconds),
        }
    }

    /// We use js_sys::Date, serialize it, then turn it into a Chrono date, due to limitations
    /// with Crono on the wasm target.
    pub fn now() -> Self {
        let now_js = js_sys::Date::new_0();

        Self::new(
            now_js
                .get_utc_full_year()
                .try_into()
                .expect("casting js year into chrono year failed"),
            now_js.get_utc_month() as u32 + 1, // JS using 0-based month indexing. Fix it.
            now_js.get_utc_date() as u32,
            now_js.get_utc_hours() as u32,
            now_js.get_utc_minutes() as u32,
            now_js.get_utc_seconds() as u32,
            now_js.get_utc_milliseconds() as u32,
        )
    }
    pub fn to_time_string(&self) -> String {
        return self.wrapped.to_string();
    }
}

struct Error {
    // pub time: DateTime<Utc>, // can't use Chrono in wasm target, see https://github.com/chronotope/chrono/issues/243
    pub time: Date,
    pub summary: String,
    pub details: Vec<String>,
}
impl Error {
    fn new(summary: String) -> Self {
        Self {
            time: Date::now(),
            summary: summary,
            details: Vec::new()
        }
    }
    fn new_with_details(summary: String, details: String) -> Self {
        Self {
            time: Date::now(),
            summary: summary,
            details: vec![details]
        }
    }
    fn new_with_suberror(summary: String, suberror: Error) -> Self {
        let mut details: Vec<_> = suberror.details.clone();
        details.append(&mut vec![suberror.summary]);
        Self {
            time: Date::now(),
            summary: summary,
            details: details
        }
    }
    fn to_time_string(&self) -> String {
        return self.time.to_time_string();
    }
}

// Update

///
/// struct FetchDataStruct : store data about a given kind of information to be fetched with fetch_json(), used to deal with the fetch response
/// 
#[derive(Clone)]
struct FetchDataStruct<T> {
    pub fetch_object: fetch::FetchObject<T>,
    // update_model callback is called if the fetch response is OK, in order to update the model according to the fetch data
    pub update_model: fn(&mut Model, Loadable<T>),
    // on_error callback is called if the fetch response in NOK, in order to deal with the error according to the information requested
    pub on_error: fn(&mut Model, Error),
}
impl<T> FetchDataStruct<T> {
    ///
    /// fn on_fetch_response : implement treatment dealing with the fetch reponse
    /// 
    fn on_fetch_response (self, model: &mut Model, info_type: &str) {
        match self.fetch_object.response() {
            Ok(response) => {
            // response OK, call update_model() callback
                (self.update_model)(model, Ready(response.data));
            }
            Err(fail_reason) => {
            // response NOK. Analyse the error type, get the details and then call on_error() callback
                let error: Error = match fail_reason {
                    fetch::FailReason::RequestError(request_error, fetch_object) => {
                        let fetch::RequestError::DomException(dom_exception) = request_error;
                        error!(dom_exception);
                        Error::new_with_details(
                            format!("The request has been aborted (timed out or network error)"), 
                            format!("'{}'", dom_exception.message()),
                        )
                    }
                    fetch::FailReason::DataError(data_error, _) => {
                        match data_error {
                            fetch::DataError::DomException(dom_exception) => {
                                error!(dom_exception);
                                Error::new_with_details(
                                    format!("[DataError] Converting body to String failed"), 
                                    format!("{}:{}", dom_exception.name(), dom_exception.message()),
                                )
                            }
                            fetch::DataError::SerdeError(serde_error, json) => {
                                error!(serde_error);
                                Error::new_with_details(
                                    format!("[DataError] Invalid data received for '{}':\n  {}", info_type, serde_error), 
                                    format!("{}", json),
                                )
                            }
                        }
                    }
                    fetch::FailReason::Status(_, fetch_object) => {
                        // response isn't ok, but maybe contains error messages - try to decode them:
                        match fetch_object.result.unwrap().data {
                            Err(fetch::DataError::SerdeError(_, json)) => {
                                error!(json);
                                Error::new_with_details(
                                    format!("The server returned an error"), 
                                    format!("{}", json),
                                )
                            },
                            data => {
                                Error::new_with_details(
                                    format!("Status Error with data"), 
                                    format!(""),
                                )
                            }
                        }
                    }
                };
                (self.on_error)(model, error);
            }
        }
    }
}
///
/// enum FetchedData : used to store the FetchDataStruct value in message Msg::DataFetched, when calling fetch_json(), according to the kind of information requested
/// 
#[derive(Clone)]
enum FetchedData {
    // for requesting balance
    Balance(FetchDataStruct<EtherscanBalResp>),
    // for requesting market_channels
    MarketChannels(FetchDataStruct<Vec<MarketChannel>>),
    // for requesting volume
    Volume(FetchDataStruct<VolumeResp>),
    // for requesting impressions
    Impressions(FetchDataStruct<VolumeResp>),
}
impl FetchedData {
    ///
    /// fn on_response : is called when the fetch request is finished, in order to deal with the response
    /// 
    fn on_response(&self, model: &mut Model) {
        match self {
            FetchedData::Balance(fetch_data_struct) => 
                fetch_data_struct.clone().on_fetch_response(model, "Balance"),
            &FetchedData::MarketChannels(ref fetch_data_struct) => 
                fetch_data_struct.clone().on_fetch_response(model, "Market channels"),
            &FetchedData::Volume(ref fetch_data_struct) => 
                fetch_data_struct.clone().on_fetch_response(model, "Volume"),
            &FetchedData::Impressions(ref fetch_data_struct) => 
                fetch_data_struct.clone().on_fetch_response(model, "Impressions"),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum ActionLoad {
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
                        .fetch_json(
                            move |fetch_object: fetch::FetchObject<EtherscanBalResp>| 
                            Msg::DataFetched(
                                FetchedData::Balance(
                                    FetchDataStruct {
                                        fetch_object: fetch_object,
                                        update_model: move |model, balance| {
                                            model.balance = balance;
                                        },
                                        on_error: move |model, error| {
                                            model.balance = Loadable::Error;
                                            error!("Fetching Balance failed ...");
                                            let new_error = Error::new_with_suberror(
                                                format!("Fetching Balance failed from URL:'{}'", ETHERSCAN_URL),
                                                error
                                            );
                                            model.add_error(new_error);
                                        },
                                    }
                                )
                            )
                        ),
                );

                // Load campaigns from the market
                // @TODO request DAI channels only
                orders.perform_cmd(
                    Request::new(format!("{}/campaigns?all", MARKET_URL))
                        .method(Method::Get)
                        .fetch_json(
                            move |fetch_object: fetch::FetchObject<Vec<MarketChannel>>| 
                            Msg::DataFetched(
                                FetchedData::MarketChannels(
                                    FetchDataStruct {
                                        fetch_object: fetch_object,
                                        update_model: move |model, market_channels| {
                                            model.market_channels = market_channels;
                                            model.last_loaded = (js_sys::Date::now() as i64) / 1000;
                                        },
                                        on_error: move |model, error| {
                                            model.market_channels = Loadable::Error;
                                            error!("Fetching Market Channels failed ...");
                                            let new_error = Error::new_with_suberror(
                                                format!("Fetching Market Channels failed from URL:'{}'", MARKET_URL),
                                                error
                                            );
                                            model.add_error(new_error);
                                        },
                                    }
                                )
                            )
                        ),
                );

                // Load volume
                orders.perform_cmd(
                    Request::new(VOLUME_URL)
                        .method(Method::Get)
                        .fetch_json(
                            move |fetch_object: fetch::FetchObject<VolumeResp>| 
                            Msg::DataFetched(
                                FetchedData::Volume(
                                    FetchDataStruct {
                                        fetch_object: fetch_object,
                                        update_model: move |model, volume| {
                                            model.volume = volume;
                                        },
                                        on_error: move |model, error| {
                                            model.volume = Loadable::Error;
                                            error!("Fetching Volume failed ...");
                                            let new_error = Error::new_with_suberror(
                                                format!("Fetching Volume failed from URL:'{}'", VOLUME_URL),
                                                error
                                            );
                                            model.add_error(new_error);
                                        },
                                    }
                                )
                            )
                        ),
                );

                // Load impressions
                orders.perform_cmd(
                    Request::new(IMPRESSIONS_URL)
                        .method(Method::Get)
                        .fetch_json(
                            move |fetch_object: fetch::FetchObject<VolumeResp>| 
                            Msg::DataFetched(
                                FetchedData::Impressions(
                                    FetchDataStruct {
                                        fetch_object: fetch_object,
                                        update_model: move |model, impressions| {
                                            model.impressions = impressions;
                                        },
                                        on_error: move |model, error| {
                                            model.impressions = Loadable::Error;
                                            error!("Fetching Impressions failed ...");
                                            let new_error = Error::new_with_suberror(
                                                format!("Fetching Impressions failed from URL:'{}'", IMPRESSIONS_URL),
                                                error
                                            );
                                            model.add_error(new_error);
                                        },
                                    }
                                )
                            )
                        ),
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
enum Msg {
    Load(ActionLoad),
    Refresh,
    // DataFetched is the unique message to be notified after a fetch request. However the FetchedData value indicates which type of information has been requested
    DataFetched(FetchedData),
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
        Msg::DataFetched(fetched_data) => {
            fetched_data.on_response(model);
        },
        Msg::SortSelected(sort_name) => model.sort = sort_name.into(),
        // @TODO handle this
        // report via a toast
    }
    // Compute the model global status
    model.refresh_global_status();
}

///
/// fn recursive_view_error_details : loops recursively over the suberrors of an error
/// 
fn recursive_view_error_details(details: &mut Vec<String>) -> Node<Msg> {
    if details.len() == 0 {
        return empty![];
    }
    let detail_option = details.pop();
    match detail_option {
        None => {
            error!("Unable to pop error from the errors list");
            empty![]
        },
        Some(detail) => {
            match details.len() {
                0 => { // details is now empty, finish with a simple div with class "error-details"
                    div![class!["error-details"], detail]
                },
                _ => { // remaining details after, call recusrsively and insert a details/summary block
                    div![
                        class!["suberror"],
                        details![
                            summary![
                                detail
                            ],
                            recursive_view_error_details(details)
                        ]
                    ]
                },
            }
        }
    }
}

///
/// fn view_errors : to display the list of errors, if any
/// 
fn view_errors(model: &Model) -> Node<Msg> {
    if model.errors.len() > 0 {
        div![
            class!["errors-section"],
            details![
                summary![
                    class!["errors-section-title"],
                    "You have errors !"
                ],
                model.errors.iter().map(|error|
                    div![
                        class!["error-block"],
                        details![
                            summary![
                                class!["error-summary"],
                                format!("[{}] {}", error.time.to_time_string(), error.summary)
                            ],
                            recursive_view_error_details(&mut error.details.clone())
                        ]
                    ]
                ),
            ]
        ]
    } else {
        empty![]
    }
}

// View
fn view(model: &Model) -> Node<Msg> {

    match &model.market_channels {
        Loading => return h2!["Loading..."],
        _ => {},
    };

    // create a Loadable for channels_dai to be reused later in the function
    let is_channels_dai = match &model.market_channels {
        Ready(channels) => Ready(channels.iter().filter(|MarketChannel { deposit_asset, .. }| deposit_asset == DAI_ADDR)),
        Loading => Loading,
        Error => Error,
    };


    div![
        // Cards
        card(
            "Campaigns",
            match &model.market_channels {
                Ready(channels) => Ready(channels.len().to_string()),
                Loading => Loading,
                Error => Error,
            }
        ),
        card("Ad units",
            match &model.market_channels {
                Ready(channels) => {
                    let unique_units = &channels
                        .iter()
                        .flat_map(|x| &x.spec.ad_units)
                        .map(|x| &x.ipfs)
                        .collect::<HashSet<_>>();
                    Ready(unique_units.len().to_string())
                },
                Loading => Loading,
                Error => Error,
            }
        ),
        card("Publishers",
        match &is_channels_dai {
                Ready(channels_dai) => {
                    let unique_publishers = channels_dai.clone()
                        .flat_map(|x| {
                            x.status
                                .balances
                                .keys()
                                .filter(|k| **k != x.creator)
                                .collect::<Vec<_>>()
                        })
                        .collect::<HashSet<_>>();
                    Ready(unique_publishers.len().to_string())
                },
                Loading => Loading,
                Error => Error,
            }
        ),
        volume_card(
            "Monthly impressions",
            match &model.impressions {
                Ready(vol) => Ready(vol.aggr
                    .iter()
                    .map(|x| &x.value)
                    .sum::<BigNum>()
                    .to_u64()
                    .unwrap_or(0)
                    .to_formatted_string(&Locale::en)),
                Loading => Loading,
                Error => Error,
            },
            &model.impressions
        ),
        br![],
        card("Total campaign deposits",
            match &is_channels_dai {
                Ready(channels_dai) => {
                    let total_deposit = channels_dai.clone()
                        .map(|MarketChannel { deposit_amount, .. }| deposit_amount)
                        .sum();
                    Ready(dai_readable(&total_deposit))
                },
                Loading => Loading,
                Error => Error,
            }
        ),
        card("Paid out",
            match &is_channels_dai {
                Ready(channels_dai) => {
                    let total_paid = channels_dai.clone().map(|x| x.status.balances_sum()).sum();
                    Ready(dai_readable(&total_paid))
                },
                Loading => Loading,
                Error => Error,
            }
        ),
        card("Locked up on-chain", match &model.balance {
            Ready(resp) => Ready(dai_readable(&resp.result)),
            Loading => Loading,
            Error => Error,
        }),
        volume_card(
            "24h volume",
            match &model.volume {
                Ready(vol) => Ready(dai_readable(&vol.aggr.iter().map(|x| &x.value).sum())),
                Loading => Loading,
                Error => Error,
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
                match &is_channels_dai {
                    Ready(channels_dai) => {
                        channel_table(
                            model.last_loaded,
                            &channels_dai.clone()
                                .sorted_by(|x, y| match model.sort {
                                    ChannelSort::Deposit => y.deposit_amount.cmp(&x.deposit_amount),
                                    ChannelSort::Status => x.status.status_type.cmp(&y.status.status_type),
                                    ChannelSort::Created => y.spec.created.cmp(&x.spec.created),
                                })
                                .collect::<Vec<_>>()
                        )
                    },
                    _ => empty![],
                },
            ]
        } else {
            seed::empty()
        },
        match &is_channels_dai {
            Ready(channels_dai) => ad_unit_stats_table(&channels_dai.clone().collect::<Vec<_>>()),
            _ => empty![],
        },
        view_errors(model)
    ]
}

fn card(label: &str, value: Loadable<String>) -> Node<Msg> {
    div![
        class!["card"],
        match value {
            Loading => div![class!["card-value loading"]],
            Ready(value) => div![class!["card-value"], value],
            Error => {
                div![class!["card-error-value"], "N/A"]
            },
        },
        div![class!["card-label"], label],
    ]
}

fn volume_chart(vol: &VolumeResp) -> Option<Node<Msg>> {
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

fn volume_card(card_label: &str, val: Loadable<String>, vol: &Loadable<VolumeResp>) -> Node<Msg> {
    let (card_value, vol) = match (&val, vol) {
        (Ready(val), Ready(vol)) => (val, vol),
        _ => return card(card_label, Loading)
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
        td![format!("${:.2}", &channel.status.usd_estimate)],
        td![dai_readable(deposit_amount)],
        td![dai_readable(
            &(&channel.spec.min_per_impression * &1000.into())
        )],
        td![dai_readable(&paid_total)],
        td![{
            let base = 100000_u64;
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

fn ad_unit_stats_table(channels: &[&MarketChannel]) -> Node<Msg> {
    let mut units_by_type = HashMap::<&str, Vec<&MarketChannel>>::new();
    for channel in channels.iter() {
        for unit in channel.spec.ad_units.iter() {
            units_by_type
                .entry(&unit.ad_type)
                .or_insert(vec![])
                .push(channel);
        }
    }
    let units_by_type_stats = units_by_type
        .iter()
        .map(|(ad_type, all)| {
            let total_per_impression: BigNum = all.iter().map(|x| &x.spec.min_per_impression).sum();
            // @TODO needs weighted avg
            let avg_per_impression = total_per_impression.div_floor(&(all.len() as u64).into());
            let total_vol: BigNum = all.iter().map(|x| &x.deposit_amount).sum();
            (ad_type, avg_per_impression, total_vol)
        })
        .sorted_by(|x, y| y.1.cmp(&x.1))
        .collect::<Vec<_>>();

    let header = tr![td!["Ad Size"], td!["CPM"], td!["Total volume"],];

    table![std::iter::once(header)
        .chain(
            units_by_type_stats
                .iter()
                .map(|(ad_type, avg_per_impression, total_vol)| {
                    tr![
                        td![ad_type],
                        td![dai_readable(&(avg_per_impression * &1000.into()))],
                        td![dai_readable(&total_vol)],
                    ]
                })
        )
        .collect::<Vec<Node<Msg>>>()]
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
        x if x < 0 => format!("just now"),
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
#[allow(clippy::needless_pass_by_value)]
fn routes(url: seed::Url) -> Msg {
    match url.path.get(0).map(String::as_str) {
        Some("channels") => Msg::Load(ActionLoad::Channels),
        Some("channel") => match url.path.get(1).as_ref() {
            Some(id) => Msg::Load(ActionLoad::ChannelDetail(id.to_string())),
            None => Msg::Load(ActionLoad::Summary),
        },
        _ => Msg::Load(ActionLoad::Summary),
    }
}
// ------ ------
//     Init : to force the initial route according to the URL (necessary since seed 0.4.0 - PR #189 'scalable application support'')
//             https://github.com/David-OConnor/seed/pull/189#issuecomment-517747635
// ------ ------

fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(routes(url));
    Model::default()
}

#[wasm_bindgen]
pub fn render() {
    let state = seed::App::build(init, update, view)
        .routes(routes)
        .finish()
        .run();

    seed::set_interval(Box::new(move || state.update(Msg::Refresh)), REFRESH_MS);
}
