#![allow(clippy::let_unit_value)]
#![allow(unused_must_use)]

use eframe::egui;
use reqwest;
use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::time::Instant;

// Type aliases to reduce complexity
type ComparisonResult = (ModelType, Result<(String, u64), String>);
type StressResult = (usize, Result<(String, u64), String>);
type ResponseResult = Result<(String, u64), String>;

#[derive(Serialize, Clone)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OllamaResponse {
    response: String,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    context: Option<Vec<i32>>,
    #[serde(default)]
    total_duration: Option<u64>,
    #[serde(default)]
    load_duration: Option<u64>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    prompt_eval_duration: Option<u64>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

#[derive(PartialEq, Clone, Copy)]
enum ModelType {
    Q4,
    Q5,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Q4 => write!(f, "Q4"),
            ModelType::Q5 => write!(f, "Q5"),
        }
    }
}

struct OllamaUI {
    prompt: String,
    response: String,
    selected_model: ModelType,
    is_loading: bool,
    ollama_url: String,
    q4_model_name: String,
    q5_model_name: String,
    response_receiver: Option<mpsc::Receiver<ResponseResult>>,
    request_start_time: Option<Instant>,
    last_response_time: Option<u64>,
    stats: String,
    // New fields for parallel comparison
    is_comparing: bool,
    q4_response: String,
    q5_response: String,
    q4_time: Option<u64>,
    q5_time: Option<u64>,
    comparison_receiver: Option<mpsc::Receiver<ComparisonResult>>,
    // Cooldown mechanism to prevent overwhelming Ollama
    last_preload_time: Option<Instant>,
    cooldown_remaining: Option<u64>,
    // Performance tracking
    performance_history: Vec<PerformanceRecord>,
    session_stats: SessionStats,
    // Stress test specific fields
    is_stress_testing: bool,
    stress_test_receiver: Option<mpsc::Receiver<StressResult>>,
    stress_test_results: Vec<StressTestResult>,
    stress_test_start_time: Option<Instant>,
}

#[derive(Clone)]
struct StressTestResult {
    #[allow(dead_code)]
    request_id: usize,
    response_time_ms: Option<u64>,
    success: bool,
    error_message: Option<String>,
}

#[derive(Clone)]
struct PerformanceRecord {
    test_type: String,
    model: String,
    response_time_ms: u64,
    tokens_per_sec: Option<f64>,
    timestamp: String,
    success: bool,
}

#[derive(Default)]
struct SessionStats {
    total_requests: u32,
    successful_requests: u32,
    failed_requests: u32,
    total_response_time_ms: u64,
    fastest_response_ms: Option<u64>,
    slowest_response_ms: Option<u64>,
}

impl Default for OllamaUI {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            response: String::new(),
            selected_model: ModelType::Q4,
            is_loading: false,
            ollama_url: "http://localhost:11434".to_string(),
            q4_model_name: "qwen-contract:latest".to_string(),
            q5_model_name: "qwen-contract-q5:latest".to_string(),
            response_receiver: None,
            request_start_time: None,
            last_response_time: None,
            stats: String::new(),
            // Initialize new comparison fields
            is_comparing: false,
            q4_response: String::new(),
            q5_response: String::new(),
            q4_time: None,
            q5_time: None,
            comparison_receiver: None,
            last_preload_time: None,
            cooldown_remaining: None,
            // Initialize performance tracking
            performance_history: Vec::new(),
            session_stats: SessionStats::default(),
            // Initialize stress test fields
            is_stress_testing: false,
            stress_test_receiver: None,
            stress_test_results: Vec::new(),
            stress_test_start_time: None,
        }
    }
}

impl OllamaUI {
    fn send_request(&mut self) {
        if self.prompt.trim().is_empty() {
            return;
        }

        let model_name = match self.selected_model {
            ModelType::Q4 => self.q4_model_name.clone(),
            ModelType::Q5 => self.q5_model_name.clone(),
        };

        let request = OllamaRequest {
            model: model_name,
            prompt: self.prompt.clone(),
            stream: false,
        };

        let url = format!("{}/api/generate", self.ollama_url);
        let (tx, rx) = mpsc::channel();
        
        self.response_receiver = Some(rx);
        self.is_loading = true;
        self.request_start_time = Some(Instant::now());
        self.response = "Generating response...".to_string();
        self.stats = String::new();

        // Use a different approach for async operations in eframe
        let url_clone = url.clone();
        let request_clone = request.clone();
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                println!("🔄 Sending request to: {}", url_clone);
                println!("📝 Model: {}", request_clone.model);
                println!("💬 Prompt: {}", &request_clone.prompt[..std::cmp::min(50, request_clone.prompt.len())]);
                
                // Optimized HTTP client for better performance
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(120))
                    .pool_max_idle_per_host(2)
                    .pool_idle_timeout(std::time::Duration::from_secs(30))
                    .user_agent("ollama-rust-ui/0.1.0")
                    .build()
                    .unwrap();
                    
                let start_time = Instant::now();
                
