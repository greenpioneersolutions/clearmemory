# curator/ — Retrieval Result Filtering Before Context Injection

## Role in the Architecture

The `curator` module sits between the retrieval pipeline and the context compiler. Its job is to receive retrieved memory excerpts and the original query, then identify and extract only the portions relevant to the query. This reduces token count before injection: a memory about a long meeting might contain 20 topics, but only the 2 sentences about authentication matter for an auth-related query.

In the tiered architecture, the curator is active only in Tier 2 (LocalLlm) and Tier 3 (Cloud). In Tier 1 (Offline), retrieval results pass directly to the context compiler without curator processing. The production curator model is Qwen3-0.6B (~1.2GB quantized), which provides fast (~1 second) inference for filtering tasks. The candle framework integration for running Qwen3-0.6B locally is planned but not yet implemented -- the module currently provides the trait interface and a noop passthrough implementation.

## File-by-File Descriptions

### mod.rs

Module declaration only. Re-exports: `qwen`.

### qwen.rs

Defines the curator trait, data types, and the noop implementation. This file is the interface contract that both the engine and future model implementations depend on.

**Key types:**

- `MemoryExcerpt` — Input to the curator. A retrieved memory before filtering. Fields:
  - `memory_id: String` — The memory's unique identifier
  - `content: String` — The memory's text content (typically the summary from retrieval)
  - `relevance_score: f64` — The score from the retrieval/rerank pipeline
  
  Derives `Debug`, `Clone`, `Serialize`, `Deserialize`.

- `CuratedExcerpt` — Output from the curator. The filtered/trimmed portion of a memory. Fields:
  - `memory_id: String` — Same memory ID as the input
  - `content: String` — The relevant portion of the memory (may be shorter than the input)
  - `relevance_score: f64` — May be adjusted by the curator based on actual relevance
  
  Derives `Debug`, `Clone`, `Serialize`, `Deserialize`.

- `CuratorModel` — The core trait. Must be `Send + Sync` for use across async boundaries.
  ```rust
  fn curate(&self, query: &str, memories: &[MemoryExcerpt]) -> Result<Vec<CuratedExcerpt>>;
  ```
  Implementations may trim content to relevant portions, reorder by relevance, filter out irrelevant memories entirely, or adjust relevance scores.

- `NoopCurator` — Passthrough implementation for Tier 1. Converts every `MemoryExcerpt` to a `CuratedExcerpt` with identical content and score. Derives `Debug`, `Default`.

**How the engine uses it:** In `engine::Engine::recall`, after retrieval and reranking, the engine constructs `MemoryExcerpt` values from the results and calls `curator.curate(query, &excerpts)`. Currently this always uses `NoopCurator`. The code path for Tier 2+ is structurally in place:

```rust
let curated_results = if self.config.general.tier != Tier::Offline {
    let curator = NoopCurator; // Will be replaced with Qwen3-0.6B
    // ... curate and use results
} else {
    filtered_results
};
```

## Key Public Types Other Modules Depend On

- `CuratorModel` trait — The interface that future Qwen3-0.6B and cloud-based curator implementations must satisfy
- `NoopCurator` — Used by the engine in Tier 1 and as the current default for all tiers
- `MemoryExcerpt` — Constructed by the engine from retrieval results
- `CuratedExcerpt` — Consumed by the engine before context compilation

## Relevant config.toml Keys

- `[general] tier` — Determines whether the curator is active (`local_llm` or `cloud` enables it; `offline` skips it)
- `[models] curator` — Which curator model to use (default: `"qwen3-0.6b"`)
- `[models] curator_resident` — Whether to keep the curator model loaded in RAM (default: `true`)
- `[models] model_path` — Custom path for pre-staged model files

## Deferred / Planned Functionality

- **Qwen3-0.6B inference via candle:** The primary planned work for this module. A `QwenCurator` struct implementing `CuratorModel` that:
  1. Loads the Qwen3-0.6B quantized model via `candle-core` and `candle-transformers`
  2. Constructs a prompt with the query and memory excerpts
  3. Runs inference to identify relevant portions
  4. Parses the model output into `CuratedExcerpt` values
  
  The model download and caching infrastructure (model path, checksums, verification) is defined in the config but not yet wired to candle.

- **Cloud curator (Tier 3):** For Tier 3 deployments, the curator could use a cloud API (Claude Haiku, GPT-4o-mini, etc.) instead of local inference. The `CuratorModel` trait is generic enough to support this -- a `CloudCurator` implementation would make an API call instead of running local inference. The `[cloud]` config section already defines `curator_model = "claude-haiku-4-5-20251001"`.

- **Token savings metrics:** The curator should report how many tokens it saved (input tokens vs output tokens) for the `curator.tokens_saved` OpenTelemetry counter defined in the observability spec.

- **Batch processing:** The current `curate` signature takes all excerpts at once, which works well for a single model call. For large result sets, the implementation may need to batch excerpts to stay within the model's context window.
