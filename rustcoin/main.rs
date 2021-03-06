extern crate consensus;
extern crate wallet;

extern crate rand;
use rand::thread_rng;

extern crate curve25519_dalek;
use curve25519_dalek::scalar::Scalar;

extern crate merlin;
use merlin::Transcript;

extern crate bulletproofs;
use bulletproofs::{BulletproofGens, PedersenGens, RangeProof};

fn main() {
	
}


fn init_chain() {
	construct_genesis();
}

fn construct_genesis() -> Vec<u8>{
	let mut version: Vec<u8> = vec![0,1];
	let mut prev_block_hash: Vec<u8> = vec![0;32];
	let mut all_tx_hash: Vec<u8> = vec![0;32];
	let mut nonce: Vec<u8> = vec![0;4];
	let mut coinbase_destination: Vec<u8> = vec![0;32];

	//construct the block
	let mut block: Vec<u8> = Vec::new();

	//block header
	block.append(&mut version);
	block.append(&mut prev_block_hash);
	block.append(&mut all_tx_hash);
	block.append(&mut nonce);

	//first tx (only coinbase)
	block.append(&mut coinbase_destination);

	consensus::insert_block(block.clone());
	return block;
}


//TODO:
// - Miner
// - tx mempool
// - serialize tx
// - merkle tree
// - networking