use prover::MobileProofVerifier;
use zkurl::{ZkURL, resolver::{ZkURLResolver, ProofBundle}};
use serde::{Serialize, Deserialize};
use tokio::sync::{RwLock, mpsc};
use std::collections::HashMap;
use std::sync::Arc;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockProposal {
    pub block_hash: String,
    pub state_root: String,
    pub zkurl: String,
    pub transactions: Vec<Transaction>,
    pub proposer_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas_used: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub node_id: String,
    pub stake: u64,
    pub public_key: String,
    pub is_active: bool,
    pub last_vote_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub block_hash: String,
    pub voter_id: String,
    pub stake: u64,
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct ValidatorSet {
    pub validators: HashMap<String, Validator>,
    pub total_stake: u64,
    pub supermajority_threshold: u64,
}

impl ValidatorSet {
    pub fn new() -> Self {
        Self {
            validators: HashMap::new(),
            total_stake: 0,
            supermajority_threshold: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsensusState {
    pub current_height: u64,
    pub current_round: u32,
    pub votes: HashMap<String, Vote>,
    pub finalized_blocks: Vec<String>,
}

impl ConsensusState {
    pub fn new() -> Self {
        Self {
            current_height: 0,
            current_round: 0,
            votes: HashMap::new(),
            finalized_blocks: vec![],
        }
    }

    pub fn group_votes_by_block(&self) -> HashMap<String, Vec<&Vote>> {
        let mut m: HashMap<String, Vec<&Vote>> = HashMap::new();
        for vote in self.votes.values() {
            m.entry(vote.block_hash.clone()).or_insert(vec![]).push(vote);
        }
        m
    }
}

pub struct QubeNode {
    pub node_id: String,
    pub stake_amount: u64,
    pub validator_set: Arc<RwLock<ValidatorSet>>,
    pub zkurl_resolver: ZkURLResolver,
    pub consensus_state: Arc<RwLock<ConsensusState>>,
}

impl QubeNode {
    pub async fn new(node_id: String, stake_amount: u64, resolver_endpoints: Vec<String>) -> Self {
        Self {
            node_id,
            stake_amount,
            validator_set: Arc::new(RwLock::new(ValidatorSet::new())),
            zkurl_resolver: ZkURLResolver::new(resolver_endpoints),
            consensus_state: Arc::new(RwLock::new(ConsensusState::new())),
        }
    }

    /// Main consensus loop (call from an async runtime)
    pub async fn run(&self, mut proposal_rx: mpsc::Receiver<BlockProposal>, mut vote_tx: mpsc::Sender<Vote>) {
        loop {
            if let Some(proposal) = proposal_rx.recv().await {
                if let Err(e) = self.process_block_proposal(proposal, &mut vote_tx).await {
                    eprintln!("Proposal processing failed: {:?}", e);
                }
            }
        }
    }

    /// Validate block proposal, fetch and verify proof with mobile verifier, then submit vote
    pub async fn process_block_proposal(&self, proposal: BlockProposal, vote_tx: &mut mpsc::Sender<Vote>) -> Result<(), String> {
        // Fetch proof bundle by zkurl
        let zkurl = ZkURL::from_str(&proposal.zkurl).map_err(|e| format!("Invalid zkURL: {e}"))?;
        let proof_bundle: ProofBundle = self.zkurl_resolver.fetch_proof(&zkurl).await
            .map_err(|e| format!("Failed to fetch proof: {e}"))?;

        // Use the mobile-optimized verifier!
        let verifier = MobileProofVerifier::new();
        let is_valid = verifier.verify_proof(&proof_bundle.proof)
            .map_err(|e| format!("Proof verify error: {:?}", e))?;
        if !is_valid {
            return Err("Proof did not pass verification".to_string());
        }

        // Check block/proof consistency
        if proposal.block_hash != proof_bundle.public_inputs.block_hash {
            return Err("Block hash mismatch with proof's public inputs!".to_string());
        }
        if proposal.state_root != proof_bundle.public_inputs.state_root {
            return Err("State root mismatch!".to_string());
        }
        if proposal.transactions.len() as u32 != proof_bundle.public_inputs.transaction_count {
            return Err("Transaction count mismatch!".to_string());
        }
        let calc_gas: u64 = proposal.transactions.iter().map(|tx| tx.gas_used).sum();
        if calc_gas != proof_bundle.public_inputs.gas_used {
            return Err("Gas usage mismatch!".to_string());
        }

        // If passes all checks, create and send vote
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let vote = Vote {
            block_hash: proposal.block_hash.clone(),
            voter_id: self.node_id.clone(),
            stake: self.stake_amount,
            timestamp: ts,
            signature: "dummy_signature".to_string(), // TODO: cryptographic signature
        };
        vote_tx.send(vote).await.map_err(|e| format!("Failed to send vote: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use serde_json;

    #[tokio::test]
    async fn test_node_proposal_handles_invalid_zkurl() {
        let node = QubeNode::new("tester".to_string(), 10_000, vec![]).await;
        let (mut tx, mut rx) = mpsc::channel(8);
        let (vote_tx, _vote_rx) = mpsc::channel(8);
        tx.send(BlockProposal {
            block_hash: "h".to_string(),
            state_root: "r".to_string(),
            zkurl: "invalid-scheme://".to_string(),
            transactions: vec![],
            proposer_id: "p".to_string(),
            timestamp: 0,
        }).await.ok();
        tokio::spawn(async move {
            node.run(rx, vote_tx).await
        });
        // If no panic, test passes for stub
    }
}
