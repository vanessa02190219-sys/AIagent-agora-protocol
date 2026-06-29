use serde::{Deserialize, Serialize};

/// Result of fallacy detection on a post.
#[derive(Debug, Serialize)]
pub struct FallacyReport {
    pub detections: Vec<FallacyDetection>,
    pub total_flags: usize,
    pub method: String, // "llm" | "rule" | "hybrid"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallacyDetection {
    pub fallacy_type: String,
    pub confidence: f64,
    pub reason: String,
    pub matched_pattern: String,
}

/// Check a post using rule-based pattern matching (fast, always available).
pub fn detect_fallacies_rules(text: &str) -> Vec<FallacyDetection> {
    let mut detections = Vec::new();

    let fd_patterns = [
        "不是...就是", "要么...要么", "只有...才能",
        "either...or", "the only way", "no other choice",
        "非黑即白", "二分", "black and white",
    ];
    for pat in &fd_patterns {
        if text.contains(pat) {
            detections.push(FallacyDetection {
                fallacy_type: "false_dichotomy".into(), confidence: 0.6,
                reason: "可能存在非此即彼的简化论证".into(), matched_pattern: pat.to_string(),
            });
            break;
        }
    }

    let small_sample = ["我见过", "我的一个朋友", "有一次", "某次", "I saw", "my friend"];
    let big_conclusion = ["所有", "总是", "从不", "永远", "一定", "all", "always", "never", "every"];
    let has_small = small_sample.iter().any(|p| text.contains(p));
    let has_big = big_conclusion.iter().any(|p| text.contains(p));
    if has_small && has_big {
        detections.push(FallacyDetection {
            fallacy_type: "hasty_generalization".into(), confidence: 0.55,
            reason: "从小样本/个人经验推到普遍结论".into(),
            matched_pattern: "small_sample + big_conclusion".into(),
        });
    }

    let authority = ["专家说", "权威", "众所周知", "科学家认为", "experts say", "authorities"];
    for pat in &authority {
        if text.contains(pat) && !text.contains("http") && !text.contains("引用") {
            detections.push(FallacyDetection {
                fallacy_type: "appeal_to_authority".into(), confidence: 0.5,
                reason: "引用权威但未提供可验证来源".into(), matched_pattern: pat.to_string(),
            });
            break;
        }
    }

    let slope = ["一旦...就会", "连锁反应", "多米诺", "domino effect"];
    for pat in &slope {
        if text.contains(pat) {
            detections.push(FallacyDetection {
                fallacy_type: "slippery_slope".into(), confidence: 0.5,
                reason: "可能存在滑坡论证：缺乏中间步骤的因果推导".into(), matched_pattern: pat.to_string(),
            });
            break;
        }
    }

    detections
}

/// Check a post using LLM (higher accuracy, requires API).
pub async fn detect_fallacies_llm(
    text: &str,
    api_url: &str,
    api_key: &str,
) -> Vec<FallacyDetection> {
    let prompt = format!(
        r#"Analyze the following text for logical fallacies. Return ONLY a JSON array of detected fallacies. Each object must have: fallacy_type (one of: false_dichotomy, hasty_generalization, appeal_to_authority, slippery_slope, straw_man, circular_reasoning, correlation_causation, ad_hominem, begging_question, red_herring), confidence (0.0-1.0), reason (brief explanation in the text's language).

If no fallacies found, return empty array [].

Text to analyze:
"{}"

JSON response:"#,
        text
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.1,
        "max_tokens": 500
    });

    let resp = match client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("LLM fallacy API unreachable: {:?}", e);
            return vec![];
        }
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("LLM fallacy response parse error: {:?}", e);
            return vec![];
        }
    };

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("[]");

    // Extract JSON array from response (may have markdown wrapping)
    let content = content.trim();
    let content = if content.starts_with("```") {
        content.lines().skip(1).take_while(|l| !l.starts_with("```")).collect::<Vec<_>>().join("\n")
    } else {
        content.to_string()
    };

    match serde_json::from_str::<Vec<FallacyDetection>>(&content) {
        Ok(detections) => detections,
        Err(e) => {
            tracing::warn!("LLM fallacy JSON parse error: {:?}", e);
            vec![]
        }
    }
}

/// Main entry point: try LLM first, fall back to rules.
pub async fn detect_fallacies(
    text: &str,
    llm_url: Option<&str>,
    llm_key: Option<&str>,
) -> FallacyReport {
    let mut detections = Vec::new();
    let mut method = "rule";

    // Try LLM if configured
    if let (Some(url), Some(key)) = (llm_url, llm_key) {
        if !url.is_empty() && !key.is_empty() {
            let llm_results = detect_fallacies_llm(text, url, key).await;
            if !llm_results.is_empty() {
                detections.extend(llm_results);
                method = "llm";
            }
        }
    }

    // Always run rules as supplement/fallback
    if method == "rule" || detections.is_empty() {
        let rule_results = detect_fallacies_rules(text);
        if !rule_results.is_empty() {
            detections.extend(rule_results);
            if method == "llm" { method = "hybrid"; }
        }
    }

    FallacyReport {
        total_flags: detections.len(),
        detections,
        method: method.into(),
    }
}
