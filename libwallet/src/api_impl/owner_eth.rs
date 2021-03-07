// Copyright 2021 The MWC Develope;
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Generic implementation of owner API eth functions

use rand::thread_rng;

use wagyu_ethereum::*;
use wagyu_model::*;

fn eth_new_account(network: String) -> Result<String, MnemonicError> {
	// type N = match network {
	// 	"ropsten" => ethereum::network::Ropsten,
	// 	_ => ethereum::Mainnet
	// };
	type N = Mainnet;
	type W = English;
	let mnemonic = EthereumMnemonic::<N, W>::new_with_count(&mut thread_rng(), 12).unwrap();
	// info!("eth_new_account: {}", mnemonic);

	// let mnemonic = ethereum::mnemonic::EthereumMnemonic::<N, ethereum::wordlist::English>::new_with_count(rng, 12).unwrap();
	// test_from_phrase::<N, W>(&mnemonic.entropy, &mnemonic.to_phrase().unwrap());
	Ok("phrase".to_string())
}

// fn get_swap_storage_key<K: Keychain>(keychain: &K) -> Result<SecretKey, Error> {
// 	Ok(keychain.derive_key(
// 		0,
// 		&ExtKeychainPath::new(3, 3, 2, 1, 0).to_identifier(),
// 		SwitchCommitmentType::None,
// 	)?)
// }
// //// List Swap trades. Returns SwapId + Status
// pub fn swap_list<'a, L, C, K>(
// 	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
// 	keychain_mask: Option<&SecretKey>,
// 	do_check: bool,
// ) -> Result<Vec<SwapListInfo>, Error>
// where
// 	L: WalletLCProvider<'a, C, K>,
// 	C: NodeClient + 'a,
// 	K: Keychain + 'a,
// {
// Need to lock first to check if the wallet is open
// 	wallet_lock!(wallet_inst, w);

// 	let swap_id = trades::list_swap_trades()?;
// 	let mut result: Vec<SwapListInfo> = Vec::new();

// 	let node_client = w.w2n_client().clone();
// 	let keychain = w.keychain(keychain_mask)?;
// 	let skey = get_swap_storage_key(&keychain)?;

// 	let mut do_check = do_check;

// 	for sw_id in &swap_id {
// 		let swap_lock = trades::get_swap_lock(sw_id);
// 		let _l = swap_lock.lock();
// 		let (context, mut swap) = trades::get_swap_trade(sw_id.as_str(), &skey, &*swap_lock)?;
// 		let trade_start_time = swap.started.timestamp();
// 		swap.wait_for_backup1 = true; // allways waiting becasue moving forward it is not a swap list task

// 		if do_check && !swap.state.is_final_state() {
// 			let (state, action, expiration) = match update_swap_status_action_impl(
// 				&mut swap,
// 				&context,
// 				node_client.clone(),
// 				&keychain,
// 			) {
// 				Ok((state, action, expiration, _state_eta)) => {
// 					swap.last_check_error = None;
// 					trades::store_swap_trade(&context, &swap, &skey, &*swap_lock)?;
// 					(state, action, expiration)
// 				}
// 				Err(e) => {
// 					do_check = false;
// 					swap.last_check_error = Some(format!("{}", e));
// 					swap.add_journal_message(format!("Processing error: {}", e));
// 					(swap.state.clone(), Action::None, None)
// 				}
// 			};

// 			result.push(SwapListInfo {
// 				swap_id: sw_id.clone(),
// 				is_seller: swap.is_seller(),
// 				mwc_amount: core::amount_to_hr_string(swap.primary_amount, true),
// 				secondary_amount: swap
// 					.secondary_currency
// 					.amount_to_hr_string(swap.secondary_amount, true),
// 				secondary_currency: swap.secondary_currency.to_string(),
// 				state,
// 				action: Some(action),
// 				expiration,
// 				trade_start_time,
// 				secondary_address: swap.get_secondary_address(),
// 				last_error: swap.get_last_error(),
// 			});
// 		} else {
// 			result.push(SwapListInfo {
// 				swap_id: sw_id.clone(),
// 				is_seller: swap.is_seller(),
// 				mwc_amount: core::amount_to_hr_string(swap.primary_amount, true),
// 				secondary_amount: swap
// 					.secondary_currency
// 					.amount_to_hr_string(swap.secondary_amount, true),
// 				secondary_currency: swap.secondary_currency.to_string(),
// 				state: swap.state.clone(),
// 				action: None,
// 				expiration: None,
// 				trade_start_time,
// 				secondary_address: swap.get_secondary_address(),
// 				last_error: swap.get_last_error(),
// 			});
// 		}
// 		trades::store_swap_trade(&context, &swap, &skey, &*swap_lock)?;
// 	}

// 	Ok(result)
// }
