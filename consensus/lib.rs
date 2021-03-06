//Consensus provides handlers for the verification of;
// - new blocks
// - new transactions 
//and writes them to the blockchain db or the mempool
//TODO -- Verify signature in transaction (line 88)
//TODO -- need to store verified tx in memory
extern crate blocks;
extern crate utils;
extern crate rkv;
extern crate sha2;
extern crate db;


use sha2::{Sha256, Digest};

use rkv::{Manager, Rkv, Store, Value};
use std::fs;
use std::path::Path;

struct verify_tx_return_values {
	pc: usize,
	utxos: Vec<Vec<u8>>
}

///Verify new blocks that come in and write to db
pub fn verify_new_block(block: Vec<u8>) -> Result<bool, String> {
	if block.len() > 1_000_000 {
		return Err(String::from("ERROR: VERIFY BLOCK: `block` is too large"));
	}

	let mut block_header: [u8; 70] = [0; 70];
	block_header.copy_from_slice(&block[0..70]); //Get blockheader into array
	let version = &block_header[0..2];

	//Version check
	if version != [0,1] {
		return Err(String::from("ERROR: VERIFY BLOCK: Incompatable `version`"));
	}

	//Nonce check
	if !utils::hash_satisfies_difficulty(&block_header.to_vec()) {
		return Err(String::from("ERROR: VERIFY BLOCK: Invalid `nonce`"));
	}

	//Matches with previous block check
	let prev_block_hash = &block_header[2..34].to_vec();
	if prev_block_hash != &utils::get_prev_block_hash() {
		return Err(String::from("ERROR: VERIFY BLOCK: `prev_block_hash` does not match"));
	}

	//Verify tx_hash matches hash of all tx
	let tx_hash = block_header[34..66].to_vec();
	let all_tx_bytes = block[70..].to_vec(); //we go from 66 to 70 because [66..70] is the nonce
	if tx_hash != utils::hash(&all_tx_bytes) {
		return Err(String::from("ERROR: VERIFY BLOCK: `tx_hash` does not match"));
	}

	let coinbase_tx_vec = all_tx_bytes[0..33].to_vec();

	//Verify all tx (excluding coinbase)
	let mut program_counter: usize = 33;
	let mut valid_tx_vector: Vec<Vec<u8>>  = Vec::new();
	while program_counter < all_tx_bytes.len() {
		match verify_tx(all_tx_bytes[program_counter..].to_vec(), true) { 
			Ok(mut i) => { 
				valid_tx_vector.append(&mut i.utxos);
				program_counter += i.pc; 
			}, 
			Err(e) => { return Err(e); }
		}
	}

	add_to_utxo_set(&mut valid_tx_vector, &mut block_header.to_vec());
	add_coinbase_to_utxo_set(coinbase_tx_vec);
	insert_block(block);
	return Ok(true);
}

//writes the coinbase tx into the utxo set
pub fn add_coinbase_to_utxo_set(coinbase_dest: Vec<u8>) {
	let mut version: Vec<u8> = vec![0,1];
	let mut to_dest: Vec<u8> = coinbase_dest;
	let mut amount: Vec<u8> = vec![0,0,0xff,0]; //block reward
	let mut raw_tx: Vec<u8> = Vec::new(); //raw_tx to be stored in the utxo set
	raw_tx.append(&mut version);
	raw_tx.append(&mut amount);
	raw_tx.append(&mut to_dest);

	let tx_hash = utils::hash(&raw_tx); //Key to reference raw_tx

	let path = Path::new("./db/store");
	let created_arc = Manager::singleton().write().unwrap().get_or_create(path, Rkv::new).unwrap();
	let env = created_arc.read().unwrap();
	let store: Store = env.open_or_create_default().unwrap(); 

	let mut writer = env.write().unwrap(); //create write tx
	writer.put(&store, tx_hash.clone(),  &Value::Blob(&raw_tx)).unwrap();
	writer.commit().unwrap();
}

//writes a standard tx into the utxo set after it has been verified in a block
//key value = hash(utxo, blockheader, index in block)
pub fn add_to_utxo_set(utxo_to_add: &mut Vec<Vec<u8>>, block_header: &mut Vec<u8>) {
	let mut digest: Vec<u8> = Vec::new();
	for i in 0..utxo_to_add.len() {
		//hash utxo + block header
		digest.append(&mut utxo_to_add[i]);
		digest.append(block_header);
		digest.push(i as u8); //index
		let utxo_hash = utils::hash(&digest); //utxo id/utxo hash - this is what we want to write to db
		
		let path = Path::new("./db/store");
		let created_arc = Manager::singleton().write().unwrap().get_or_create(path, Rkv::new).unwrap();
		let env = created_arc.read().unwrap();
		let store: Store = env.open_or_create_default().unwrap(); 

		let mut writer = env.write().unwrap(); //create write tx
		writer.put(&store, utxo_hash,  &Value::Blob(&utxo_to_add[i])).unwrap();
		writer.commit().unwrap();

		digest = Vec::new();
	}
}

