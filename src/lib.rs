#[macro_use]
extern crate seed;
use seed::prelude::*;
use serde::Deserialize;
use seed::{Method, Request};
use futures::Future;

mod bignum;
use bignum::*;

const MARKET_URL: &str = "https://market.adex.network/campaigns?all";

// Channel stuff
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all="camelCase")]
struct MarketChannel {
    pub deposit_asset: String,
    pub deposit_amount: BigNum
}

// Model

struct Model {
    pub val: i32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            val: 0,
        }
    }
}


// Update

#[derive(Clone)]
enum Msg {
    LoadCampaigns,
    CampaignsLoaded(Vec<MarketChannel>),
    OnFetchErr(JsValue),
    Increment,
}

fn update(msg: Msg, model: &mut Model, orders: &mut Orders<Msg>) {
    match msg {
        Msg::LoadCampaigns => {
            let order = Request::new(MARKET_URL)
                .method(Method::Get)
                .fetch_json()
                .map(Msg::CampaignsLoaded)
                .map_err(Msg::OnFetchErr);
            orders.skip().perform_cmd(order);
        },
        Msg::CampaignsLoaded(campaigns) => {
            let total: num_bigint::BigUint = campaigns
                .iter()
                .map(|MarketChannel { deposit_amount, .. }| &deposit_amount.0)
                .sum();

            log!(format!("campaigns: {:?}", &campaigns));
        },
        Msg::OnFetchErr(_) => (), // @TODO
        Msg::Increment => model.val += 1,
    }
}


// View
fn view(model: &Model) -> El<Msg> {
    button![
        simple_ev(Ev::Click, Msg::Increment),
        format!("Hello, World × {}", model.val)
    ]
}

#[wasm_bindgen]
pub fn render() {
    let state = seed::App::build(Model::default(), update, view)
        .finish()
        .run();

    state.update(Msg::LoadCampaigns);
}
