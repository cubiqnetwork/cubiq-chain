use core::consensus::{QubeNode, BlockProposal, Transaction};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let node = QubeNode::new(
        "node1".to_string(),
        10_000,
        vec!["https://zkproof.cubiq.dev".to_string()],
    ).await;

    let (mut proposal_tx, proposal_rx) = mpsc::channel(10);
    let (vote_tx, mut vote_rx) = mpsc::channel(10);

    // Spawn node handler
    tokio::spawn(async move {
        node.run(proposal_rx, vote_tx).await;
    });

    // Dummy block proposal
    let proposal = BlockProposal {
        block_hash: "0xabc...".to_string(),
        state_root: "0xbeef...".to_string(),
        zkurl: "zk://prover@domain.com/block1#v1&gzip&stark".to_string(),
        transactions: vec![],
        proposer_id: "node1".to_string(),
        timestamp: 123456789,
    };
    proposal_tx.send(proposal).await.ok();

    // Votes returned on vote_rx
    if let Some(vote) = vote_rx.recv().await {
        println!("Voted block: {:?}", vote);
    }
}
