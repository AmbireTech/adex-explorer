use super::{dai_readable, types, Msg};

use adex_domain::BigNum;
use lazysort::*;
use seed::prelude::*;
use std::collections::HashMap;
use types::{MarketChannel, MarketStatusType};

pub fn ad_unit_stats_table(channels: &[&MarketChannel]) -> Node<Msg> {
    let units_by_type = channels
        .iter()
        .flat_map(|channel| {
            channel
                .spec
                .ad_units
                .iter()
                .map(move |unit| (&unit.ad_type, channel))
        })
        .fold(
            HashMap::<&str, Vec<&MarketChannel>>::new(),
            |mut by_type, (ad_type, channel)| {
                by_type
                    .entry(&ad_type)
                    .or_insert_with(Vec::new)
                    .push(channel);

                by_type
            },
        );

    let units_by_type_stats = units_by_type
        .iter()
        .map(|(ad_type, all)| {
            let total_vol: BigNum = all.iter().map(|x| &x.deposit_amount).sum();

            let active = all
                .iter()
                .filter(|x| x.status.status_type == MarketStatusType::Active);
            let total_active_vol: BigNum = active
                .clone()
                .map(|x| &x.deposit_amount - &x.status.balances_sum())
                .sum();

            let all_by_impression: BigNum = active
                .clone()
                .map(|x| &x.deposit_amount * &x.spec.min_per_impression)
                .sum();

            let all_deposits: BigNum = active.clone().map(|x| &x.deposit_amount).sum();

            let avg_weighted_per_impression: BigNum = if all_deposits == BigNum::from(0) {
                BigNum::from(0)
            } else {
                all_by_impression.div_floor(&all_deposits)
            };

            (
                ad_type,
                avg_weighted_per_impression,
                total_active_vol,
                total_vol,
            )
        })
        .sorted_by(|x, y| y.1.cmp(&x.1))
        .collect::<Vec<_>>();

    let header = tr![
        td!["Ad Size"],
        //td!["Current CPM"],
        td!["Active volume"],
        td!["Total volume"]
    ];

    table![std::iter::once(header)
        .chain(
            units_by_type_stats
                .iter()
                .filter(|(_, _, total_active_vol, _)| { total_active_vol > &BigNum::from(0) })
                .map(
                    |(ad_type, avg_weighted_per_impression, total_active_vol, total_vol)| {
                        tr![
                            td![ad_type],
                            //td![dai_readable(&(avg_weighted_per_impression * &1000.into()))],
                            td![dai_readable(&total_active_vol)],
                            td![dai_readable(&total_vol)],
                        ]
                    }
                )
        )
        .collect::<Vec<Node<Msg>>>()]
}