                let result = match client
                    .post(&url_clone)
                    .json(&request_clone)
                    .send()
                    .await
                {
                    Ok(response) => {
                        let status = response.status();
                        println!("📡 Response status: {}", status);
                        
                        if status.is_success() {
                            let response_text = match response.text().await {
                                Ok(text) => text,
                                Err(e) => return Err(format!("Failed to read response text: {}", e)),
                            };
                            
                            println!("📄 Response body: {}", &response_text[..std::cmp::min(200, response_text.len())]);
                            
                            match serde_json::from_str::<OllamaResponse>(&response_text) {
                                Ok(ollama_response) => {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    println!("✅ Successfully parsed response in {}ms", elapsed);
                                    Ok((ollama_response.response, elapsed))
                                }
                                Err(e) => {
                                    println!("❌ JSON parse error: {}", e);
                                    Err(format!("Failed to parse JSON response: {}\n\nResponse preview: {}", e, &response_text[..std::cmp::min(300, response_text.len())]))
                                }
                            }
                        } else {
                            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                            Err(format!("HTTP {}: {}\nModel: {}\nURL: {}", status, error_text, request_clone.model, url_clone))
                        }
                    }
                    Err(e) => {
                        println!("❌ Request error: {}", e);
                        if e.is_timeout() {
                            Err("⏱️ Request timed out. The model might be loading or the server is slow.".to_string())
                        } else if e.is_connect() {
                            Err("🔌 Connection failed. Is Ollama running? Try: ollama serve".to_string())
                        } else if e.is_request() {
                            Err(format!("📤 Request error: {}\nCheck your model name and URL.", e))
                        } else {
                            Err(format!("🔥 Network error: {}", e))
                        }
                    }
                };

