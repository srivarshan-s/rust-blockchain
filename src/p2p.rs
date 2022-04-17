// Import libraries
use super::{App, Block};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    identity,
    mdns::{Mdns, MdnsEvent},
    swarm::{NetworkBehaviourEventProcess, Swarm},
    NetworkBehaviour, PeerId,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::mpsc;

// Key pair for client in the network
pub static KEYS: Lazy<identity::Keypair> = Lazy::new(identity::Keypair::generate_ed25519);
// ID to identify the client in the network
pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
// CHAIN topic to subscribe and receive blockchains
pub static CHAIN_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("chains"));
// BLOCK topic to subscribe and receive bocks
pub static BLOCK_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("blocks"));

// Struct to define the blockchain sent and received in the network
#[derive(Debug, Serialize, Deserialize)]
pub struct ChainResponse {
    pub blocks: Vec<Block>, // The chain
    pub receiver: String,   // Receiver of the chain
}

// Struct used to trigger a chaim response from another node
#[derive(Debug, Serialize, Deserialize)]
pub struct LocalChainRequest {
    pub from_peer_id: String, // Peer ID of the sender
}

// Enum to handle input from user and chain requests/responses
pub enum EventType {
    LocalChainResponse(ChainResponse), // Chain response event
    Input(String),                     // User input event
    Init,
}

// Struct to hold the publisher/subscriber communication
// and the mDNS instance
#[derive(NetworkBehaviour)]
pub struct AppBehaviour {
    pub floodsub: Floodsub, // Publish/Subscribe instance
    pub mdns: Mdns,         // mDNS instance
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<ChainResponse>, // Channel for sending req/res event
    #[behaviour(ignore)]
    pub init_sender: mpsc::UnboundedSender<bool>, // Channel for sending init event
    #[behaviour(ignore)]
    pub app: App, // The Application
}
impl AppBehaviour {
    // Function to initialize AppBehaviour
    pub async fn new(
        app: App,
        response_sender: mpsc::UnboundedSender<ChainResponse>,
        init_sender: mpsc::UnboundedSender<bool>,
    ) -> Self {
        let mut behaviour = Self {
            app,
            floodsub: Floodsub::new(*PEER_ID),
            mdns: Mdns::new(Default::default())
                .await
                .expect("can create mdns"),
            response_sender,
            init_sender,
        };
        // Subscribe to CHAIN topic
        behaviour.floodsub.subscribe(CHAIN_TOPIC.clone());
        // Subscribe to BLOCK topic
        behaviour.floodsub.subscribe(BLOCK_TOPIC.clone());
        behaviour
    }
}
// Event handler for mDNS events
impl NetworkBehaviourEventProcess<MdnsEvent> for AppBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}
// Event handler for Pub/Sub events
impl NetworkBehaviourEventProcess<FloodsubEvent> for AppBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        if let FloodsubEvent::Message(msg) = event {
            // Check if event is ChainResponse
            if let Ok(resp) = serde_json::from_slice::<ChainResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    println!("INFO => Response from {}:", msg.source);
                    resp.blocks.iter().for_each(|r| println!("INFO => {:?}", r));
                    // Compare both local and received chain and choose chain
                    self.app.blocks = self.app.choose_chain(self.app.blocks.clone(), resp.blocks);
                }
            }
            // Check if event is LocalChainRequest
            else if let Ok(resp) = serde_json::from_slice::<LocalChainRequest>(&msg.data) {
                println!("INFO => Sending local chain to {}", msg.source);
                let peer_id = resp.from_peer_id;
                // Send ChainResponse with chain and receiver
                if PEER_ID.to_string() == peer_id {
                    if let Err(e) = self.response_sender.send(ChainResponse {
                        blocks: self.app.blocks.clone(),
                        receiver: msg.source.to_string(),
                    }) {
                        println!("ERROR => error sending response via channel, {}", e);
                    }
                }
            }
            // Check if event is a Block
            else if let Ok(block) = serde_json::from_slice::<Block>(&msg.data) {
                println!("INFO => Received new block from {}", msg.source);
                // Add block to chain
                self.app.try_add_block(block);
            }
        }
    }
}

// Function to list the number of peers connected to the network
pub fn get_list_peers(swarm: &Swarm<AppBehaviour>) -> Vec<String> {
    println!("INFO => Discovered Peers:");
    // Get the list of peers
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    // Initialize a Hash Set
    let mut unique_peers = HashSet::new();
    // Insert each peer into the Hash Set
    for peer in nodes {
        unique_peers.insert(peer);
    }
    // Return all the unique peers
    unique_peers.iter().map(|p| p.to_string()).collect()
}

// Function to print the peers connected to the network
pub fn handle_print_peers(swarm: &Swarm<AppBehaviour>) {
    let peers = get_list_peers(swarm); // Get Hash Set of connected unique peers
    peers.iter().for_each(|p| println!("INFO => {}", p)); // Print all peers
}

// Function to print the current block chain
pub fn handle_print_chain(swarm: &Swarm<AppBehaviour>) {
    println!("INFO => Local Blockchain:");
    // JSONify all the blocks
    let pretty_json =
        serde_json::to_string_pretty(&swarm.behaviour().app.blocks).expect("can jsonify blocks");
    // Print the JSONified blocks
    println!("INFO => {}", pretty_json);
}

// Function to mine a new block
pub fn handle_create_block(cmd: &str, swarm: &mut Swarm<AppBehaviour>) {
    // Extract string data from std input
    if let Some(data) = cmd.strip_prefix("create block ") {
        let behaviour = swarm.behaviour_mut(); // Get the App behaviour
        let latest_block = behaviour // Get the latest block
            .app
            .blocks
            .last()
            .expect("there is at least one block");
        let block = Block::new(
            // Create a new block with the given data
            latest_block.id + 1,
            latest_block.hash.clone(),
            data.to_owned(),
        );
        // JSONify the new block
        let json = serde_json::to_string(&block).expect("can jsonify request");
        // Add the block to the chain
        behaviour.app.blocks.push(block);
        // Bradcast the new block to the other nodes in the network
        println!("INFO => broadcasting new block");
        behaviour
            .floodsub
            .publish(BLOCK_TOPIC.clone(), json.as_bytes());
    }
}
