use std::collections::HashMap;

/// Translation service using MyMemory API (free, no key required).
/// Falls back to placeholder if the API is unreachable.
#[derive(Clone)]
pub struct Translator {
    pub supported: Vec<String>,
    cache: HashMap<(String, String), String>,
    client: reqwest::Client,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            supported: vec!["zh".into(), "en".into(), "ja".into(), "ko".into()],
            cache: HashMap::new(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .user_agent("Agora-Translator/0.1")
                .build()
                .unwrap_or_default(),
        }
    }

    /// Translate text to target language via MyMemory API.
    pub async fn translate(&mut self, text: &str, from: &str, to: &str) -> String {
        if from == to || text.trim().is_empty() {
            return text.to_string();
        }

        let hash_key = (Self::quick_hash(text), to.to_string());
        if let Some(cached) = self.cache.get(&hash_key) {
            return cached.clone();
        }

        let result = self.call_api(text, from, to).await;
        self.cache.insert(hash_key, result.clone());
        result
    }

    async fn call_api(&self, text: &str, from: &str, to: &str) -> String {
        let lang_pair = format!("{}|{}", from, to);
        let url = format!(
            "https://api.mymemory.translated.net/get?q={}&langpair={}&de=agora@protocol.org",
            urlencoding(text),
            &lang_pair
        );

        tracing::debug!("Translation request: {}→{} ({} chars)", from, to, text.len());

        match self.client.get(&url).send().await {
            Ok(resp) => {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        let translated = json["responseData"]["translatedText"]
                            .as_str()
                            .unwrap_or(text);
                        if translated != text {
                            tracing::info!("Translated [{}→{}]: {} chars", from, to, translated.len());
                        }
                        translated.to_string()
                    }
                    Err(_) => {
                        tracing::warn!("Translation API parse error for {}→{}", from, to);
                        text.to_string()
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Translation API unreachable ({}→{}): {:?}", from, to, e);
                // Fallback: return original text
                text.to_string()
            }
        }
    }

    fn quick_hash(s: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        format!("{:x}", h.finish())
    }
}

/// URL encoding that correctly handles UTF-8 multi-byte characters.
fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        match *byte {
            b' ' => result.push('+'),
            b'-' | b'_' | b'.' | b'~' => result.push(*byte as char),
            b if b.is_ascii_alphanumeric() => result.push(b as char),
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}
