use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TargetingTag {
    pub tag: String,
    pub score: Score,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(transparent)]
pub struct Score(#[serde(deserialize_with = "score_deserialize")] u8);

pub fn score_deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let score_unchecked: u8 = u8::deserialize(deserializer)?;

    match score_unchecked > 100 {
        true => Err(serde::de::Error::custom(
            "Score should be between 0 >= x <= 100",
        )),
        false => Ok(score_unchecked),
    }
}
