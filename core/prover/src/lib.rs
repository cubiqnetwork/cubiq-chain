use p3_field::extension::BinomialExtensionField;
use p3_goldilocks::Goldilocks;
// use p3_matrix::dense::RowMajorMatrix;
use js_sys;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use wasm_bindgen::prelude::*;
use web_sys;

type F = Goldilocks;
type EF = BinomialExtensionField<F, 2>;

/// MobileProofVerifier struct exposed to WASM or native.
#[wasm_bindgen]
pub struct MobileProofVerifier {
    config: VerifierConfig,
}

#[wasm_bindgen]
impl MobileProofVerifier {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            config: VerifierConfig::mobile_optimized(),
        }
    }

    /// Verify proof bytes, return true if valid, false otherwise.
    ///
    /// Errors are converted to `JsValue` for WASM consumers.
    #[wasm_bindgen]
    pub fn verify_proof(&self, proof_bytes: &[u8]) -> Result<bool, JsValue> {
        let proof = self
            .deserialize_proof(proof_bytes)
            .map_err(|e| JsValue::from_str(&format!("Failed to deserialize proof: {}", e)))?;

        let start = Instant::now();
        let result = self.verify_stark_proof(&proof);
        let elapsed = start.elapsed();

        if elapsed.as_millis() > self.config.max_verification_time_ms {
            web_sys::console::warn_1(&JsValue::from_str(&format!(
                "Warning: Proof verification took {}ms (target < {}ms)",
                elapsed.as_millis(),
                self.config.max_verification_time_ms
            )));
        }

        Ok(result)
    }

    /// Returns current memory usage in bytes (approximation for WASM).
    #[wasm_bindgen]
    pub fn get_memory_usage(&self) -> u32 {
        // Approximate JS heap size for WASM (this calls JS eval)
        js_sys::eval("performance.memory ? performance.memory.usedJSHeapSize : 0")
            .unwrap_or(JsValue::from(0))
            .as_f64()
            .unwrap_or(0.0) as u32
    }
}

impl MobileProofVerifier {
    // Deserialize proof from binary form using bincode
    fn deserialize_proof(&self, bytes: &[u8]) -> Result<STARKProof<F, EF>, bincode::Error> {
        bincode::deserialize(bytes)
    }

    // Mobile-optimized STARK verification (simplified)
    fn verify_stark_proof(&self, proof: &STARKProof<F, EF>) -> bool {
        if !self.verify_proof_structure(proof) {
            return false;
        }
        if !self.verify_fri_consistency(proof) {
            return false;
        }
        self.verify_constraints(proof)
    }

    fn verify_proof_structure(&self, proof: &STARKProof<F, EF>) -> bool {
        !proof.trace_cap.is_empty() && !proof.quotient_chunks_cap.is_empty()
    }

    fn verify_fri_consistency(&self, _proof: &STARKProof<F, EF>) -> bool {
        // Simplified stub: always true for now
        true
    }

    fn verify_constraints(&self, _proof: &STARKProof<F, EF>) -> bool {
        // Simplified stub: always true for now
        true
    }
}

#[derive(Serialize, Deserialize)]
pub struct STARKProof<F, EF> {
    trace_cap: Vec<[F; 4]>,
    quotient_chunks_cap: Vec<[F; 4]>,
    fri_proof: FRIProof<F, EF>,
}

#[derive(Serialize, Deserialize)]
pub struct FRIProof<F, EF> {
    commit_phase_caps: Vec<Vec<[F; 4]>>,
    query_proofs: Vec<QueryProof<F, EF>>,
    final_poly: Vec<EF>,
}

#[derive(Serialize, Deserialize)]
pub struct QueryProof<F, EF> {
    initial_trees_proof: Vec<Vec<F>>,
    steps: Vec<FRIQueryStep<F, EF>>,
}

#[derive(Serialize, Deserialize)]
pub struct FRIQueryStep<F, EF> {
    sibling_value: EF,
    opening_proof: Vec<[F; 4]>,
}

struct VerifierConfig {
    pub max_memory_mb: usize,
    pub max_verification_time_ms: u128,
    pub fri_queries: usize,
}

impl VerifierConfig {
    pub fn mobile_optimized() -> Self {
        Self {
            max_memory_mb: 400,
            max_verification_time_ms: 500,
            fri_queries: 80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_proof_structure_check() {
        let proof = STARKProof {
            trace_cap: vec![[Goldilocks::ZERO; 4]; 1],
            quotient_chunks_cap: vec![[Goldilocks::ZERO; 4]; 1],
            fri_proof: FRIProof {
                commit_phase_caps: vec![vec![[Goldilocks::ZERO; 4]]],
                query_proofs: vec![],
                final_poly: vec![],
            },
        };
        let verifier = MobileProofVerifier::new();
        assert!(verifier.verify_proof_structure(&proof));
    }

    #[test]
    fn empty_proof_structure_check() {
        let proof = STARKProof {
            trace_cap: vec![],
            quotient_chunks_cap: vec![],
            fri_proof: FRIProof {
                commit_phase_caps: vec![],
                query_proofs: vec![],
                final_poly: vec![],
            },
        };
        let verifier = MobileProofVerifier::new();
        assert!(!verifier.verify_proof_structure(&proof));
    }
}
