// Copyright 2020 The MWC Develope;
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

//! Generic implementation of owner API atomic swap functions

use crate::grin_util::secp::key::SecretKey;
use crate::grin_util::Mutex;

use crate::grin_keychain::{Identifier, Keychain, SwitchCommitmentType};
use crate::internal::selection;
use crate::swap::error::ErrorKind;
use crate::swap::fsm::state::{Input, StateId, StateProcessRespond};
use crate::swap::message::{Message, Update};
use crate::swap::swap::Swap;
use crate::swap::types::{Action, Currency, SwapTransactionsConfirmations};
use crate::swap::{trades, BuyApi, Context, SwapApi};
use crate::types::NodeClient;
use crate::Error;
use crate::{
	wallet_lock, OutputData, OutputStatus, Slate, SwapStartArgs, TxLogEntry, TxLogEntryType,
	WalletBackend, WalletInst, WalletLCProvider,
};
use grin_util::to_hex;
use std::convert::TryFrom;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

// TODO  - Validation for all parameters.

/// Start swap trade process. Return SwapID that can be used to check the status or perform further action.
pub fn swap_start<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	params: &SwapStartArgs,
) -> Result<String, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	// Starting a swap trade.
	// This method only initialize and store the swap process. Nothing is done

	// TODO  - validate SwapStartArgs values
	// TODO  - we probably want to do that as a generic solution because all params need to be validated

	wallet_lock!(wallet_inst, w);
	let node_client = w.w2n_client().clone();
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;
	let height = node_client.get_chain_tip()?.0;

	let secondary_currency = Currency::try_from(params.secondary_currency.as_str())?;
	let mut swap_api = crate::swap::api::create_instance(&secondary_currency, node_client)?;

	let parent_key_id = w.parent_key_id(); // account is current one
	let (outputs, total, amount, fee) = crate::internal::selection::select_coins_and_fee(
		&mut **w,
		params.mwc_amount,
		height,
		params.minimum_confirmations.unwrap_or(10),
		500,
		1,
		false,
		&parent_key_id,
		&None, // outputs to include into the transaction
		1,     // Number of resulting outputs. Normally it is 1
		false,
		0,
	)?;

	let context = create_context(
		&mut **w,
		keychain_mask,
		&mut swap_api,
		&keychain,
		secondary_currency,
		true,
		Some(
			outputs
				.iter()
				.map(|out| (out.key_id.clone(), out.mmr_index.clone(), out.value))
				.collect(),
		),
		total - amount - fee,
	)?;

	let swap = (*swap_api).create_swap_offer(
		&keychain,
		&context,
		params.mwc_amount,       // mwc amount to sell
		params.secondary_amount, // btc amount to buy
		secondary_currency,
		params.secondary_redeem_address.clone(),
		params.seller_lock_first,
		params.mwc_confirmations,
		params.secondary_confirmations,
		params.message_exchange_time_sec,
		params.redeem_time_sec,
	)?;

	// Store swap result into the file.
	let swap_id = swap.id.to_string();
	if trades::get_swap_trade(swap_id.as_str(), &skey).is_ok() {
		// Should be impossible, uuid suppose to be unique. But we don't want to overwrite anything
		return Err(ErrorKind::TradeIoError(
			swap_id.clone(),
			"This trade record already exist".to_string(),
		)
		.into());
	}

	trades::store_swap_trade(&context, &swap, &skey)?;

	Ok(swap_id)
}

/// List Swap trades. Returns SwapId + Status
pub fn swap_list<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
) -> Result<Vec<(String, String)>, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	let swap_id = trades::list_swap_trades()?;
	let mut result: Vec<(String, String)> = Vec::new();

	wallet_lock!(wallet_inst, w);
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;

	for sw_id in &swap_id {
		let (_, swap) = trades::get_swap_trade(sw_id.as_str(), &skey)?;
		result.push((sw_id.clone(), swap.state.to_string()));
	}

	Ok(result)
}

/// Delete Swap trade.
pub fn swap_delete<'a, L, C, K>(
	_wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	_keychain_mask: Option<&SecretKey>,
	swap_id: &str,
) -> Result<(), Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	trades::delete_swap_trade(swap_id)?;
	Ok(())
}

