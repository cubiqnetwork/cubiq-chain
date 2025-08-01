use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::fmt;

/// Represents a zkURL (zero-knowledge URL) reference as used by the Cubiq network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZkURL {
    /// Optional Prover identifier (could be public key or unique string)
    pub prover_id: Option<String>,
    /// Domain or content hash (IPFS hash or DNS)
    pub domain_or_hash: String,
    /// Proof identifier (unique within the domain or hash)
    pub proof_id: String,
    /// Optional metadata (versioning, compression, type)
    pub metadata: Option<ZkURLMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZkURLMetadata {
    pub version: String,
    pub compression: Option<String>,
    pub proof_type: String,
}

/// Errors for parsing/handling zkURLs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkURLError {
    InvalidScheme,
    InvalidFormat,
    ParseError(String),
}

impl fmt::Display for ZkURLError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZkURLError::InvalidScheme => write!(f, "Invalid zkURL scheme"),
            ZkURLError::InvalidFormat => write!(f, "Invalid zkURL format"),
            ZkURLError::ParseError(err) => write!(f, "Parse error: {}", err),
        }
    }
}

impl std::error::Error for ZkURLError {}

impl FromStr for ZkURL {
    type Err = ZkURLError;

    /// Parses a zkURL string:  
    /// Format: zk://[proverID]@[domain_or_hash]/[proof_id]#[metadata]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with("zk://") {
            return Err(ZkURLError::InvalidScheme);
        }
        let url_part = &s[5..]; // Remove "zk://"
        let parts: Vec<&str> = url_part.split('@').collect();

        let (prover_id, remaining) = if parts.len() == 2 {
            (Some(parts[0].to_string()), parts[1])
        } else {
            (None, parts[0])
        };

        let (domain_hash_and_path, metadata_str) = if let Some(hash_pos) = remaining.find('#') {
            let (left, right) = remaining.split_at(hash_pos);
            (left, Some(&right[1..]))
        } else {
            (remaining, None)
        };

        let path_parts: Vec<&str> = domain_hash_and_path.splitn(2, '/').collect();
        if path_parts.len() != 2 {
            return Err(ZkURLError::InvalidFormat);
        }
        let domain_or_hash = path_parts[0].to_string();
        let proof_id = path_parts[1].to_string();

        let metadata = if let Some(meta_str) = metadata_str {
            Some(ZkURLMetadata::parse(meta_str)?)
        } else {
            None
        };

        Ok(ZkURL {
            prover_id,
            domain_or_hash,
            proof_id,
            metadata,
        })
    }
}

impl ZkURLMetadata {
    /// Parses the metadata segment (e.g., "v1&gzip&stark")
    pub fn parse(s: &str) -> Result<Self, ZkURLError> {
        let parts: Vec<&str> = s.split('&').collect();
        Ok(ZkURLMetadata {
            version: parts.get(0).unwrap_or(&"v1").to_string(),
            compression: parts.get(1).map(|s| s.to_string()),
            proof_type: parts.get(2).unwrap_or(&"stark").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_url() {
        let url = "zk://prover123@domain.com/block1024#v1&gzip&stark";
        let parsed = ZkURL::from_str(url).unwrap();
        assert_eq!(parsed.prover_id, Some("prover123".to_string()));
        assert_eq!(parsed.domain_or_hash, "domain.com");
        assert_eq!(parsed.proof_id, "block1024");
        let meta = parsed.metadata.expect("Metadata should exist");
        assert_eq!(meta.version, "v1");
        assert_eq!(meta.compression, Some("gzip".to_string()));
        assert_eq!(meta.proof_type, "stark");
    }

    #[test]
    fn test_parse_ipfs_content_only() {
        let url = "zk://QmHash123/block1";
        let parsed = ZkURL::from_str(url).unwrap();
        assert_eq!(parsed.prover_id, None);
        assert_eq!(parsed.domain_or_hash, "QmHash123");
        assert_eq!(parsed.proof_id, "block1");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_invalid_url_scheme() {
        let url = "http://domain.com/block";
        let result = ZkURL::from_str(url);
        assert!(matches!(result, Err(ZkURLError::InvalidScheme)));
    }
}
pub mod resolver;
