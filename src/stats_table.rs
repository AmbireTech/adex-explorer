use super::{types, Msg, dai_readable};

use adex_domain::{BigNum};
use lazysort::*;
use std::collections::HashMap;
use types::{MarketChannel, MarketStatusType};
use seed::prelude::*;

pub fn ad_unit_stats_table(channels: &[&MarketChannel]) -> Node<Msg> {
	let mut units_by_type = HashMap::<&str, Vec<&MarketChannel>>::new();
	let active = channels
		.iter()
		.filter(|x| x.status.status_type == MarketStatusType::Active);
	for channel in active {
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
			let total_vol: BigNum = all
				.iter()
				.map(|x| &x.deposit_amount - &x.status.balances_sum())
				.sum();
			(ad_type, avg_per_impression, total_vol)
		})
		.sorted_by(|x, y| y.1.cmp(&x.1))
		.collect::<Vec<_>>();

	let header = tr![td!["Ad Size"], td!["CPM"], td!["Active volume"],];

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
