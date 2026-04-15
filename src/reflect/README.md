# reflect/ -- Multi-Memory Synthesis and Mental Model Persistence

## Role in the Architecture

The `reflect` module implements the synthesis layer of Clear Memory. While the retrieval pipeline finds relevant memories and the context compiler assembles them, `reflect` goes a step further: it synthesizes across multiple memories to produce coherent narratives, project summaries, and mental models. This is the intelligence layer that turns raw recalled memories into structured understanding.

Reflect is a **Tier 2+ feature**. In Tier 1 (fully offline) deployments, the reflect engine returns a message indicating that Tier 2 or higher is required. In Tier 2, synthesis is performed by a bundled Qwen3-4B model via the `candle` framework. In Tier 3, cloud LLMs (Claude, GPT, Gemini) can be used for highest quality synthesis. The actual candle-based Qwen3-4B inference is **planned but not yet integrated** -- the module currently provides the trait interface and a stub implementation.

Mental models are the persistent output of reflect operations. They are markdown files stored in `~/.clearmemory/mental_models/` that capture synthesized views of topics, projects, or teams. They are generated and updated by the reflect engine and can be served back to users or injected into context.

## File-by-File Description

### `mod.rs`

Module root. Re-exports the two submodules:

- `pub mod mental_models;`
- `pub mod synthesizer;`

### `synthesizer.rs`

Defines the core trait for reflect engines and provides the Tier 1 stub.

**Key types:**

- **`ReflectEngine` (trait)** -- The primary interface that all reflect implementations must satisfy. Requires `Send + Sync` for use across async tasks.
  - `fn synthesize(&self, query: &str, memories: &[String]) -> Result<String>` -- Takes a query/topic and a slice of memory contents, returns a synthesized narrative string.
  
- **`StubReflectEngine`** -- Default implementation for Tier 1 deployments. Always returns `"Reflect requires Tier 2 or higher"`. Implements `Default`.

**Planned:** A `QwenReflectEngine` struct that loads the Qwen3-4B model via `candle-core` + `candle-transformers` and performs actual multi-document synthesis. The 4B parameter size is the minimum for coherent multi-document synthesis -- do NOT downgrade to 0.6B for reflect.

### `mental_models.rs`

Handles persistence of mental models as markdown files on disk.

**Key types:**

- **`MentalModel`** -- A synthesized mental model on a specific topic. Fields:
  - `topic: String` -- The topic or title of the mental model
  - `content: String` -- The synthesized markdown content
  - `updated_at: DateTime<Utc>` -- When this model was last updated

**Key functions:**

- **`save_mental_model(dir: &Path, model: &MentalModel) -> Result<()>`** -- Writes a mental model as a markdown file. Creates the directory if needed. The filename is derived from the topic by replacing non-alphanumeric characters with hyphens and lowercasing (e.g., "Auth Migration" becomes `auth-migration.md`). The file format is:
  ```
  # <topic>
  
  _Updated: <rfc3339 timestamp>_
  
  <content>
  ```

- **`load_mental_model(dir: &Path, topic: &str) -> Result<Option<MentalModel>>`** -- Loads a mental model from disk by topic name. Returns `None` if the file does not exist. Parses the markdown format back into a `MentalModel` struct.

- **`topic_to_filename(topic: &str) -> String`** (private) -- Converts a topic string to a safe filename component.

- **`parse_mental_model_markdown(topic: &str, raw: &str) -> Result<MentalModel>`** (private) -- Parses markdown content back into a `MentalModel`, extracting the `updated_at` timestamp from the `_Updated: ..._ ` line.

## Key Public Types Other Modules Depend On

- **`ReflectEngine`** -- Used by the MCP server (`clearmemory_reflect` tool) and HTTP handlers to perform synthesis. The server holds a `Box<dyn ReflectEngine>` or `Arc<dyn ReflectEngine>` and dispatches to either the stub or a real implementation based on the configured tier.
- **`MentalModel`** -- Used when the reflect tool generates or updates mental models that are persisted to `~/.clearmemory/mental_models/`.

## Relevant config.toml Keys

```toml
[general]
tier = "local_llm"              # "offline" uses StubReflectEngine; "local_llm" or "cloud" use real engines

[models]
reflect = "qwen3-4b"            # Model identifier for Tier 2+ reflect
reflect_resident = false         # true = keep model in RAM; false = load/unload on demand
```

## Deferred / Planned Functionality

- **Qwen3-4B inference via candle:** The actual local LLM synthesis engine is not yet implemented. When built, it will implement the `ReflectEngine` trait and use `candle-core` + `candle-transformers` for inference.
- **Cloud reflect engine:** A Tier 3 implementation that calls cloud APIs (Claude, GPT, Gemini) for highest quality synthesis.
- **Mental model updates:** Logic to incrementally update existing mental models when new relevant memories are ingested, rather than regenerating from scratch.
- **Mental model indexing:** Making mental models searchable via the retrieval pipeline.
