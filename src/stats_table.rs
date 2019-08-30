use super::{dai_readable, types, Msg};

use adex_domain::BigNum;
use lazysort::*;
use seed::prelude::*;
use std::collections::HashMap;
use types::{MarketChannel, MarketStatusType};

pub fn ad_unit_stats_table(channels: &[&MarketChannel]) -> Node<Msg> {
	let mut units_by_type = HashMap::<&str, (Vec<&MarketChannel>, Vec<&MarketChannel>)>::new();
	for channel in channels {
		for unit in channel.spec.ad_units.iter() {
		    	let units = units_by_type
				    .entry(&unit.ad_type)
				    .or_insert((vec![], vec![]));

			units.0.push(channel);
			if channel.status.status_type == MarketStatusType::Active {
				units.1.push(channel);
			}
		}
	}

	let units_by_type_stats = units_by_type
		.iter()
		.map(|(ad_type, (all, active))| {
			let total_vol: BigNum = all.iter().map(|x| &x.deposit_amount).sum();
			let total_active_vol: BigNum = active
				.iter()
				.map(|x| &x.deposit_amount - &x.status.balances_sum())
				.sum();

			let all_by_impression: BigNum = active
				.iter()
				.map(|x| &x.deposit_amount * &x.spec.min_per_impression)
				.sum();

			let all_deposits: BigNum = active.iter().map(|x| &x.deposit_amount).sum();

                        
			let avg_weighted_per_impression = if all_deposits == 0.into() { all_deposits } else { all_by_impression.div_floor(&all_deposits) };
			(
				ad_type,
				avg_weighted_per_impression,
				total_active_vol,
				// total_vol
			)
		})
		.sorted_by(|x, y| y.1.cmp(&x.1))
		.collect::<Vec<_>>();

	let header = tr![
		td!["Ad Size"],
		td!["CPM"],
		td!["Active volume"],
		// td!["Total volume"]
	];

	table![std::iter::once(header)
		.chain(units_by_type_stats.iter().map(
			|(
				ad_type,
				avg_weighted_per_impression,
				total_active_vol,
				//total_vol
			)| {
				tr![
					td![ad_type],
					td![dai_readable(&(avg_weighted_per_impression * &1000.into()))],
					td![dai_readable(&total_active_vol)],
					// td![dai_readable(&total_vol)],
				]
			}
		))
		.collect::<Vec<Node<Msg>>>()]
}
