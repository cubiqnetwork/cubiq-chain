use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    core::upgrade,
    gossipsub::{
        Behaviour as Gossipsub, ConfigBuilder, GossipsubEvent, IdentTopic, MessageAuthenticity,
        MessageId, ValidationMode,
    },
    identify::{Behaviour as Identify, Config as IdentifyConfig, Event as IdentifyEvent},
    mdns::{Behaviour as Mdns, Event as MdnsEvent},
    noise::{AuthenticKeypair, Keypair as NoiseKeypair, NoiseConfig, X25519Spec},
    swarm::{Swarm, SwarmBuilder, SwarmEvent},
    tcp::TokioTcpConfig,
    yamux, Multiaddr, NetworkBehaviour, PeerId, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::{
    collections::HashMap,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;

/// Network messages passed between nodes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    BlockProposal(BlockProposal),
    Vote(Vote),
    ProofAnnouncement(String), // zkURL string
    Finalization(String),      // block hash
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProposal {
    pub block_hash: String,
    pub state_root: String,
    pub zkurl: String,
    pub transactions: Vec<Transaction>,
    pub proposer_id: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas_used: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vote {
    pub block_hash: String,
    pub voter_id: String,
    pub stake: u64,
    pub timestamp: u64,
    pub signature: String,
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
pub struct CubiqBehaviour {
    gossipsub: Gossipsub,
    mdns: Mdns,
    identify: Identify,
}

impl CubiqBehaviour {
    pub async fn new(local_key: libp2p::identity::Keypair) -> Result<Self> {
        let gossipsub_config = ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .max_transmit_size(1 * 1024 * 1024) // 1 MB
            .duplicate_cache_time(Duration::from_secs(60))
            .build()
            .expect("Valid gossipsub config");

        let mut gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )?;

        let topics = [
            "cubiq-blocks",
            "cubiq-votes",
            "cubiq-proofs",
            "cubiq-finalization",
        ];
        for topic in topics {
            gossipsub.subscribe(IdentTopic::new(topic))?;
        }

        let mdns = Mdns::new(Default::default()).await?;
        let identify = Identify::new(IdentifyConfig::new(
            "/cubiq/1.0.0".into(),
            local_key.public(),
        ));

        Ok(Self {
            gossipsub,
            mdns,
            identify,
        })
    }
}

/// Main P2P networking structure
pub struct P2PNetworking {
    pub swarm: Swarm<CubiqBehaviour>,
    pub peer_list: HashMap<PeerId, u64>, // peer id to last seen unix timestamp
    pub sender: mpsc::UnboundedSender<NetworkMessage>,
    pub receiver: mpsc::UnboundedReceiver<NetworkMessage>,
}

impl P2PNetworking {
    /// Create a new P2P networking instance
    pub async fn new() -> Result<Self> {
        let local_key = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);

        // Noise keys from libp2p identity keys
        let noise_keys = NoiseKeypair::<X25519Spec>::new()
            .into_authentic(&local_key)
            .expect("Noise key generation failed");

        let transport = TokioTcpConfig::new()
            .nodelay(true)
            .upgrade(upgrade::Version::V1)
            .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
            .multiplex(yamux::Config::default())
            .boxed();

        let behaviour = CubiqBehaviour::new(local_key.clone()).await?;

        let swarm = SwarmBuilder::with_executor(
            transport,
            behaviour,
            local_peer_id,
            Box::new(|fut| {
                tokio::spawn(fut);
            }),
        );

        let mut swarm = swarm;

        swarm.listen_on(Multiaddr::from_str("/ip4/0.0.0.0/tcp/0")?)?;

        let (sender, receiver) = mpsc::unbounded_channel();

        Ok(Self {
            swarm,
            peer_list: HashMap::new(),
            sender,
            receiver,
        })
    }

    /// Run the event loop for the networking layer
    pub async fn run(mut self) -> Result<()> {
        println!("Starting P2P networking event loop");

        loop {
            tokio::select! {
                event = self.swarm.next() => {
                    if let Some(event) = event {
                        self.handle_swarm_event(event).await?;
                    }
                },
                Some(message) = self.receiver.recv() => {
                    self.handle_outgoing_message(message).await?;
                },
            }
        }
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<<CubiqBehaviour as NetworkBehaviour>::Event, anyhow::Error>,
    ) -> Result<()> {
        use CubiqBehaviourEvent::*;
        match event {
            SwarmEvent::Behaviour(Gossipsub(event)) => self.handle_gossipsub_event(event).await?,
            SwarmEvent::Behaviour(Mdns(event)) => self.handle_mdns_event(event)?,
            SwarmEvent::Behaviour(Identify(event)) => {
                println!("Identify event: {:?}", event);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_gossipsub_event(&mut self, event: GossipsubEvent) -> Result<()> {
        if let GossipsubEvent::Message {
            propagation_source,
            message_id: _,
            message,
        } = event
        {
            if let Ok(net_msg) = serde_json::from_slice::<NetworkMessage>(&message.data) {
                println!(
                    "Received message from {:?}: {:?}",
                    propagation_source, net_msg
                );
                // TODO: forward into consensus or other logic
            } else {
                eprintln!("Failed to deserialize network message");
            }
        }
        Ok(())
    }

    fn handle_mdns_event(&mut self, event: MdnsEvent) -> Result<()> {
        use MdnsEvent::*;
        match event {
            Discovered(list) => {
                for (peer_id, _addr) in list {
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                    self.peer_list.insert(peer_id, now);
                    println!("mDNS Discovered peer: {}", peer_id);
                }
            }
            Expired(list) => {
                for (peer_id, _addr) in list {
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    self.peer_list.remove(&peer_id);
                    println!("mDNS Expired peer: {}", peer_id);
                }
            }
        }
        Ok(())
    }

    async fn handle_outgoing_message(&mut self, message: NetworkMessage) -> Result<()> {
        let topic = match &message {
            NetworkMessage::BlockProposal(_) => "cubiq-blocks",
            NetworkMessage::Vote(_) => "cubiq-votes",
            NetworkMessage::ProofAnnouncement(_) => "cubiq-proofs",
            NetworkMessage::Finalization(_) => "cubiq-finalization",
        };

        let topic = IdentTopic::new(topic);
        let data = serde_json::to_vec(&message)?;

        self.swarm.behaviour_mut().gossipsub.publish(topic, data)?;

        Ok(())
    }
}

/// The unified behaviour event emitted from combined behaviours
#[derive(Debug)]
pub enum CubiqBehaviourEvent {
    Gossipsub(GossipsubEvent),
    Mdns(MdnsEvent),
    Identify(IdentifyEvent),
}

impl From<GossipsubEvent> for CubiqBehaviourEvent {
    fn from(event: GossipsubEvent) -> Self {
        CubiqBehaviourEvent::Gossipsub(event)
    }
}

impl From<MdnsEvent> for CubiqBehaviourEvent {
    fn from(event: MdnsEvent) -> Self {
        CubiqBehaviourEvent::Mdns(event)
    }
}

impl From<IdentifyEvent> for CubiqBehaviourEvent {
    fn from(event: IdentifyEvent) -> Self {
        CubiqBehaviourEvent::Identify(event)
    }
}
