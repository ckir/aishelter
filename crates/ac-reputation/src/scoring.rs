use ac_types::reputation::{ReputationDimension, ReputationEvent};

/// Exponential decay scoring.
/// score = alpha * recent_result + (1 - alpha) * old_score
pub struct Scorer {
    alpha: f64,
}

impl Scorer {
    pub fn new(alpha: f64) -> Self {
        Self { alpha }
    }

    pub fn update(&self, old_score: f64, recent_result: f64) -> f64 {
        self.alpha * recent_result + (1.0 - self.alpha) * old_score
    }

    pub fn confidence(&self, n_events: u64) -> f64 {
        // Simple confidence: 1 - e^(-n/10)
        1.0 - (-(n_events as f64) / 10.0).exp()
    }

    /// Compute a score from a sequence of events, applying exponential decay.
    /// Starts at 0.5 and updates for each event ordered by timestamp.
    pub fn compute_score_from_events(events: &[ReputationEvent]) -> f64 {
        let scorer = Self::default();
        let mut score = 0.5;
        for event in events {
            score = scorer.update(score, event.value);
        }
        score
    }

    /// Compute score for a specific dimension from events filtered to that dimension.
    pub fn compute_dimension_score(events: &[ReputationEvent], dimension: ReputationDimension) -> f64 {
        let filtered: Vec<_> = events.iter()
            .filter(|e| e.dimension == dimension)
            .cloned()
            .collect();
        if filtered.is_empty() {
            return 0.0;
        }
        Self::compute_score_from_events(&filtered)
    }
}

impl Default for Scorer {
    fn default() -> Self {
        Self { alpha: 0.05 }
    }
}
