use chrono::serde::{ts_milliseconds, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::bignum::BigNum;
use crate::{AdUnit, EventSubmission, TargetingTag, ValidatorDesc};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    // TODO: Make a custom ser/deser 2 validators(leader, follower) array
    pub validators: Vec<ValidatorDesc>,
    /// Maximum payment per impression
    pub max_per_impression: BigNum,
    /// Minimum payment offered per impression
    pub min_per_impression: BigNum,
    /// An array of TargetingTag (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targeting: Vec<TargetingTag>,
    /// Minimum targeting score (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_targeting_score: Option<u64>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    pub event_submission: EventSubmission,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager
    //#[serde(default, skip_serializing_if = "Option::is_none", with = "ts_milliseconds_option")]
    //pub active_from: Option<DateTime<Utc>>,
    /// A random number to ensure the campaignSpec hash is unique
    pub nonce: BigNum,
    /// A millisecond timestamp of when the campaign should enter a withdraw period
    /// (no longer accept any events other than CHANNEL_CLOSE)
    /// A sane value should be lower than channel.validUntil * 1000 and higher than created
    /// It's recommended to set this at least one month prior to channel.validUntil * 1000
    #[serde(with = "ts_milliseconds")]
    pub withdraw_period_start: DateTime<Utc>,
    /// An array of AdUnit (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_units: Vec<AdUnit>,
}
