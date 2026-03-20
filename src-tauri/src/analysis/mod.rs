pub mod ancestry;
pub mod carrier;
pub mod health_risk;
pub mod pharmacogenomics;
pub mod traits;

use serde::{Deserialize, Serialize};

/// Generic wrapper for analysis results, adding metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult<T: Serialize> {
    pub data: T,
    pub computed_at: String,
    pub genome_id: i64,
}

impl<T: Serialize> AnalysisResult<T> {
    pub fn new(data: T, genome_id: i64) -> Self {
        Self {
            data,
            computed_at: chrono::Utc::now().to_rfc3339(),
            genome_id,
        }
    }
}