                let _ = tx.send(result);
Ok::<(), String>(())
            });
        });
    }

    fn check_response(&mut self) {
        // Check comparison responses first
        self.check_comparison_responses();
        
        // Check stress test responses
        self.check_stress_test_responses();
        
        // Then check single model responses
        if let Some(receiver) = &self.response_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.is_loading = false;
                
                match result {
                    Ok((response, duration)) => {
                        self.response = response.clone();
                        self.last_response_time = Some(duration);
                        
                        // Check if this was a preload operation and set cooldown
                        if response.contains("Models pre-loaded into RAM") {
                            self.last_preload_time = Some(Instant::now());
                            println!("🕒 Starting 5-second cooldown after preload to prevent overwhelming Ollama");
                        }
                        
                        // Record performance
                        let model_name = match self.selected_model {
                            ModelType::Q4 => self.q4_model_name.clone(),
                            ModelType::Q5 => self.q5_model_name.clone(),
                        };
                        let response_clone = self.response.clone();
                        self.record_performance("Single Request", &model_name, duration, true, Some(&response_clone));
                        
                        // Calculate stats
                        let words = self.response.split_whitespace().count();
                        let chars = self.response.len();
                        let tokens_estimate = words as f64 * 1.3; // Rough estimate
                        let tokens_per_second = if duration > 0 {
                            (tokens_estimate * 1000.0) / duration as f64
                        } else {
                            0.0
                        };
                        
                        self.stats = format!(
                            "Single Model Response:\nTime: {}ms\nWords: {}\nCharacters: {}\nEst. tokens: {:.0}\nTokens/second: {:.1}",
                            duration, words, chars, tokens_estimate, tokens_per_second
                        );
                    }
                    Err(error) => {
                        self.response = format!("Error: {}", error);
                        self.stats = String::new();
                        
                        // Record failed performance
                        let model_name = match self.selected_model {
                            ModelType::Q4 => self.q4_model_name.clone(),
                            ModelType::Q5 => self.q5_model_name.clone(),
                        };
                        self.record_performance("Single Request", &model_name, 0, false, None);
                    }
                }
                
                self.response_receiver = None;
            }
        }
    }
    
    fn test_ollama_connection(&mut self) {
        let url = format!("{}/api/tags", self.ollama_url);
        let (tx, rx) = mpsc::channel();
        
        self.response_receiver = Some(rx);
        self.is_loading = true;
        self.response = "Testing connection...".to_string();
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap();
                    
                let result = match client.get(&url).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.text().await {
                                Ok(text) => {
                                    if text.contains("models") {
                                        Ok(("✅ Connection successful! Ollama is running and accessible.".to_string(), 0))
                                    } else {
                                        Ok(("⚠️ Connected but unexpected response format.".to_string(), 0))
                                    }
                                }
                                Err(e) => Err(format!("Failed to read response: {}", e)),
                            }
                        } else {
                            Err(format!("HTTP {}: Connection failed", response.status()))
                        }
                    }
                    Err(e) => Err(format!("❌ Connection failed: {}\n\nTips:\n1. Make sure Ollama is running: 'ollama serve'\n2. Check if the URL is correct\n3. Verify firewall settings", e)),
                };

                let _ = tx.send(result);
            });
        });
    }
    
    fn compare_models_parallel(&mut self) {
        if self.prompt.trim().is_empty() {
            return;
        }

        let prompt = self.prompt.clone();
        let url = self.ollama_url.clone();
        let q4_model = self.q4_model_name.clone();
        let q5_model = self.q5_model_name.clone();
        
        let (tx, rx) = mpsc::channel();
        
        self.comparison_receiver = Some(rx);
        self.is_comparing = true;
        self.request_start_time = Some(Instant::now());
        self.q4_response = "🏃‍♂️ Q4 generating...".to_string();
        self.q5_response = "🏃‍♀️ Q5 generating...".to_string();
        self.q4_time = None;
        self.q5_time = None;

        println!("🚀 Starting parallel comparison for prompt: {}", &prompt[..std::cmp::min(50, prompt.len())]);

        // Spawn Q4 request (optimized for core 1)
        let tx_q4 = tx.clone();
        let url_q4 = url.clone();
        let prompt_q4 = prompt.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .enable_io()
                .build()
                .unwrap();
                
            let _ = rt.block_on(async move {
                let request = OllamaRequest {
                    model: q4_model,
                    prompt: prompt_q4,
                    stream: false,
                };

                let result = Self::send_http_request(url_q4, request, "Q4").await;
                let _ = tx_q4.send((ModelType::Q4, result));
                Ok::<(), String>(())
            });
        });

        // Spawn Q5 request (optimized for core 2)
        let tx_q5 = tx;
        let url_q5 = url;
        let prompt_q5 = prompt;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .enable_io()
                .build()
                .unwrap();
                
            let _ = rt.block_on(async move {
                let request = OllamaRequest {
                    model: q5_model,
                    prompt: prompt_q5,
                    stream: false,
                };

                let result = Self::send_http_request(url_q5, request, "Q5").await;
                let _ = tx_q5.send((ModelType::Q5, result));
                Ok::<(), String>(())
            });
        });
    }

    fn stress_test_parallel(&mut self) {
        if self.prompt.trim().is_empty() {
            self.prompt = "Test stress".to_string();
        }

        let prompt = self.prompt.clone();
        let url = self.ollama_url.clone();
        let q4_model = self.q4_model_name.clone();
        
        let (tx, rx) = mpsc::channel();
        self.stress_test_receiver = Some(rx);
        self.is_stress_testing = true;
        self.stress_test_start_time = Some(Instant::now());
        self.stress_test_results = Vec::new();
        
        // Initialize results tracking
        for i in 0..5 {
            self.stress_test_results.push(StressTestResult {
                request_id: i,
                response_time_ms: None,
                success: false,
                error_message: None,
            });
        }
        
        println!("🚀 Starting AGGRESSIVE stress test: 5 rapid parallel requests!");

        // Send 5 rapid requests with minimal staggering
        for i in 0..5 {
            let tx_clone = tx.clone();
            let url_clone = url.clone();
            let prompt_clone = format!("{} (Stress Request {})", prompt, i + 1);
            let model_clone = q4_model.clone();
            let delay_ms = i as u64 * 50; // Reduced to 50ms staggering for more aggressive testing
            
            std::thread::spawn(move || {
                // Minimal delay to stagger requests
                if delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .enable_io()
                    .build()
                    .unwrap();
                    
                let _ = rt.block_on(async move {
                    let request = OllamaRequest {
                        model: model_clone,
                        prompt: prompt_clone,
                        stream: false,
                    };

                    let result = Self::send_http_request(url_clone, request, &format!("Stress-{}", i + 1)).await;
                    let _ = tx_clone.send((i, result));
                    Ok::<(), String>(())
                });
            });
        }
        
        self.q4_response = "🚀 AGGRESSIVE Stress Test: 5 rapid requests (50ms apart)".to_string();
        self.q5_response = "� Watch real-time results below! Testing Ollama's limits...".to_string();
    }

    fn preload_models(&mut self) {
        let url = self.ollama_url.clone();
        let q4_model = self.q4_model_name.clone();
        let q5_model = self.q5_model_name.clone();
        
        let (tx, rx) = mpsc::channel();
        self.response_receiver = Some(rx);
        self.is_loading = true;
        self.response = "🔥 Pre-loading models into RAM for faster responses...".to_string();
        
        println!("🚀 Pre-loading models into Ollama's RAM cache");
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(async move {
                let mut results = Vec::new();
                
                // Pre-load Q4 model with a tiny prompt to get it into RAM
                println!("📥 Loading Q4 model into RAM...");
                let q4_request = OllamaRequest {
                    model: q4_model.clone(),
                    prompt: "Hi".to_string(),
                    stream: false,
                };
                
                match Self::send_http_request(format!("{}/api/generate", &url), q4_request, "Q4-Preload").await {
                    Ok((_, duration)) => {
                        println!("✅ Q4 model loaded into RAM in {}ms", duration);
                        results.push(format!("Q4 loaded: {}ms", duration));
                    }
                    Err(e) => {
                        println!("❌ Q4 preload failed: {}", e);
                        results.push(format!("Q4 failed: {}", e));
                    }
                }
                
                // Pre-load Q5 model
                println!("📥 Loading Q5 model into RAM...");
                let q5_request = OllamaRequest {
                    model: q5_model.clone(),
                    prompt: "Hi".to_string(),
                    stream: false,
                };
                
                match Self::send_http_request(format!("{}/api/generate", &url), q5_request, "Q5-Preload").await {
                    Ok((_, duration)) => {
                        println!("✅ Q5 model loaded into RAM in {}ms", duration);
                        results.push(format!("Q5 loaded: {}ms", duration));
                    }
                    Err(e) => {
                        println!("❌ Q5 preload failed: {}", e);
                        results.push(format!("Q5 failed: {}", e));
                    }
                }
                
                let summary = format!(
                    "🔥 Models pre-loaded into RAM!\n{}\n\n💡 Next requests should be much faster since models are cached in memory.\n⏳ Starting 5-second cooldown to prevent overwhelming Ollama...",
                    results.join("\n")
                );
                
                let _ = tx.send(Ok((summary, 0)));
                Ok::<(), String>(())
            });
        });
    }

    fn check_model_status(&mut self) {
        let url = format!("{}/api/ps", self.ollama_url);
        let (tx, rx) = mpsc::channel();
        
        self.response_receiver = Some(rx);
        self.is_loading = true;
        self.response = "🔍 Checking which models are loaded in RAM...".to_string();
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap();
                    
                let result = match client.get(&url).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.text().await {
                                Ok(text) => {
                                    println!("📊 Model status response: {}", text);
                                    
                                    // Parse the response to show loaded models
                                    if text.contains("models") {
                                        let formatted = if text.trim() == "{\"models\":[]}" {
                                            "🔄 No models currently loaded in RAM.\n\n💡 Use 'Pre-load Models' to cache them for faster responses!".to_string()
                                        } else {
                                            format!("🔥 Models in RAM:\n{}\n\n✅ Loaded models will respond much faster!", text)
                                        };
                                        Ok((formatted, 0))
                                    } else {
                                        Ok((format!("📊 Model status:\n{}", text), 0))
                                    }
                                }
                                Err(e) => Err(format!("Failed to read model status: {}", e)),
                            }
                        } else {
                            Err(format!("HTTP {}: Failed to get model status", response.status()))
                        }
                    }
                    Err(e) => Err(format!("❌ Failed to check model status: {}", e)),
                };

                let _ = tx.send(result);
                Ok::<(), String>(())
            });
        });
    }

    // Extracted HTTP request logic for reuse
    async fn send_http_request(url: String, request: OllamaRequest, model_label: &str) -> Result<(String, u64), String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 1000;
        
        for attempt in 1..=MAX_RETRIES {
            println!("🔄 {} Attempt {}/{} - Sending request to: {}", model_label, attempt, MAX_RETRIES, url);
            println!("📝 {} Model: {}", model_label, request.model);
            
            // Optimized HTTP client for dual-core performance
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .pool_max_idle_per_host(1)  // One connection per core
                .pool_idle_timeout(std::time::Duration::from_secs(30))
                .tcp_nodelay(true)  // Reduce latency
                .user_agent("ollama-rust-ui-parallel/0.1.0")
                .build()
                .unwrap();
                
            let start_time = Instant::now();
            let api_url = if url.contains("/api/generate") { url.clone() } else { format!("{}/api/generate", url) };
            
            match client.post(&api_url).json(&request).send().await {
                Ok(response) => {
                    let status = response.status();
                    println!("📡 {} Response status: {}", model_label, status);
                    
                    if status.is_success() {
                        let response_text = match response.text().await {
                            Ok(text) => text,
                            Err(e) => {
                                if attempt < MAX_RETRIES {
                                    println!("⚠️ {} Retry in {}ms: Failed to read response text: {}", model_label, RETRY_DELAY_MS, e);
                                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                                    continue;
                                }
                                return Err(format!("{} Failed to read response text: {}", model_label, e));
                            }
                        };
                        
                        println!("📄 {} Response body: {}", model_label, &response_text[..std::cmp::min(100, response_text.len())]);
                        
                        match serde_json::from_str::<OllamaResponse>(&response_text) {
                            Ok(ollama_response) => {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                println!("✅ {} Successfully parsed response in {}ms", model_label, elapsed);
                                return Ok((ollama_response.response, elapsed));
                            }
                            Err(e) => {
                                if attempt < MAX_RETRIES {
                                    println!("⚠️ {} Retry in {}ms: JSON parse error: {}", model_label, RETRY_DELAY_MS, e);
                                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                                    continue;
                                }
                                println!("❌ {} JSON parse error: {}", model_label, e);
                                return Err(format!("{} Failed to parse JSON: {}", model_label, e));
                            }
                        }
                    } else {
                        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        if attempt < MAX_RETRIES && (status.is_server_error() || status == 429) {
                            println!("⚠️ {} Retry in {}ms: HTTP {}: {}", model_label, RETRY_DELAY_MS, status, error_text);
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                            continue;
                        }
                        return Err(format!("{} HTTP {}: {}", model_label, status, error_text));
                    }
                }
                Err(e) => {
                    println!("❌ {} Request error on attempt {}: {}", model_label, attempt, e);
                    if attempt < MAX_RETRIES {
                        let delay = RETRY_DELAY_MS * attempt as u64; // Exponential backoff
                        println!("⚠️ {} Retrying in {}ms...", model_label, delay);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }
                    
                    if e.is_timeout() {
                        return Err(format!("{} ⏱️ Request timed out after {} attempts", model_label, MAX_RETRIES));
                    } else if e.is_connect() {
                        return Err(format!("{} 🔌 Connection failed after {} attempts", model_label, MAX_RETRIES));
                    } else {
                        return Err(format!("{} 🔥 Network error after {} attempts: {}", model_label, MAX_RETRIES, e));
                    }
                }
            }
        }
        
        Err(format!("{} All {} attempts failed", model_label, MAX_RETRIES))
    }

    fn check_comparison_responses(&mut self) {
        let mut results_to_process = Vec::new();
        
        // First, collect all results to avoid borrowing conflicts
        if let Some(receiver) = &self.comparison_receiver {
            while let Ok((model_type, result)) = receiver.try_recv() {
                results_to_process.push((model_type, result));
            }
        }
        
        // Now process the results
        for (model_type, result) in results_to_process {
            match model_type {
                ModelType::Q4 => {
                    let q4_model_name = self.q4_model_name.clone();
                    match result {
                        Ok((response, duration)) => {
                            self.q4_response = response.clone();
                            self.q4_time = Some(duration);
                            self.record_performance("Parallel Compare", &q4_model_name, duration, true, Some(&response));
                        }
                        Err(error) => {
                            self.q4_response = format!("❌ Q4 Error: {}", error);
                            self.record_performance("Parallel Compare", &q4_model_name, 0, false, None);
                        }
                    }
                }
                ModelType::Q5 => {
                    let q5_model_name = self.q5_model_name.clone();
                    match result {
                        Ok((response, duration)) => {
                            self.q5_response = response.clone();
                            self.q5_time = Some(duration);
                            self.record_performance("Parallel Compare", &q5_model_name, duration, true, Some(&response));
                        }
                        Err(error) => {
                            self.q5_response = format!("❌ Q5 Error: {}", error);
                            self.record_performance("Parallel Compare", &q5_model_name, 0, false, None);
                        }
                    }
                }
            }
        }
                
        // Check if both responses are complete
        if self.q4_time.is_some() && self.q5_time.is_some() {
            self.is_comparing = false;
            self.comparison_receiver = None;
            
            // Calculate comparison stats
            if let (Some(q4_time), Some(q5_time)) = (self.q4_time, self.q5_time) {
                let faster_model = if q4_time < q5_time { "Q4" } else { "Q5" };
                let time_diff = if q4_time < q5_time { q5_time - q4_time } else { q4_time - q5_time };
                let percentage_faster = ((time_diff as f64 / q4_time.max(q5_time) as f64) * 100.0).round();
                
                self.stats = format!(
                    "🏆 Parallel Comparison Results:\n\
                    Q4 Time: {:.2}s ({} words)\n\
                    Q5 Time: {:.2}s ({} words)\n\
                    {} is {:.0}% faster ({:.2}s difference)\n\
                    Total parallel execution optimized for dual-core i5",
                    q4_time as f64 / 1000.0, self.q4_response.split_whitespace().count(),
                    q5_time as f64 / 1000.0, self.q5_response.split_whitespace().count(),
                    faster_model, percentage_faster, time_diff as f64 / 1000.0
                );
                
                println!("🏁 Parallel comparison complete! {} faster by {}ms", faster_model, time_diff);
            }
        }
    }

    fn check_stress_test_responses(&mut self) {
        let mut results_to_process = Vec::new();
        
        // First, collect all results to avoid borrowing conflicts
        if let Some(receiver) = &self.stress_test_receiver {
            while let Ok((request_id, result)) = receiver.try_recv() {
                results_to_process.push((request_id, result));
            }
        }
        
        // Now process the results
        for (request_id, result) in results_to_process {
            if request_id < self.stress_test_results.len() {
                let q4_model_name = self.q4_model_name.clone();
                match result {
                    Ok((response, duration)) => {
                        self.stress_test_results[request_id].response_time_ms = Some(duration);
                        self.stress_test_results[request_id].success = true;
                        self.record_performance("Stress Test", &q4_model_name, duration, true, Some(&response));
                        
                        println!("✅ Stress request {} completed in {}ms", request_id + 1, duration);
                    }
                    Err(error) => {
                        self.stress_test_results[request_id].success = false;
                        self.stress_test_results[request_id].error_message = Some(error.clone());
                        self.record_performance("Stress Test", &q4_model_name, 0, false, None);
                        
                        println!("❌ Stress request {} failed: {}", request_id + 1, error);
                    }
                }
            }
        }
        
        // Check if all stress test requests are complete
        let completed_count = self.stress_test_results.iter()
            .filter(|r| r.response_time_ms.is_some() || r.error_message.is_some())
            .count();
            
        if completed_count == 5 && self.is_stress_testing {
            self.is_stress_testing = false;
            self.stress_test_receiver = None;
            
            // Calculate stress test summary
            let successful = self.stress_test_results.iter().filter(|r| r.success).count();
            let failed = self.stress_test_results.iter().filter(|r| !r.success).count();
            let total_time = if let Some(start_time) = self.stress_test_start_time {
                start_time.elapsed().as_millis() as u64
            } else {
                0
            };
            
            let avg_response_time: f64 = self.stress_test_results.iter()
                .filter_map(|r| r.response_time_ms.map(|t| t as f64))
                .sum::<f64>() / successful.max(1) as f64;
            
            self.stats = format!(
                "🚀 Stress Test Complete!\n\
                Total Time: {:.2}s\n\
                Successful: {}/5 ({:.1}%)\n\
                Failed: {}/5\n\
                Avg Response: {:.2}s\n\
                🦀 Rust handled {} parallel requests efficiently!",
                total_time as f64 / 1000.0,
                successful, (successful as f64 / 5.0) * 100.0,
                failed,
                avg_response_time / 1000.0,
                successful
            );
            
            println!("🏁 Stress test complete! {}/5 successful", successful);
        }
    }

    fn record_performance(&mut self, test_type: &str, model: &str, response_time_ms: u64, success: bool, response_text: Option<&str>) {
        use chrono::prelude::*;
        
        // Calculate tokens per second if we have response text
        let tokens_per_sec = if success && response_time_ms > 0 {
            if let Some(text) = response_text {
                let words = text.split_whitespace().count();
                let tokens_estimate = words as f64 * 1.3; // Rough estimate: ~1.3 tokens per word
                Some((tokens_estimate * 1000.0) / response_time_ms as f64)
            } else {
                None
            }
        } else {
            None
        };
        
        let record = PerformanceRecord {
            test_type: test_type.to_string(),
            model: model.to_string(),
            response_time_ms,
            tokens_per_sec,
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            success,
        };
        
        self.performance_history.push(record);
        
        // Update session stats
        self.session_stats.total_requests += 1;
        if success {
            self.session_stats.successful_requests += 1;
            self.session_stats.total_response_time_ms += response_time_ms;
            
            // Update fastest/slowest
            if let Some(fastest) = self.session_stats.fastest_response_ms {
                if response_time_ms < fastest {
                    self.session_stats.fastest_response_ms = Some(response_time_ms);
                }
            } else {
                self.session_stats.fastest_response_ms = Some(response_time_ms);
            }
            
            if let Some(slowest) = self.session_stats.slowest_response_ms {
                if response_time_ms > slowest {
                    self.session_stats.slowest_response_ms = Some(response_time_ms);
                }
            } else {
                self.session_stats.slowest_response_ms = Some(response_time_ms);
            }
        } else {
            self.session_stats.failed_requests += 1;
        }
        
        // Keep only last 20 records for display
        if self.performance_history.len() > 20 {
            self.performance_history.remove(0);
        }
    }
    
    fn get_performance_summary(&self) -> String {
        if self.session_stats.total_requests == 0 {
            return "No requests made yet".to_string();
        }
        
        let success_rate = (self.session_stats.successful_requests as f64 / self.session_stats.total_requests as f64) * 100.0;
        let avg_time = if self.session_stats.successful_requests > 0 {
            self.session_stats.total_response_time_ms / self.session_stats.successful_requests as u64
        } else {
            0
        };
        
        format!(
            "📊 Session: {} requests | {:.1}% success | Avg: {}ms | Fastest: {}ms | Slowest: {}ms",
            self.session_stats.total_requests,
            success_rate,
            avg_time,
            self.session_stats.fastest_response_ms.unwrap_or(0),
            self.session_stats.slowest_response_ms.unwrap_or(0)
        )
    }
}

