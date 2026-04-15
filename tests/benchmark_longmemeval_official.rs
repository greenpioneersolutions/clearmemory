//! Official LongMemEval Benchmark Runner
//!
//! Runs Clear Memory's retrieval pipeline against the OFFICIAL LongMemEval
//! dataset (Wu et al., ICLR 2025). This produces numbers that are directly
//! comparable to published results from MemPalace, Hindsight, Zep, and Mem0.
//!
//! Dataset: https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned
//!
//! The dataset must be downloaded before running:
//!   curl -L -o tests/fixtures/longmemeval_oracle.json \
//!     https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_oracle.json
//!
//! Run:
//!   cargo test --release --test benchmark_longmemeval_official -- --nocapture --ignored

use clearmemory::migration;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// LongMemEval data structures
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LongMemEvalQuestion {
    question_id: String,
    question_type: String,
    question: String,
    #[allow(dead_code)]
    answer: serde_json::Value, // Can be string or number
    #[allow(dead_code)]
    question_date: String,
    #[allow(dead_code)]
    haystack_dates: Vec<String>,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<Turn>>,
    answer_session_ids: Vec<String>,
}

#[derive(Deserialize)]
struct Turn {
    role: String,
    content: String,
    #[allow(dead_code)]
    has_answer: bool,
}

// ---------------------------------------------------------------------------
// Evaluation metrics
// ---------------------------------------------------------------------------

struct RetrievalMetrics {
    recall_any_at_5: f64,
    recall_any_at_10: f64,
    recall_all_at_5: f64,
    recall_all_at_10: f64,
    mrr: f64,
    ndcg_at_10: f64,
}

#[derive(Default)]
struct PerTypeMetrics {
    count: usize,
    recall_any_5_sum: f64,
    recall_any_10_sum: f64,
    recall_all_5_sum: f64,
    recall_all_10_sum: f64,
    mrr_sum: f64,
}