/// Get a Swap kernel object.
pub fn swap_get<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
) -> Result<Swap, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	wallet_lock!(wallet_inst, w);
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;
	let (_, swap) = trades::get_swap_trade(swap_id, &skey)?;
	Ok(swap)
}

/// Update the state of Swap trade. Returns the new state
pub fn swap_adjust<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
	adjust_cmd: &str,
) -> Result<(StateId, Action), Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	wallet_lock!(wallet_inst, w);
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;
	let node_client = w.w2n_client();

	let (context, mut swap) = trades::get_swap_trade(swap_id, &skey)?;

	let swap_api =
		crate::swap::api::create_instance(&swap.secondary_currency, node_client.clone())?;
	let mut fsm = swap_api.get_fsm(&keychain, &swap);

	if adjust_cmd == "cancel" {
		if !fsm.is_cancellable(&swap)? {
			return Err(ErrorKind::Generic(
				"Swap Trade is not cancellable at current stage".to_string(),
			)
			.into());
		}

		// Cancelling the trade
		let tx_conf = swap_api.request_tx_confirmations(&keychain, &swap)?;
		let resp = fsm.process(Input::Cancel, &mut swap, &context, &tx_conf)?;
		trades::store_swap_trade(&context, &swap, &skey)?;

		return Ok((swap.state.clone(), resp.action.unwrap_or(Action::None)));
	}

	let state = StateId::from_cmd_str(adjust_cmd)?;
	if !fsm.has_state(&state) {
		return Err(
			ErrorKind::Generic(format!("State {} is invalid for this trade", adjust_cmd)).into(),
		);
	}
	swap.state = state;

	let tx_conf = swap_api.request_tx_confirmations(&keychain, &swap)?;
	let resp = fsm.process(Input::Check, &mut swap, &context, &tx_conf)?;
	trades::store_swap_trade(&context, &swap, &skey)?;

	return Ok((swap.state.clone(), resp.action.unwrap_or(Action::None)));
}

/// Dump the swap file content
pub fn swap_dump<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
) -> Result<String, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	wallet_lock!(wallet_inst, w);
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;
	let dump_res = trades::dump_swap_trade(swap_id, &skey)?;
	Ok(dump_res)
}

/// Get a status and action for the swap.
pub fn get_swap_status_action<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
) -> Result<(StateId, Action), Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	wallet_lock!(wallet_inst, w);
	let node_client = w.w2n_client().clone();
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;

	let (context, mut swap) = trades::get_swap_trade(swap_id, &skey)?;

	let swap_api = crate::swap::api::create_instance(&swap.secondary_currency, node_client)?;
	let mut fsm = swap_api.get_fsm(&keychain, &swap);
	let tx_conf = swap_api.request_tx_confirmations(&keychain, &mut swap)?;
	let resp = fsm.process(Input::Check, &mut swap, &context, &tx_conf)?;

	// Action might update the states. Need to save it
	trades::store_swap_trade(&context, &swap, &skey)?;

	Ok((resp.next_state_id, resp.action.unwrap_or(Action::None)))
}

/// Get a status of the transactions that involved into the swap.
pub fn get_swap_tx_tstatus<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
) -> Result<SwapTransactionsConfirmations, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	wallet_lock!(wallet_inst, w);
	let node_client = w.w2n_client().clone();
	let keychain = w.keychain(keychain_mask)?;
	let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;

	let (_context, mut swap) = trades::get_swap_trade(swap_id, &skey)?;

	let swap_api = crate::swap::api::create_instance(&swap.secondary_currency, node_client)?;
	let res = swap_api.request_tx_confirmations(&keychain, &mut swap)?;

	Ok(res)
}

