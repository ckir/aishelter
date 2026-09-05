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
        1.0 - (-(n_events as f64) / 10.0).exp()
    }

    /// Compute a score from raw values using exponential decay starting at 0.5.
    pub fn compute_score_from_events(values: &[f64]) -> f64 {
        let scorer = Self::default();
        let mut score = 0.5;
        for &val in values {
            score = scorer.update(score, val);
        }
        score
    }
}

impl Default for Scorer {
    fn default() -> Self {
        Self { alpha: 0.05 }
    }
}