//insert block into db after it has been verified
//key value = hash(blockhash)
pub fn insert_block(block: Vec<u8>) {
	let path = Path::new("./db/store");
	let created_arc = Manager::singleton().write().unwrap().get_or_create(path, Rkv::new).unwrap();
	let env = created_arc.read().unwrap();
	let store: Store = env.open_or_create_default().unwrap(); 

	let block_hash = utils::hash(&block);

	let mut writer = env.write().unwrap(); //create write tx
	writer.put(&store, block_hash.clone(),  &Value::Blob(&block)).unwrap();

	//store the last block hash - key is vec![1]
	writer.put(&store, vec![1],  &Value::Blob(&block_hash.clone())).unwrap();
	writer.commit().unwrap();
}


//verifies a raw tx
fn verify_tx(all_tx_bytes: Vec<u8>, is_Block: bool) -> Result<verify_tx_return_values, String> {
	let version = &all_tx_bytes[0..2]; //needs to be changed to counter
	if version != [0,1] { //this needs to be changed to 2 bytes
		return Err(String::from("VERIFY TX ERROR: Incompatable `version` in tx"));
	}

	let path = Path::new("./db/store");
	let created_arc = Manager::singleton().write().unwrap().get_or_create(path, Rkv::new).unwrap();
	let env = created_arc.read().unwrap();
	let store: Store = env.open_or_create_default().unwrap(); 

	let mut writer = env.write().unwrap(); //create write tx
    let reader = env.read().expect("reader");

	let input_count = all_tx_bytes[2];
	let mut sum_inputs: u64 = 0;
	let mut s: usize = 2; //counter for position in bytecode of block
	for i in 0..input_count {
		let utxo_tx_hash = &all_tx_bytes[s..s+32];
		s+=32;

		//read the utxo from the db
		let utxo_ref = reader.get(&store, utxo_tx_hash).unwrap();
		//where the utxo will be stored
		let mut utxo: Vec<u8> = Vec::new();
		match utxo_ref {
			Some(i) => {
				match i {
					//check if utxo is 39 bytes long
					//2 byte version
					//4 byte value
					//33 byte compressed pubkey
					rkv::Value::Blob(i) if i.to_vec().len() == 39 => {
						utxo = i.to_vec();
					},
					_ => { return Err(String::from("Invalid `utxo` referenced in input")); }
				}
			},
			None => { return Err(String::from("Invalid `utxo` referenced in input")); }
		}

		//seperate the owner(33 byte pubkey) from the value
		let utxo_value = utxo[2..6].to_vec();
		let utxo_owner = utxo[6..39].to_vec(); //compressed pubkey format means its 33 bytes (first byte being 0x02 or 0x03);

		//signautre size (this will either be 67-69 bytes)
		let sig_size = all_tx_bytes[s..s+1].to_vec()[0] as usize;
		s+=1;

		//get the signature
		let signature = &all_tx_bytes[s..s+sig_size];

		//verify the signature is a valid ecdsa
		//TODO: Can be changed to EdCSA?
		utils::verify_signature(utxo_owner.to_vec(), signature.to_vec(), utxo_tx_hash.to_vec());
		s+=sig_size;
	}

	let output_count = all_tx_bytes[s];
	s+=1;
	let mut sum_outputs: u64 = 0; //Amount to be spent
	let mut utxo_vector: Vec<Vec<u8>> = Vec::new();
	for i in 0..output_count {
		let value_array = &all_tx_bytes[s..s+6];
		s+=6;
		
		let mut cache_sum: u64 = 0;
		for i in 0..value_array.len() { //Get the byte array of `sum_outputs` and cast it to a u64
			cache_sum = cache_sum * 16 + value_array[i] as u64;
		}
		sum_outputs += cache_sum;

		if sum_outputs > sum_inputs {
			return Err(String::from("Sum of outputs exceeds the inputs"));
		}

		let mut to_pub_key = all_tx_bytes[s..s+33].to_vec();
		let mut raw_utxo: Vec<u8> = Vec::new();

		raw_utxo.append(&mut [0,1].to_vec());
		raw_utxo.append(&mut value_array.to_vec());
		raw_utxo.append(&mut to_pub_key);
		utxo_vector.push(raw_utxo); //vector of utxos to be returned so that we can store in utxo set if valid
		s+=33; 
	}

	//If the tx is not inside a block send it to the mempool
	if !is_Block {
		//key for referencing the tx pool vec![1,2]
		//each tx in the mempool will be stored here
		//first we need to read then reinsert the read value
		let mut current_mempool = match reader.get(&store, &vec![1,2]).unwrap().unwrap() {
			Value::Blob(i) => i.to_vec(),
			_ => { return Err(String::from("ERROR VERIFY TX: Invalid mempool values")) }
		}; //this needs to be changed to if let
		//edit the current mempool and insert the utxos that have been verified
		for i in 0..utxo_vector.len() {
			current_mempool.append(&mut utxo_vector[i]);
		}
		//create write of new mempool to key value vec![1,2]
		writer.put(&store, vec![1,2],  &Value::Blob(&current_mempool)).unwrap();
		writer.commit().unwrap();
	}

	let result = verify_tx_return_values {
		pc: s, 
		utxos: utxo_vector
	};

	return Ok(result); //Return the program counter inside the block + the utxos to write to the chain
}



