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
}

impl Default for Scorer {
    fn default() -> Self {
        Self { alpha: 0.05 }
    }
}
