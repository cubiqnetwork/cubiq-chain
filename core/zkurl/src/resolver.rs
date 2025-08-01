use crate::{ZkURL, ZkURLError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Structure representing a proof bundle retrieved from the network.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProofBundle {
    pub proof: Vec<u8>,              // Actual proof bytes
    pub public_inputs: PublicInputs, // Public inputs related to proof
    pub signature: String,           // Cryptographic signature of proof
    pub prover_id: String,           // Prover identifier
    pub timestamp: u64,              // Unix timestamp of proof creation
    pub metadata: ProofMetadata,     // Metadata about the proof
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicInputs {
    pub block_hash: String,
    pub state_root: String,
    pub gas_used: u64,
    pub transaction_count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofMetadata {
    pub version: String,
    pub compression: Option<String>,
    pub size_bytes: usize,
}

/// Resolver that fetches proofs using zkURLs with fallback endpoints.
pub struct ZkURLResolver {
    client: Client,
    fallback_endpoints: Vec<String>,
    timeout: Duration,
}

impl ZkURLResolver {
    /// Create a new resolver with fallback endpoints.
    pub fn new(fallback_endpoints: Vec<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_millis(5000))
                .build()
                .expect("Failed to build HTTP client"),
            fallback_endpoints,
            timeout: Duration::from_millis(5000),
        }
    }

    /// Fetches the proof bundle referenced by the zkURL.
    ///
    /// Tries the primary URL constructed from zkURL, then fallback endpoints.
    pub async fn fetch_proof(&self, zkurl: &ZkURL) -> Result<ProofBundle, ZkURLError> {
        let primary_url = self.construct_url(zkurl);
        
        // Try main endpoint first
        if let Ok(bundle) = self.fetch_from_endpoint(&primary_url).await {
            if self.verify_proof_bundle(&bundle).await? {
                return Ok(bundle);
            }
        }

        // Fallback endpoints
        for endpoint in &self.fallback_endpoints {
            let fallback_url = format!("{}/proof/{}", endpoint, zkurl.proof_id);
            if let Ok(bundle) = self.fetch_from_endpoint(&fallback_url).await {
                if self.verify_proof_bundle(&bundle).await? {
                    return Ok(bundle);
                }
            }
        }
        
        Err(ZkURLError::ParseError("Proof not found at any endpoint".into()))
    }

    /// Helper to fetch proof bundle JSON from URL.
    async fn fetch_from_endpoint(&self, url: &str) -> Result<ProofBundle, ZkURLError> {
        let response = self.client.get(url).timeout(self.timeout).send().await
            .map_err(|e| ZkURLError::ParseError(format!("Network error: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(ZkURLError::ParseError(format!("HTTP error: {}", response.status())));
        }

        let proof_bundle = response.json::<ProofBundle>().await
            .map_err(|e| ZkURLError::ParseError(format!("Failed to parse JSON: {}", e)))?;

        Ok(proof_bundle)
    }

    /// Verify signature, timestamp, and constraints on the proof bundle.
    async fn verify_proof_bundle(&self, bundle: &ProofBundle) -> Result<bool, ZkURLError> {
        // Stub: Implement actual cryptographic signature verification here
        // For now, always return true unless conditions fail

        // Check timestamp recency: max 1 hour old
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ZkURLError::ParseError(format!("System time error: {}", e)))?
            .as_secs();

        if current_time < bundle.timestamp || current_time - bundle.timestamp > 3600 {
            return Ok(false);
        }

        // Proof size limit (e.g., max 5 MB)
        if bundle.proof.len() > 5_000_000 {
            return Ok(false);
        }

        // TODO: Add signature verification logic here (crypto verification)

        Ok(true)
    }

    /// Construct the primary proof URL based on zkURL format:
    /// - If prover_id is present: https://{domain_or_hash}/proof/{proof_id}
    /// - Else (content-addressed): https://ipfs.io/ipfs/{domain_or_hash}
    fn construct_url(&self, zkurl: &ZkURL) -> String {
        if let Some(_prover_id) = &zkurl.prover_id {
            format!(
                "https://{}/proof/{}",
                zkurl.domain_or_hash,
                zkurl.proof_id
            )
        } else {
            format!(
                "https://ipfs.io/ipfs/{}",
                zkurl.domain_or_hash
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_construct_url_with_prover() {
        let zkurl = ZkURL {
            prover_id: Some("proverABC".to_string()),
            domain_or_hash: "example.com".to_string(),
            proof_id: "block99".to_string(),
            metadata: None,
        };
        let resolver = ZkURLResolver::new(vec![]);
        let url = resolver.construct_url(&zkurl);
        assert_eq!(url, "https://example.com/proof/block99");
    }

    #[tokio::test]
    async fn test_construct_url_without_prover() {
        let zkurl = ZkURL {
            prover_id: None,
            domain_or_hash: "QmHash123".to_string(),
            proof_id: "proofX".to_string(),
            metadata: None,
        };
        let resolver = ZkURLResolver::new(vec![]);
        let url = resolver.construct_url(&zkurl);
        assert_eq!(url, "https://ipfs.io/ipfs/QmHash123");
    }

    #[tokio::test]
    async fn test_verify_proof_bundle_fails_on_old_timestamp() {
        let old_bundle = ProofBundle {
            proof: vec![0u8; 10],
            public_inputs: PublicInputs {
                block_hash: String::new(),
                state_root: String::new(),
                gas_used: 0,
                transaction_count: 0,
            },
            signature: String::new(),
            prover_id: "prover".to_string(),
            timestamp: 0, // Unix epoch old date
            metadata: ProofMetadata {
                version: "v1".to_string(),
                compression: None,
                size_bytes: 10,
            },
        };

        let resolver = ZkURLResolver::new(vec![]);
        let result = resolver.verify_proof_bundle(&old_bundle).await.unwrap();
        assert_eq!(result, false);
    }
}
