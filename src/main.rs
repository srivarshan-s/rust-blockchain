// Import libraries
use chrono::prelude::*;
use libp2p::{
    core::upgrade,
    futures::StreamExt,
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::{Swarm, SwarmBuilder},
    tcp::TokioTcpConfig,
    Transport,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::{
    io::{stdin, AsyncBufReadExt, BufReader},
    select, spawn,
    sync::mpsc,
    time::sleep,
};

// Import module
mod p2p;

// Requirement for nonce hash (difficulty)
const DIFFICULTY_PREFIX: &str = "00";

// Helper function to convert the hash value to a binary value
fn hash_to_binary(hash: &[u8]) -> String {
    let mut bin: String = String::default();
    for ch in hash {
        bin.push_str(&format!("{:b}", ch));
    }
    bin
}

// Struct to hold the Application state
pub struct App {
    pub blocks: Vec<Block>, // List of blocks (blockchain)
}

impl App {
    // Function to create a new blockchain
    fn new() -> Self {
        Self { blocks: vec![] } // Return empty list
    }

    // Function to conceive the initial block
    fn genesis(&mut self) {
        // Create the genesis block with pre-defined attributes
        /* let genesis_block = Block {
            id: 0,
            time_stamp: Utc::now().timestamp(),
            prev_hash: String::from("genesis"),
            data: String::from("genesis!"),
            nonce: 2863,
            hash: "0000f816a87f806bb0073dcf026a64fb40c946b5abee2573702828694d5b4c43".to_string(),
        }; */
        let genesis_block = Block::new(0, "NULL".to_string(), "GENESIS BLOCK".to_string());
        self.blocks.push(genesis_block); // Push the genesis block to the blockchain
    }

    // Function to check the validity of the Block
    fn validate_block(&self, block: &Block, prev_block: &Block) -> bool {
        // Check if the previous hash value matches the has value of the last block
        if block.prev_hash != prev_block.hash {
            println!("WARNING => Block with id: {} has wrong previous hash", block.id);
            return false;
        }
        // Check if difficulty is satisified (requirement)
        else if !hash_to_binary(&hex::decode(&block.hash).expect("can decode from hex"))
            .starts_with(DIFFICULTY_PREFIX)
        {
            println!("WARNING => Block with id: {} has invalid difficulty", block.id);
            return false;
        }
        // Check if the block ID is correct
        else if block.id != prev_block.id + 1 {
            println!(
                "WARNING => Block with id: {} is not the next block after the latest block: {}",
                block.id, prev_block.id,
            );
            return false;
        }
        // Check if the hash of the block matches the hash attribute
        else if hex::encode(calculate_hash(
            block.id,
            block.time_stamp,
            &block.prev_hash,
            &block.data,
            block.nonce,
        )) != block.hash
        {
            println!("WARNING => block with id: {} has invalid hash", block.id);
            return false;
        }
        true
    }

    // Add a valid Block to the existing chain
    fn try_add_block(&mut self, block: Block) {
        let latest_block = self.blocks.last().expect("there is atleast oneblock");
        // Check validity of the Block
        if self.validate_block(&block, latest_block) {
            self.blocks.push(block);
        } else {
            // error!("could not add block - invalid");
            println!("ERROR => Could not add block - invalid")
        }
    }

    // Function to validate a chaim
    fn validate_chain(&self, chain: &[Block]) -> bool {
        // Iterate over every consecutive pair of blocks in the chain
        for i in 0..chain.len() {
            if i == 0 {
                continue;
            }
            // Get the first block
            let first = chain.get(i).expect("has to exist");
            // Get the second Block
            let second = chain.get(i).expect("has to exist");
            // Validate both Blocks
            if !self.validate_block(second, first) {
                return false;
            }
        }
        true
    }

    // Function to choose the longest valid chain
    fn choose_chain(&mut self, local: Vec<Block>, remote: Vec<Block>) -> Vec<Block> {
        // Validate the local chain
        let validate_local = self.validate_chain(&local);
        // Validate the remote chain
        let validate_remote = self.validate_chain(&remote);
        // If both chains are valid, choose the longer chain
        if validate_local && validate_remote {
            if local.len() >= remote.len() {
                local
            } else {
                remote
            }
        // Check if local chain is invalid
        } else if validate_remote && !validate_local {
            remote // Return remote chain
        }
        // Check if remote chain is invalid
        else if !validate_remote && validate_local {
            local // Return local chain
        }
        // Panic if both chains are invalid
        else {
            panic!("local and remote chains are both invalid!");
        }
    }
}

// Function to mine a block (calculate nonce and hash of Block)
fn mine_block(id: u64, time_stamp: i64, prev_hash: &str, data: &str) -> (u64, String) {
    // info!("mining block...");
    println!("INFO => Mining block...");    
    // Initialize nonce value
    let mut nonce = 0;
    // Infinite loop till nonce is calculated
    loop {
        /* if nonce % 100000 == 0 {
            info!("nonce: {}", nonce);
        } */
        // Calculate the hash value
        let hash = calculate_hash(id, time_stamp, prev_hash, data, nonce);
        // Convert hash value to binary
        let binary_hash = hash_to_binary(&hash);
        // Check if hash value satisifies requirement (difficulty)
        if binary_hash.starts_with(DIFFICULTY_PREFIX) {
            println!(
                "INFO => Block mined! \nnonce: {} \nhash: {} \nbinary hash: {}",
                nonce,
                hex::encode(&hash),
                binary_hash
            );
            // If satisfied return the hash value and the nonce value
            return (nonce, hex::encode(hash));
        }
        // Increment the nonce value
        nonce += 1;
    }
}

// Utility function to calculate the hash value
fn calculate_hash(id: u64, timestamp: i64, previous_hash: &str, data: &str, nonce: u64) -> Vec<u8> {
    // Serialize the Block attributes into JSON format
    let data = serde_json::json!({
        "id": id,
        "previous_hash": previous_hash,
        "data": data,
        "timestamp": timestamp,
        "nonce": nonce
    });
    // Initialize hasher
    let mut hasher = Sha256::new();
    // Calculate the hash value
    hasher.update(data.to_string().as_bytes());
    // Return hash value
    hasher.finalize().as_slice().to_owned()
}

// Struct to represent a Block
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub id: u64,           // ID of the Block
    pub hash: String,      // The hash value of the Block
    pub prev_hash: String, // The hash value of the previous Block
    pub time_stamp: i64,   // The time stamp of when the Block was conceived
    pub data: String,      // The data that the block holds
    pub nonce: u64,        // The nonce value associated with the Block
}