struct QuestionResult {
    question_id: String,
    question_type: String,
    recall_any_at_5: bool,
    recall_any_at_10: bool,
    recall_all_at_5: bool,
    recall_all_at_10: bool,
    reciprocal_rank: f64,
    retrieved_session_ids: Vec<String>,
    expected_session_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core benchmark logic
// ---------------------------------------------------------------------------

fn flatten_session_to_text(session: &[Turn]) -> String {
    session.iter()
        .map(|t| format!("[{}]: {}", t.role, t.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_longmemeval_benchmark(
    data_path: &Path,
    use_embeddings: bool,
) -> (RetrievalMetrics, Vec<QuestionResult>) {
    // Load dataset
    let data = fs::read_to_string(data_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}\nDownload with:\n  curl -L -o {} https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_oracle.json",
            data_path.display(), e, data_path.display()));

    let questions: Vec<LongMemEvalQuestion> = serde_json::from_str(&data)
        .expect("Failed to parse LongMemEval JSON");

    println!("  Loaded {} questions", questions.len());

    // Collect all unique sessions across all questions
    let mut all_sessions: HashMap<String, String> = HashMap::new();
    for q in &questions {
        for (i, session) in q.haystack_sessions.iter().enumerate() {
            let session_id = &q.haystack_session_ids[i];
            if !all_sessions.contains_key(session_id) {
                all_sessions.insert(session_id.clone(), flatten_session_to_text(session));
            }
        }
    }
    println!("  {} unique sessions in haystack", all_sessions.len());

    // Setup SQLite
    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    for (session_id, text) in &all_sessions {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', '2023-01-01T00:00:00Z')",
            rusqlite::params![session_id, format!("hash-{}", session_id), text],
        ).unwrap();
    }

    let summaries: HashMap<String, String> = all_sessions.iter()
        .map(|(id, text)| (id.clone(), text.clone()))
        .collect();

    // Setup LanceDB + embeddings
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
        let total = all_sessions.len();
        let mut indexed = 0;
        for (session_id, text) in &all_sessions {
            // Embed first 512 chars as summary (full sessions are too long for single embedding)
            let summary: String = text.chars().take(512).collect();
            if let Ok(vec) = emb.embed_query(&summary) {
                rt.block_on(lance.insert(session_id, &vec, None)).unwrap();
            }
            indexed += 1;
            if indexed % 100 == 0 {
                println!("  Indexed {}/{} sessions", indexed, total);
            }
        }
        println!("  Indexed {}/{} sessions", indexed, total);
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

    // Evaluate each question
    let mut results: Vec<QuestionResult> = Vec::new();
    let mut per_type: HashMap<String, PerTypeMetrics> = HashMap::new();

    for (qi, q) in questions.iter().enumerate() {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(&q.question).ok());
        let query_slice = query_vec.as_deref();

        let result = rt.block_on(retrieval::recall(
            &q.question, &conn, &lance, query_slice,
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let retrieved_ids: Vec<String> = result.results.iter()
            .map(|r| r.memory_id.clone())
            .collect();

        let expected: HashSet<&String> = q.answer_session_ids.iter().collect();
        let retrieved_set: HashSet<&String> = retrieved_ids.iter().collect();

        // recall_any: at least one expected session found
        let any_in_top_5 = retrieved_ids.iter().take(5).any(|id| expected.contains(id));
        let any_in_top_10 = retrieved_ids.iter().take(10).any(|id| expected.contains(id));

        // recall_all: ALL expected sessions found
        let all_in_top_5 = expected.iter().all(|id| retrieved_ids.iter().take(5).any(|r| r == *id));
        let all_in_top_10 = expected.iter().all(|id| retrieved_ids.iter().take(10).any(|r| r == *id));

        // MRR: reciprocal rank of first relevant result
        let rr = retrieved_ids.iter()
            .position(|id| expected.contains(id))
            .map(|pos| 1.0 / (pos as f64 + 1.0))
            .unwrap_or(0.0);

        let qr = QuestionResult {
            question_id: q.question_id.clone(),
            question_type: q.question_type.clone(),
            recall_any_at_5: any_in_top_5,
            recall_any_at_10: any_in_top_10,
            recall_all_at_5: all_in_top_5,
            recall_all_at_10: all_in_top_10,
            reciprocal_rank: rr,
            retrieved_session_ids: retrieved_ids,
            expected_session_ids: q.answer_session_ids.clone(),
        };

        let entry = per_type.entry(q.question_type.clone()).or_default();
        entry.count += 1;
        entry.recall_any_5_sum += if any_in_top_5 { 1.0 } else { 0.0 };
        entry.recall_any_10_sum += if any_in_top_10 { 1.0 } else { 0.0 };
        entry.recall_all_5_sum += if all_in_top_5 { 1.0 } else { 0.0 };
        entry.recall_all_10_sum += if all_in_top_10 { 1.0 } else { 0.0 };
        entry.mrr_sum += rr;

        results.push(qr);

        if (qi + 1) % 50 == 0 {
            println!("  Evaluated {}/{} questions", qi + 1, questions.len());
        }
    }

    let n = questions.len() as f64;
    let metrics = RetrievalMetrics {
        recall_any_at_5: results.iter().filter(|r| r.recall_any_at_5).count() as f64 / n,
        recall_any_at_10: results.iter().filter(|r| r.recall_any_at_10).count() as f64 / n,
        recall_all_at_5: results.iter().filter(|r| r.recall_all_at_5).count() as f64 / n,
        recall_all_at_10: results.iter().filter(|r| r.recall_all_at_10).count() as f64 / n,
        mrr: results.iter().map(|r| r.reciprocal_rank).sum::<f64>() / n,
        ndcg_at_10: 0.0, // TODO: proper NDCG computation
    };

    // Print results
    println!();
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │  OFFICIAL LongMemEval Results (500 questions)               │");
    println!("  ├─────────────────────────────────────────────────────────────┤");
    println!("  │  Recall_any@5:  {:.4}  ({}/500)                          │",
        metrics.recall_any_at_5,
        results.iter().filter(|r| r.recall_any_at_5).count());
    println!("  │  Recall_any@10: {:.4}  ({}/500)                          │",
        metrics.recall_any_at_10,
        results.iter().filter(|r| r.recall_any_at_10).count());
    println!("  │  Recall_all@5:  {:.4}  ({}/500)                          │",
        metrics.recall_all_at_5,
        results.iter().filter(|r| r.recall_all_at_5).count());
    println!("  │  Recall_all@10: {:.4}  ({}/500)                          │",
        metrics.recall_all_at_10,
        results.iter().filter(|r| r.recall_all_at_10).count());
    println!("  │  MRR:           {:.4}                                     │", metrics.mrr);
    println!("  └─────────────────────────────────────────────────────────────┘");
    println!();

    // Per-type breakdown
    println!("  Per Question Type:");
    println!("  {:<30} {:>5} {:>10} {:>10} {:>10}", "Type", "Count", "Any@5", "Any@10", "MRR");
    println!("  {}", "-".repeat(70));
    let mut types: Vec<_> = per_type.iter().collect();
    types.sort_by_key(|(k, _)| (*k).clone());
    for (qtype, m) in &types {
        let c = m.count as f64;
        println!("  {:<30} {:>5} {:>10.4} {:>10.4} {:>10.4}",
            qtype, m.count,
            m.recall_any_5_sum / c,
            m.recall_any_10_sum / c,
            m.mrr_sum / c);
    }

    // Export per-question results as JSON
    let results_json: Vec<serde_json::Value> = results.iter().map(|r| {
        serde_json::json!({
            "question_id": r.question_id,
            "question_type": r.question_type,
            "recall_any@5": r.recall_any_at_5,
            "recall_any@10": r.recall_any_at_10,
            "recall_all@5": r.recall_all_at_5,
            "recall_all@10": r.recall_all_at_10,
            "reciprocal_rank": r.reciprocal_rank,
            "retrieved_top10": r.retrieved_session_ids,
            "expected": r.expected_session_ids,
        })
    }).collect();

    // Write per-question results to file
    let output_path = Path::new("tests/results/longmemeval_per_question.json");
    if let Some(parent) = output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&results_json) {
        let _ = fs::write(output_path, json);
        println!();
        println!("  Per-question results written to: {}", output_path.display());
    }

    (metrics, results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Keyword-only (no embeddings) — fast, no model download
#[test]
fn test_longmemeval_official_keyword_only() {
    let data_path = Path::new("tests/fixtures/longmemeval_oracle.json");
    if !data_path.exists() {
        println!("SKIPPED: LongMemEval dataset not found at {}", data_path.display());
        println!("Download with:");
        println!("  curl -L -o tests/fixtures/longmemeval_oracle.json \\");
        println!("    https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_oracle.json");
        return;
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  OFFICIAL LongMemEval — Keyword Only (no model)              ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let (metrics, _) = run_longmemeval_benchmark(data_path, false);

    println!("╚════════════════════════════════════════════════════════════════╝");

    // No assertions — this is a measurement benchmark, not a pass/fail test
    println!();
    println!("  Summary: Recall_any@10 = {:.1}%", metrics.recall_any_at_10 * 100.0);
}

/// Full pipeline with BGE-Small-EN embeddings
#[test]
#[ignore] // Requires ~50MB model download + takes several minutes
fn test_longmemeval_official_full_pipeline() {
    let data_path = Path::new("tests/fixtures/longmemeval_oracle.json");
    if !data_path.exists() {
        println!("SKIPPED: LongMemEval dataset not found.");
        println!("Download with:");
        println!("  curl -L -o tests/fixtures/longmemeval_oracle.json \\");
        println!("    https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_oracle.json");
        return;
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  OFFICIAL LongMemEval — Full Pipeline (BGE-Small-EN)         ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let (metrics, _) = run_longmemeval_benchmark(data_path, true);

    println!("╚════════════════════════════════════════════════════════════════╝");

    println!();
    println!("  Summary: Recall_any@10 = {:.1}%", metrics.recall_any_at_10 * 100.0);
    println!();
    println!("  Compare against published results:");
    println!("    MemPalace:  96.6% (R@5, raw mode)");
    println!("    Hindsight:  91.4%");
    println!("    Zep:        63.8%");
    println!("    Mem0:       49.0%");
}
