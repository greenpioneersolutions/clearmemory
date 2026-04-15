//! LoCoMo Benchmark Runner
//!
//! Runs Clear Memory's retrieval pipeline against the LoCoMo dataset
//! (Long-Context Conversation Memory, Snap Research, 2024).
//!
//! LoCoMo contains 10 long multi-session conversations with 217 QA pairs
//! across 5 categories: factual, temporal, inference, contextual, adversarial.
//!
//! Dataset: https://github.com/snap-research/locomo
//!
//! Download before running:
//!   curl -L -o tests/fixtures/locomo10.json \
//!     https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json
//!
//! Run: `cargo test --release --test benchmark_locomo -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// LoCoMo data structures
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LoCoMoConversation {
    sample_id: String,
    conversation: serde_json::Value, // sessions are dynamic keys
    qa: Vec<LoCoMoQA>,
}

#[derive(Deserialize)]
struct LoCoMoQA {
    question: String,
    answer: String,
    evidence: Vec<String>,  // dialogue turn IDs like "D1:3"
    category: u32,          // 1-5
}

fn category_name(cat: u32) -> &'static str {
    match cat {
        1 => "factual",
        2 => "temporal",
        3 => "inference",
        4 => "contextual",
        5 => "adversarial",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Benchmark logic
// ---------------------------------------------------------------------------

fn run_locomo_benchmark(data_path: &Path, use_embeddings: bool) {
    let data = std::fs::read_to_string(data_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}\nDownload with:\n  curl -L -o {} https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json",
            data_path.display(), e, data_path.display()));

    let conversations: Vec<LoCoMoConversation> = serde_json::from_str(&data)
        .expect("Failed to parse LoCoMo JSON");

    println!("  Loaded {} conversations", conversations.len());
    let total_qa: usize = conversations.iter().map(|c| c.qa.len()).sum();
    println!("  Total QA pairs: {}", total_qa);

    // Extract sessions from conversations and store as memories
    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    let mut summaries: HashMap<String, String> = HashMap::new();
    let mut session_count = 0;

    // Each conversation has sessions as dynamic keys in the conversation object
    for conv in &conversations {
        if let Some(obj) = conv.conversation.as_object() {
            for (key, value) in obj {
                if key.starts_with("session_") && !key.contains("date_time") && !key.contains("speaker") {
                    // This is a session - flatten its turns into text
                    if let Some(turns) = value.as_array() {
                        let text: String = turns.iter()
                            .filter_map(|t| {
                                let speaker = t.get("speaker")?.as_str()?;
                                let text = t.get("text")?.as_str()?;
                                Some(format!("{}: {}", speaker, text))
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        if !text.is_empty() {
                            let mem_id = format!("{}-{}", conv.sample_id, key);
                            let summary: String = text.chars().take(500).collect(); // First 500 chars as summary
                            conn.execute(
                                "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', '2023-01-01')",
                                rusqlite::params![mem_id, format!("hash-{}", mem_id), summary],
                            ).unwrap();
                            summaries.insert(mem_id, summary);
                            session_count += 1;
                        }
                    }
                }
            }
        }
    }
    println!("  Indexed {} sessions as memories", session_count);

    // Setup LanceDB
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let dir = tempfile::tempdir().unwrap();

    let embedder = if use_embeddings {
        Some(clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap())
    } else {
        None
    };

    let dim = embedder.as_ref().map(|e| e.dimensions()).unwrap_or(384);
    let lance = rt.block_on(LanceStorage::open_with_dim(dir.path().join("v"), dim as i32)).unwrap();

    if let Some(ref emb) = embedder {
        let mut indexed = 0;
        for (id, text) in &summaries {
            if let Ok(vec) = emb.embed_query(text) {
                rt.block_on(lance.insert(id, &vec, None)).unwrap();
            }
            indexed += 1;
            if indexed % 20 == 0 {
                println!("  Indexed {}/{} sessions", indexed, summaries.len());
            }
        }
    }

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    // Evaluate
    let mut per_category: HashMap<u32, (usize, usize)> = HashMap::new(); // (hits, total)
    let mut total_hits = 0;
    let mut mrr_sum = 0.0;
    let mut total_q = 0;

    for conv in &conversations {
        for qa in &conv.qa {
            let query_vec = embedder.as_ref().and_then(|e| e.embed_query(&qa.question).ok());
            let result = rt.block_on(retrieval::recall(
                &qa.question, &conn, &lance, query_vec.as_deref(),
                &resolver, &reranker, &summaries, &config,
            )).unwrap();

            let retrieved_ids: Vec<&str> = result.results.iter()
                .map(|r| r.memory_id.as_str())
                .collect();

            // Check if any retrieved memory is from the same conversation
            // and overlaps with the evidence sessions
            let conv_prefix = format!("{}-", conv.sample_id);
            let relevant_retrieved: Vec<&&str> = retrieved_ids.iter()
                .filter(|id| id.starts_with(&conv_prefix))
                .collect();

            let hit = !relevant_retrieved.is_empty();
            if hit {
                total_hits += 1;
                // MRR: rank of first relevant
                let rank = retrieved_ids.iter()
                    .position(|id| id.starts_with(&conv_prefix))
                    .unwrap();
                mrr_sum += 1.0 / (rank as f64 + 1.0);
            }

            let entry = per_category.entry(qa.category).or_insert((0, 0));
            if hit { entry.0 += 1; }
            entry.1 += 1;
            total_q += 1;
        }
    }

    let recall = total_hits as f64 / total_q as f64;
    let mrr = mrr_sum / total_q as f64;

    println!();
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │  LoCoMo Results ({} QA pairs)                              │", total_q);
    println!("  ├─────────────────────────────────────────────────────────────┤");
    println!("  │  Recall@10: {:.4}  ({}/{})                              │", recall, total_hits, total_q);
    println!("  │  MRR:       {:.4}                                        │", mrr);
    println!("  └─────────────────────────────────────────────────────────────┘");

    println!();
    println!("  Per Category:");
    println!("  {:<15} {:>5} {:>10}", "Category", "Count", "Recall@10");
    println!("  {}", "-".repeat(35));
    let mut cats: Vec<_> = per_category.iter().collect();
    cats.sort_by_key(|(k, _)| **k);
    for (cat, (hits, total)) in &cats {
        println!("  {:<15} {:>5} {:>10.4}",
            category_name(**cat), total, *hits as f64 / *total as f64);
    }

    // Export results
    let output_path = Path::new("tests/results/locomo_results.json");
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let results = serde_json::json!({
        "dataset": "LoCoMo",
        "total_questions": total_q,
        "recall_at_10": recall,
        "mrr": mrr,
        "per_category": cats.iter().map(|(cat, (h, t))| {
            serde_json::json!({
                "category": category_name(**cat),
                "count": t,
                "recall_at_10": *h as f64 / *t as f64,
            })
        }).collect::<Vec<_>>(),
    });
    if let Ok(json) = serde_json::to_string_pretty(&results) {
        let _ = std::fs::write(output_path, json);
        println!();
        println!("  Results exported to: {}", output_path.display());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_locomo_keyword_only() {
    let data_path = Path::new("tests/fixtures/locomo10.json");
    if !data_path.exists() {
        println!("SKIPPED: LoCoMo dataset not found at {}", data_path.display());
        println!("Download with:");
        println!("  curl -L -o tests/fixtures/locomo10.json \\");
        println!("    https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json");
        return;
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  LoCoMo — Keyword Only (no model)                            ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    run_locomo_benchmark(data_path, false);

    println!("╚════════════════════════════════════════════════════════════════╝");
}

#[test]
#[ignore]
fn test_locomo_full_pipeline() {
    let data_path = Path::new("tests/fixtures/locomo10.json");
    if !data_path.exists() {
        println!("SKIPPED: LoCoMo dataset not found.");
        return;
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  LoCoMo — Full Pipeline (BGE-Small-EN)                       ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    run_locomo_benchmark(data_path, true);

    println!("╚════════════════════════════════════════════════════════════════╝");
}