impl Block {
    // Function to create a new block
    pub fn new(id: u64, prev_hash: String, data: String) -> Self {
        let now = Utc::now(); // Get the timestamp
                              // Calculate the nonce and the hash
        let (nonce, hash) = mine_block(id, now.timestamp(), &prev_hash, &data);
        // Create a new Block
        Self {
            id,
            hash,
            time_stamp: now.timestamp(),
            prev_hash,
            data,
            nonce,
        }
    }
}

// Main Function
#[tokio::main]
async fn main() {
    // Intitialize the logger
    pretty_env_logger::init();

    // Print PEER ID
    println!("INFO => Peer Id: {}", p2p::PEER_ID.clone());

    // Initialize Response Event Channel
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();
    // Initialize Initialization Event Channel
    let (init_sender, mut init_rcv) = mpsc::unbounded_channel();

    // Initialize Authentication Key pair
    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&p2p::KEYS)
        .expect("can create auth keys");

    // Initialize P2P transport
    let transp = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    // Intialize App Behaviour
    let behaviour = p2p::AppBehaviour::new(App::new(), response_sender, init_sender.clone()).await;

    // Initialize Swarm (entity that runs the network stack)
    let mut swarm = SwarmBuilder::new(transp, behaviour, *p2p::PEER_ID)
        .executor(Box::new(|fut| {
            spawn(fut);
        }))
        .build();

    // Initialize standard input reader
    let mut stdin = BufReader::new(stdin()).lines();

    // Start the network stack
    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can get a local socket"),
    )
    .expect("swarm can be started");

    // Start asynchronous coroutine
    spawn(async move {
        sleep(Duration::from_secs(1)).await; // Wait for 1 second
        println!("INFO => Sending init event");
        init_sender.send(true).expect("can send init event"); // Send Initialization trigger
    });

    // Infinite loop
    loop {
        let evt = {
            select! { // macro used to race multiple async functions
                // Read input from console
                line = stdin.next_line() => Some(p2p::EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                // Create event for input
                response = response_rcv.recv() => { // Listen on response channel
                    Some(p2p::EventType::LocalChainResponse(response.expect("response exists")))
                },
                _init = init_rcv.recv() => { // Listen on init channel
                    Some(p2p::EventType::Init)
                }
                event = swarm.select_next_some() => { // Other event (can be ignored)
                    // info!("Unhandled Swarm Event: {:?}", event);
                    None
                },
            }
        };

        // Handle the created events
        if let Some(event) = evt {
            match event {
                // Check if it is an Init Event
                p2p::EventType::Init => {
                    let peers = p2p::get_list_peers(&swarm); // Obtain the connected peers
                    swarm.behaviour_mut().app.genesis(); // Create the genesis block
                                                         // Print the connected nodes
                    println!("INFO => Connected nodes: {}", peers.len());
                    if !peers.is_empty() {
                        // Check if peers are connected
                        // Request chain from last peer
                        let req = p2p::LocalChainRequest {
                            from_peer_id: peers
                                .iter()
                                .last()
                                .expect("at least one peer")
                                .to_string(),
                        };
                        // JSONify the request
                        let json = serde_json::to_string(&req).expect("can jsonify request");
                        // Publish the request to the network
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .publish(p2p::CHAIN_TOPIC.clone(), json.as_bytes());
                    }
                }
                // Check if it is Chain request event
                p2p::EventType::LocalChainResponse(resp) => {
                    // JSONify the response (the chain)
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
                    // Publish the chain to the network
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(p2p::CHAIN_TOPIC.clone(), json.as_bytes());
                }
                // Check of it is user input event
                p2p::EventType::Input(line) => match line.as_str() {
                    "list peers" => p2p::handle_print_peers(&swarm), // Print the peers connected to the network
                    cmd if cmd.starts_with("print chain") => p2p::handle_print_chain(&swarm), // Print the current chain
                    cmd if cmd.starts_with("create block") => {
                        p2p::handle_create_block(cmd, &mut swarm)
                    } // Mine new block
                    cmd if cmd.starts_with("exit") => break,
                    // _ => error!("unknown command"), // Other commands
                    _ => println!("ERROR => unknown command"), // Other commands
                },
            }
        }
    }
}
