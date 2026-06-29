use std::collections::HashMap;
use std::sync::Mutex;

/// Simple in-memory rate limiter: max `limit` requests per `window_secs` per agent.
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (u32, i64)>>, // (count, window_start_secs)
    limit: u32,
    window_secs: i64,
}

impl RateLimiter {
    pub fn new(limit: u32, window_secs: i64) -> Self {
        Self { windows: Mutex::new(HashMap::new()), limit, window_secs }
    }

    /// Check if an agent is allowed. Returns true if allowed, false if rate limited.
    pub fn check(&self, agent_key: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        let mut guard = self.windows.lock().unwrap();
        let entry = guard.entry(agent_key.to_string()).or_insert((0, now));

        if now - entry.1 > self.window_secs {
            // New window
            *entry = (1, now);
            return true;
        }

        if entry.0 >= self.limit {
            return false;
        }

        entry.0 += 1;
        true
    }
}