/// Process the action for the swap. Action has to match the expected one
/// message_sender - method that can send the message to another party. Caller defines how it can be done
/// Return: new State & Action
pub fn swap_process<'a, L, C, K, F>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_id: &str,
	message_sender: F,
	destination: Option<String>, // destination is used for several commands with different meaning
	fee_satoshi_per_byte: Option<f32>,
) -> Result<StateProcessRespond, Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
	F: FnOnce(Message) -> Result<(), Error> + 'a,
{
	let (node_client, keychain, parent_key_id) = {
		wallet_lock!(wallet_inst, w);
		let node_client = w.w2n_client().clone();
		let keychain = w.keychain(keychain_mask)?;
		let parent_key_id = w.parent_key_id();
		(node_client, keychain, parent_key_id)
	};

	let skey = keychain.derive_key(0, &parent_key_id, SwitchCommitmentType::None)?;

	let (context, mut swap) = trades::get_swap_trade(swap_id, &skey)?;

	let swap_api =
		crate::swap::api::create_instance(&swap.secondary_currency, node_client.clone())?;

	let tx_conf = swap_api.request_tx_confirmations(&keychain, &swap)?;
	let mut fsm = swap_api.get_fsm(&keychain, &swap);

	let mut process_respond = fsm.process(Input::Check, &mut swap, &context, &tx_conf)?;

	// Action might update the states. Need to save it
	trades::store_swap_trade(&context, &swap, &skey)?;

	if process_respond.action.is_none() {
		return Ok(process_respond);
	}

	match process_respond.action.clone().unwrap() {
		Action::SellerSendOfferMessage(message)
		| Action::BuyerSendAcceptOfferMessage(message)
		| Action::BuyerSendInitRedeemMessage(message)
		| Action::SellerSendRedeemMessage(message) => {
			message_sender(message)?;
			process_respond = fsm.process(Input::execute(), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;
		}
		Action::SellerWaitingForOfferMessage
		| Action::SellerWaitingForInitRedeemMessage
		| Action::BuyerWaitingForRedeemMessage => {
			let message_fn = destination.ok_or(ErrorKind::Generic("Please define 'destination' value if you you are processing income message from the file".to_string()))?;

			let mut file = File::open(message_fn.clone()).map_err(|e| {
				ErrorKind::Generic(format!("Unable to open file {}, {}", message_fn, e))
			})?;
			let mut contents = String::new();
			file.read_to_string(&mut contents).map_err(|e| {
				ErrorKind::Generic(format!(
					"Unable to read a message from the file {}, {}",
					message_fn, e
				))
			})?;
			// processing the message with a regular API.

			let message = Message::from_json(&contents)?;
			if message.id != swap.id {
				return Err(ErrorKind::Generic(format!(
					"Message id {} doesn't match selected trade id",
					message.id
				))
				.into());
			}

			swap_income_message(wallet_inst.clone(), keychain_mask, &contents)?;
		}
		Action::SellerPublishMwcLockTx => {
			wallet_lock!(wallet_inst, w);
			// Checking if transaction is already created.
			let kernel = &swap.lock_slate.tx.body.kernels[0].excess;
			if w.tx_log_iter()
				.filter(|tx| tx.kernel_excess.filter(|c| c == kernel).is_some())
				.count() == 0
			{
				// Transaction doesn't exist, let's create it and lock the outputs.
				let seller_context = context.unwrap_seller()?;
				let slate_context = crate::types::Context::from_send_slate(
					&swap.lock_slate,
					context.lock_nonce.clone(),
					seller_context.inputs.clone(),
					vec![(
						seller_context.change_output.clone(),
						None,
						seller_context.change_amount,
					)],
					parent_key_id,
					0,
				)?;
				selection::lock_tx_context(
					&mut **w,
					keychain_mask,
					&swap.lock_slate,
					&slate_context,
					Some(format!("Swap {} Lock", swap_id)),
				)?;
			}

			process_respond = fsm.process(Input::execute(), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;
			println!(
				"Lock MWC slate is published at transaction {}",
				swap.lock_slate.id
			);
		}
		Action::SellerPublishTxSecondaryRedeem(_currency) => {
			process_respond = fsm.process(Input::execute(), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;
			println!(
				"{} redeem transaction is published",
				swap.secondary_currency
			);
		}
		Action::DepositSecondary {
			currency,
			amount,
			address,
		} => {
			println!(
				"Please deposit {} {} to {}",
				currency.amount_to_hr_string(amount, true),
				currency,
				address
			);
		}
		Action::BuyerPublishMwcRedeemTx => {
			process_respond = fsm.process(Input::execute(), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;

			wallet_lock!(wallet_inst, w);

			// Checking if this transaction already exist
			let kernel = &swap.redeem_slate.tx.body.kernels[0].excess;
			if w.tx_log_iter()
				.filter(|tx| tx.kernel_excess.filter(|c| c == kernel).is_some())
				.count() == 0
			{
				// Creating receive transaction from the slate
				let buyer_context = context.unwrap_buyer()?;
				create_receive_tx_record(
					&mut **w,
					keychain_mask,
					&swap.redeem_slate,
					format!("Swap {}", swap_id),
					&buyer_context.redeem,
				)?;
			}
			println!(
				"Redeem MWC slate is published at transaction {}",
				swap.redeem_slate.id
			);
		}
		Action::SellerPublishMwcRefundTx => {
			process_respond = fsm.process(Input::execute(), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;

			wallet_lock!(wallet_inst, w);

			let kernel = &swap.redeem_slate.tx.body.kernels[0].excess;
			if w.tx_log_iter()
				.filter(|tx| tx.kernel_excess.filter(|c| c == kernel).is_some())
				.count() == 0
			{
				// For MWC transaction we can create a record in the wallet.
				let seller_context = context.unwrap_seller()?;
				create_receive_tx_record(
					&mut **w,
					keychain_mask,
					&swap.refund_slate,
					format!("Swap {} Refund", swap_id),
					&seller_context.refund_output,
				)?;
			}
		}
		Action::BuyerPublishSecondaryRefundTx(_currency) => {
			if destination.is_none() {
				return Err(ErrorKind::Generic(format!(
					"Please specify 'destination' {} address for your refund",
					swap.secondary_currency
				))
				.into());
			}

			process_respond = fsm.process(
				Input::Execute {
					refund_address: destination,
					fee_satoshi_per_byte,
				},
				&mut swap,
				&context,
				&tx_conf,
			)?;
			trades::store_swap_trade(&context, &swap, &skey)?;
		}
		_ => (), // Nothing to do
	}

	Ok(process_respond)
}

// Creating Transaction and output for expected recieve slate
fn create_receive_tx_record<'a, T: ?Sized, C, K>(
	wallet: &mut T,
	keychain_mask: Option<&SecretKey>,
	slate: &Slate,
	tx_name: String,
	output_key_id: &Identifier,
) -> Result<(), Error>
where
	T: WalletBackend<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	let parent_key_id = wallet.parent_key_id();
	let mut batch = wallet.batch(keychain_mask)?;
	let log_id = batch.next_tx_log_id(&parent_key_id)?;
	let mut t = TxLogEntry::new(parent_key_id.clone(), TxLogEntryType::TxReceived, log_id);

	// Creating trnasaction
	t.tx_slate_id = Some(slate.id.clone());
	t.amount_credited = slate.amount;
	t.address = Some(tx_name);
	t.num_outputs = 1;
	t.output_commits = slate
		.tx
		.body
		.outputs
		.iter()
		.map(|o| o.commit.clone())
		.collect();
	t.messages = None;
	t.ttl_cutoff_height = None;
	// when invoicing, this will be invalid
	assert!(slate.tx.body.kernels.len() == 1);
	t.kernel_excess = Some(slate.tx.body.kernels[0].excess);
	t.kernel_lookup_min_height = Some(slate.height);
	batch.save_tx_log_entry(t, &parent_key_id)?;

	assert!(slate.tx.body.outputs.len() == 1);

	// Creating output for that
	batch.save(OutputData {
		root_key_id: parent_key_id.clone(),
		key_id: output_key_id.clone(),
		mmr_index: None,
		n_child: output_key_id.to_path().last_path_index(),
		commit: Some(to_hex(slate.tx.body.outputs[0].commit.0.to_vec())),
		value: slate.amount,
		status: OutputStatus::Unconfirmed,
		height: slate.height,
		lock_height: slate.lock_height,
		is_coinbase: false,
		tx_log_entry: Some(log_id),
	})?;
	batch.commit()?;
	Ok(())
}

/// Processing swap income message. Note result of that can be a new offer of modification of the current one
/// We only notify user about that, no permission will be ask.
/// Reason: Nothing will be done with the funds until user will go forward manually
pub fn swap_income_message<'a, L, C, K>(
	wallet_inst: Arc<Mutex<Box<dyn WalletInst<'a, L, C, K>>>>,
	keychain_mask: Option<&SecretKey>,
	swap_message: &str,
) -> Result<(), Error>
where
	L: WalletLCProvider<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	let message = Message::from_json(swap_message)?;
	let swap_id = message.id.to_string();

	match &message.inner {
		Update::None => {
			return Err(
				ErrorKind::Generic("Get empty message, nothing to process".to_string()).into(),
			)
		}
		Update::Offer(offer_update) => {
			// We get an offer
			wallet_lock!(wallet_inst, w);
			let node_client = w.w2n_client().clone();
			let keychain = w.keychain(keychain_mask)?;
			let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;

			if trades::get_swap_trade(swap_id.as_str(), &skey).is_ok() {
				return Err( ErrorKind::Generic(format!("trade with SwapID {} already exist. Probably you already processed this message", swap_id)).into());
			}

			let mut swap_api = crate::swap::api::create_instance(
				&offer_update.secondary_currency,
				node_client.clone(),
			)?;
			// Creating Buyer context
			let context = create_context(
				&mut **w,
				keychain_mask,
				&mut swap_api,
				&keychain,
				offer_update.secondary_currency,
				false,
				None,
				0,
			)?;

			let (id, offer, secondary_update) = message.unwrap_offer()?;

			let swap = BuyApi::accept_swap_offer(
				&keychain,
				&context,
				id,
				offer,
				secondary_update,
				&node_client,
			)?;

			trades::store_swap_trade(&context, &swap, &skey)?;
			println!("You get an offer to swap BTC to MWC. SwapID is {}", swap.id);
			return Ok(());
		}
		_ => {
			wallet_lock!(wallet_inst, w);
			let node_client = w.w2n_client().clone();
			let keychain = w.keychain(keychain_mask)?;
			let skey = keychain.derive_key(0, &w.parent_key_id(), SwitchCommitmentType::None)?;

			let (context, mut swap) = trades::get_swap_trade(swap_id.as_str(), &skey)?;

			let swap_api =
				crate::swap::api::create_instance(&swap.secondary_currency, node_client)?;
			let tx_conf = swap_api.request_tx_confirmations(&keychain, &swap)?;
			let mut fsm = swap_api.get_fsm(&keychain, &swap);

			fsm.process(Input::IncomeMessage(message), &mut swap, &context, &tx_conf)?;
			trades::store_swap_trade(&context, &swap, &skey)?;
			println!("Processed message for SwapId {}", swap.id);
		}
	};
	Ok(())
}

// Local Helper method to create a context
fn create_context<'a, T: ?Sized, C, K>(
	wallet: &mut T,
	keychain_mask: Option<&SecretKey>,
	swap_api: &mut Box<dyn SwapApi<K> + 'a>,
	keychain: &K,
	secondary_currency: Currency,
	is_seller: bool,
	inputs: Option<Vec<(Identifier, Option<u64>, u64)>>,
	change_amount: u64,
) -> Result<Context, Error>
where
	T: WalletBackend<'a, C, K>,
	C: NodeClient + 'a,
	K: Keychain + 'a,
{
	let secondary_key_size =
		(**swap_api).context_key_count(keychain, secondary_currency, is_seller)?;
	let mut keys: Vec<Identifier> = Vec::new();

	for _ in 0..secondary_key_size {
		keys.push(wallet.next_child(keychain_mask)?);
	}

	let context = (**swap_api).create_context(
		keychain,
		secondary_currency,
		is_seller,
		inputs,
		change_amount,
		keys,
	)?;

	Ok(context)
}