impl eframe::App for OllamaUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update cooldown timer
        if let Some(last_preload) = self.last_preload_time {
            let elapsed = last_preload.elapsed().as_secs();
            const COOLDOWN_SECONDS: u64 = 5;
            if elapsed < COOLDOWN_SECONDS {
                self.cooldown_remaining = Some(COOLDOWN_SECONDS - elapsed);
            } else {
                self.cooldown_remaining = None;
            }
        }

        self.check_response();
        self.check_comparison_responses();
        self.check_stress_test_responses();
        
        // Request UI updates for smooth experience on dual-core system
        ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60fps
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🦀 Ollama Rust UI - Performance Test");
                ui.separator();
                if let Some(remaining) = self.cooldown_remaining {
                    ui.colored_label(egui::Color32::YELLOW, format!("⏳ Cooldown: {}s", remaining));
                    ui.label("(Preventing Ollama overload after preload)");
                }
            });
            
            // Performance Stats Panel
            ui.collapsing("⚡ Performance Stats", |ui| {
                ui.horizontal(|ui| {
                    ui.label("💾 App Memory Usage:");
                    // Get current memory usage (approximation)
                    if let Ok(usage) = std::process::Command::new("ps")
                        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                        .output() 
                    {
                        if let Ok(mem_str) = String::from_utf8(usage.stdout) {
                            if let Ok(mem_kb) = mem_str.trim().parse::<f64>() {
                                let mem_mb = mem_kb / 1024.0;
                                ui.label(format!("{:.1} MB", mem_mb));
                                ui.label(format!("({:.1}% of 16GB)", (mem_mb / 16384.0) * 100.0));
                            }
                        }
                    }
                });
                
                // Show system RAM usage
                ui.horizontal(|ui| {
                    ui.label("🖥️ System RAM:");
                    if let Ok(output) = std::process::Command::new("vm_stat").output() {
                        if let Ok(vm_stat) = String::from_utf8(output.stdout) {
                            // Parse free and active memory from vm_stat
                            let lines: Vec<&str> = vm_stat.lines().collect();
                            let mut free_pages = 0;
                            let mut active_pages = 0;
                            
                            for line in lines {
                                if line.contains("Pages free:") {
                                    if let Some(num_str) = line.split_whitespace().nth(2) {
                                        free_pages = num_str.trim_end_matches('.').parse::<u64>().unwrap_or(0);
                                    }
                                }
                                if line.contains("Pages active:") {
                                    if let Some(num_str) = line.split_whitespace().nth(2) {
                                        active_pages = num_str.trim_end_matches('.').parse::<u64>().unwrap_or(0);
                                    }
                                }
                            }
                            
                            let page_size = 4096; // 4KB pages on macOS
                            let free_gb = (free_pages * page_size) as f64 / (1024.0 * 1024.0 * 1024.0);
                            let _active_gb = (active_pages * page_size) as f64 / (1024.0 * 1024.0 * 1024.0);
                            let used_percentage = ((16.0 - free_gb) / 16.0) * 100.0;
                            
                            ui.label(format!("{:.1}GB free / 16GB total", free_gb));
                            ui.label(format!("({:.1}% used)", used_percentage));
                        }
                    }
                });
                
                if let Some(last_time) = self.last_response_time {
                    ui.horizontal(|ui| {
                        ui.label("⏱️ Last Response Time:");
                        ui.label(format!("{:.2}s", last_time as f64 / 1000.0));
                    });
                }
                
                ui.horizontal(|ui| {
                    ui.label("🖥️ System:");
                    ui.label("Dual-Core i5 @ 2.3GHz, 16GB RAM");
                });
                
                ui.horizontal(|ui| {
                    ui.label("🚀 Rust Optimizations:");
                    ui.label("Zero-cost abstractions, async/await, parallel HTTP");
                });
                
                if self.is_comparing {
                    ui.horizontal(|ui| {
                        ui.label("⚡ Current Mode:");
                        ui.label("🏃‍♂️🏃‍♀️ Parallel dual-core processing");
                    });
                } else if self.is_stress_testing {
                    ui.horizontal(|ui| {
                        ui.label("⚡ Current Mode:");
                        ui.label("🔥 Aggressive stress testing (5 parallel)");
                    });
                }
                
                // Session performance summary
                ui.separator();
                ui.label(self.get_performance_summary());
                
                // Performance history table
                if !self.performance_history.is_empty() {
                    ui.separator();
                    ui.collapsing("📈 Performance History (Last 20 requests)", |ui| {
                        egui::Grid::new("performance_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Time");
                                ui.label("Test Type");
                                ui.label("Model");
                                ui.label("Duration");
                                ui.label("Tokens/sec");
                                ui.label("Status");
                                ui.end_row();
                                
                                for record in self.performance_history.iter().rev() {
                                    ui.label(&record.timestamp);
                                    ui.label(&record.test_type);
                                    ui.label(&record.model);
                                    if record.success {
                                        ui.label(format!("{:.2}s", record.response_time_ms as f64 / 1000.0));
                                        if let Some(tokens_sec) = record.tokens_per_sec {
                                            ui.label(format!("{:.1}", tokens_sec));
                                        } else {
                                            ui.label("N/A");
                                        }
                                        ui.label("✅");
                                    } else {
                                        ui.label("Failed");
                                        ui.label("N/A");
                                        ui.label("❌");
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                }
            });
            
            ui.separator();
            
            // Configuration section
            ui.collapsing("⚙️ Configuration", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Ollama URL:");
                    ui.text_edit_singleline(&mut self.ollama_url);
                    if ui.button("🔍 Test Connection").clicked() {
                        self.test_ollama_connection();
                    }
                });
                
                ui.horizontal(|ui| {
                    ui.label("Q4 Model:");
                    ui.text_edit_singleline(&mut self.q4_model_name);
                });
                
                ui.horizontal(|ui| {
                    ui.label("Q5 Model:");
                    ui.text_edit_singleline(&mut self.q5_model_name);
                });
                
                ui.separator();
                ui.label("🧠 RAM Optimization:");
                ui.horizontal(|ui| {
                    if ui.button("🔥 Pre-load Models").clicked() {
                        self.preload_models();
                    }
                    if ui.button("📊 Check Model Status").clicked() {
                        self.check_model_status();
                    }
                });
                ui.label("💡 Pre-loading models into RAM can make subsequent requests 10x faster!");
            });
            
            ui.separator();
            
            // Model selection
            ui.horizontal(|ui| {
                ui.label("Select Model:");
                ui.radio_value(&mut self.selected_model, ModelType::Q4, "Q4 Model");
                ui.radio_value(&mut self.selected_model, ModelType::Q5, "Q5 Model");
            });
            
            ui.separator();
            
            // Prompt input
            ui.label("Prompt:");
            let prompt_response = ui.text_edit_multiline(&mut self.prompt);
            
            ui.horizontal(|ui| {
                let in_cooldown = self.cooldown_remaining.is_some();
                let send_button = ui.add_enabled(!self.is_loading && !self.is_comparing && !self.is_stress_testing && !in_cooldown, egui::Button::new("Send Request"));
                let compare_button = ui.add_enabled(!self.is_loading && !self.is_comparing && !self.is_stress_testing && !in_cooldown, egui::Button::new("🚀 Compare Both Models (Parallel)"));
                let stress_button = ui.add_enabled(!self.is_loading && !self.is_comparing && !self.is_stress_testing && !in_cooldown, egui::Button::new("⚡ Stress Test (5x Rapid)"));
                let preload_button = ui.add_enabled(!self.is_loading && !self.is_comparing && !self.is_stress_testing, egui::Button::new("🔄 Pre-load Models into RAM"));
                let status_button = ui.add_enabled(!self.is_loading && !self.is_comparing && !self.is_stress_testing, egui::Button::new("📊 Check Model Status"));
                
                if let Some(remaining) = self.cooldown_remaining {
                    ui.label(format!("⏳ Cooldown: {}s", remaining));
                }
                
                if send_button.clicked() || (prompt_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.send_request();
                }
                
                if compare_button.clicked() {
                    self.compare_models_parallel();
                }
                
                if stress_button.clicked() {
                    self.stress_test_parallel();
                }
                
                if preload_button.clicked() {
                    self.preload_models();
                }
                
                if status_button.clicked() {
                    self.check_model_status();
                }
                
                if ui.button("Clear").clicked() {
                    self.prompt.clear();
                    self.response.clear();
                    self.q4_response.clear();
                    self.q5_response.clear();
                    self.stats.clear();
                }
                
                if self.is_loading {
                    ui.spinner();
                    ui.label("Generating...");
                } else if self.is_comparing {
                    ui.spinner();
                    ui.label("🏃‍♂️🏃‍♀️ Parallel processing...");
                } else if self.is_stress_testing {
                    ui.spinner();
                    ui.label("⚡ Stress testing in progress...");
                }
            });
            
            ui.horizontal(|ui| {
                if ui.button("Compare Q4 and Q5").clicked() {
                    self.compare_models_parallel();
                }
                
                if self.is_comparing {
                    ui.spinner();
                    ui.label("Comparing models...");
                }
            });
            
            ui.separator();
            
            // Response section - show either single response, parallel comparison, or stress test
            if self.is_stress_testing || !self.stress_test_results.is_empty() {
                ui.heading("⚡ Stress Test Results");
                
                if self.is_stress_testing {
                    ui.label("🔥 Running 5 parallel requests...");
                    
                    // Show real-time progress
                    egui::Grid::new("stress_test_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Request");
                            ui.label("Status");
                            ui.label("Time");
                            ui.end_row();
                            
                            for (i, result) in self.stress_test_results.iter().enumerate() {
                                ui.label(format!("#{}", i + 1));
                                
                                if let Some(time_ms) = result.response_time_ms {
                                    ui.label("✅ Complete");
                                    ui.label(format!("{:.2}s", time_ms as f64 / 1000.0));
                                } else if result.error_message.is_some() {
                                    ui.label("❌ Failed");
                                    ui.label("Error");
                                } else {
                                    ui.label("🏃‍♂️ Running...");
                                    ui.label("...");
                                }
                                ui.end_row();
                            }
                        });
                } else {
                    // Show final results
                    let successful = self.stress_test_results.iter().filter(|r| r.success).count();
                    let failed = self.stress_test_results.len() - successful;
                    
                    ui.label(format!("🏁 Final Results: {}/5 successful, {} failed", successful, failed));
                    
                    egui::Grid::new("stress_results_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Request");
                            ui.label("Status");
                            ui.label("Response Time");
                            ui.label("Error");
                            ui.end_row();
                            
                            for (i, result) in self.stress_test_results.iter().enumerate() {
                                ui.label(format!("#{}", i + 1));
                                
                                if result.success {
                                    ui.label("✅ Success");
                                    if let Some(time_ms) = result.response_time_ms {
                                        ui.label(format!("{:.2}s", time_ms as f64 / 1000.0));
                                    } else {
                                        ui.label("N/A");
                                    }
                                    ui.label("");
                                } else {
                                    ui.label("❌ Failed");
                                    ui.label("N/A");
                                    ui.label(result.error_message.as_deref().unwrap_or("Unknown error"));
                                }
                                ui.end_row();
                            }
                        });
                }
            } else if self.is_comparing || !self.q4_response.is_empty() || !self.q5_response.is_empty() {
                ui.heading("🏁 Parallel Model Comparison");
                
                ui.horizontal(|ui| {
                    // Q4 Results
                    ui.vertical(|ui| {
                        ui.heading("Q4 Model");
                        if let Some(time) = self.q4_time {
                            ui.label(format!("⏱️ Time: {:.2}s", time as f64 / 1000.0));
                        }
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .show(ui, |ui| {
                                ui.text_edit_multiline(&mut self.q4_response);
                            });
                    });
                    
                    ui.separator();
                    
                    // Q5 Results
                    ui.vertical(|ui| {
                        ui.heading("Q5 Model");
                        if let Some(time) = self.q5_time {
                            ui.label(format!("⏱️ Time: {:.2}s", time as f64 / 1000.0));
                        }
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .show(ui, |ui| {
                                ui.text_edit_multiline(&mut self.q5_response);
                            });
                    });
                });
            } else {
                // Single response display
                ui.horizontal(|ui| {
                    ui.label("Response:");
                    if let Some(time) = self.last_response_time {
                        ui.label(format!("({}ms)", time));
                    }
                });
                
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        ui.text_edit_multiline(&mut self.response);
                    });
            }
            
            // Stats section
            if !self.stats.is_empty() {
                ui.separator();
                ui.collapsing("Performance Stats", |ui| {
                    ui.label(&self.stats);
                });
            }
            
            // Parallel comparison results
            if self.is_comparing {
                ui.separator();
                ui.label("🏁 Comparing Q4 and Q5 models...");
                ui.horizontal(|ui| {
                    ui.label("Q4 Response Time:");
                    if let Some(time) = self.q4_time {
                        ui.label(format!("{:.2}s", time as f64 / 1000.0));
                    } else {
                        ui.label("Still running...");
                    }
                });
                
                ui.horizontal(|ui| {
                    ui.label("Q5 Response Time:");
                    if let Some(time) = self.q5_time {
                        ui.label(format!("{:.2}s", time as f64 / 1000.0));
                    } else {
                        ui.label("Still running...");
                    }
                });
                
                if let (Some(q4_time), Some(q5_time)) = (self.q4_time, self.q5_time) {
                    let faster_model = if q4_time < q5_time { "Q4" } else { "Q5" };
                    let time_diff = if q4_time < q5_time { q5_time - q4_time } else { q4_time - q5_time };
                    let percentage_faster = ((time_diff as f64 / q4_time.max(q5_time) as f64) * 100.0).round();
                    
                    ui.separator();
                    ui.label(format!(
                        "🏆 {} is faster by {:.2}s ({:.0}%)",
                        faster_model, time_diff as f64 / 1000.0, percentage_faster
                    ));
                }
            }
        });
        
        // Request repaint for smooth updates
        ctx.request_repaint();
    }
}

fn main() -> Result<(), eframe::Error> {
    println!("🦀 Starting Ollama Rust UI...");
    println!("💡 Make sure Ollama is running: ollama serve");
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Ollama Rust UI - Speed Test")
            .with_resizable(true),
        ..Default::default()
    };
    
    println!("🚀 Launching GUI...");
    
    eframe::run_native(
        "Ollama UI",
        options,
        Box::new(|_cc| Ok(Box::new(OllamaUI::default()))),
    )
}
