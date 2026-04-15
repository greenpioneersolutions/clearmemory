# context/ — Token-Budget-Aware Context Compilation

## Role in the Architecture

The `context` module is Clear Memory's output stage: it takes retrieval results and assembles them into a single text payload that gets injected into an LLM prompt. The context compiler operates within a configurable token budget (default: 4096 tokens) and fills content in strict priority order across four memory tiers (L0 through L3). It also deduplicates against content the CLI already has loaded (e.g., CLAUDE.md contents, files passed via --add-dir) to avoid wasting tokens on information the LLM already sees.

In the overall architecture, the context module sits between the retrieval pipeline (which produces scored results) and the final prompt assembly. The engine calls the context compiler after retrieval and optional curator filtering to produce the optimized payload that makes Clear Memory's token-cost savings possible.

## File-by-File Descriptions

### mod.rs

Module declaration only. Re-exports: `compiler`, `dedup`, `layers`.

### layers.rs

Defines the four memory tiers and their data structures, plus activation logic for on-demand tiers.

**Key types:**

- `L0Context` — Identity tier (~50 tokens). Always loaded. Fields: `tier: Tier`, `active_stream: Option<String>`, `user_id: Option<String>`. Has `render()` method that produces a pipe-separated string like `"Tier: offline | Active stream: my-project | User: user1"`. Has `estimate_tokens()` method (len/4 + 1 heuristic).
- `L1Context` — Working set tier (~200-500 tokens). Always loaded. Fields: `recent_facts: Vec<String>`, `stream_description: Option<String>`. Has `render()` method that joins stream description and facts with newlines. Has `estimate_tokens()`.
- `L2Memory` — Recall tier (on-demand). A single relevant memory from search within the active stream. Fields: `memory_id: String`, `summary: String`, `score: f64`.
- `L3Memory` — Deep search tier (on-demand). Cross-stream, cross-project retrieval. Fields: `memory_id: String`, `summary: String`, `score: f64`, `source_stream: Option<String>`.

**Activation functions:**

- `should_activate_l2(query)` — Returns true if the query is non-empty and longer than 5 characters. L2 activates for meaningful queries, not status checks.
- `should_activate_l3(query, l2_results_count, l2_max_score)` — Returns true if L2 results are insufficient: fewer than 3 results OR the max score is below 0.3.

### compiler.rs

The core context assembly engine. Fills tiers in priority order within the token budget.

**Key types:**

- `CompiledContext` — The output: `text: String` (the assembled payload), `tokens_used: usize`, `tokens_budget: usize`, `l0_tokens: usize`, `l1_tokens: usize`, `l2_count: usize`, `l3_count: usize`.
- `ContextCompiler` — The compiler. Fields: `budget: usize`, `dedup: ContextDedup`.

**Key functions:**

- `ContextCompiler::new(budget)` — Creates a compiler with the given token budget and a fresh dedup instance.
- `ContextCompiler::with_dedup(budget, dedup)` — Creates a compiler with pre-registered dedup content.
- `ContextCompiler::compile(l0, l1, l2, l3)` — The main method. Fills content in strict order:
  1. **L0**: Renders identity context. Added if it fits in the remaining budget.
  2. **L1**: Renders working set. Added if non-empty and fits.
  3. **L2**: Iterates recall results (highest score first). Skips duplicates (via `dedup.is_duplicate`). Each memory added as `"- {summary}\n"` if it fits.
  4. **L3**: Iterates deep search results. Same dedup and budget logic. Prefixes with `"[{source_stream}] "` when a source stream is present.
  
  Stops filling each tier when the budget is exhausted.

**Token estimation:**

- `estimate_tokens(text)` — Rough heuristic: `text.len() / 4`, minimum 1. Conservative for English text (~4 characters per token).

### dedup.rs

Content deduplication against known context sources. Prevents injecting memories that overlap with content the LLM already has in its context window.

**Key types:**

- `ContextDedup` — Tracks SHA-256 hashes of known content. Fields: `known_hashes: HashSet<String>`. Implements `Default`.

**Key functions:**

- `ContextDedup::register_known_content(content)` — Registers the full content hash AND paragraph-level hashes. Paragraphs are split by `"\n\n"` and only paragraphs longer than 50 characters get their own hash entry. This enables partial overlap detection (a memory that matches a paragraph from CLAUDE.md will be caught).
- `ContextDedup::is_duplicate(content)` — Returns true if the content's hash matches any registered hash.
- `ContextDedup::known_count()` — Number of registered hashes.

## Key Public Types Other Modules Depend On

- `ContextCompiler` — used by the engine to assemble final context payloads
- `CompiledContext` — the output type returned to the server/CLI
- `L0Context`, `L1Context`, `L2Memory`, `L3Memory` — used by the engine to construct tier data from retrieval results
- `ContextDedup` — used by the engine to register CLI-provided context before compilation
- `should_activate_l2`, `should_activate_l3` — used by the engine to decide which retrieval tiers to run

## Relevant config.toml Keys

- `[retrieval] token_budget` — Maximum tokens for context injection (default: `4096`)

## Deferred / Planned Functionality

- **Tiktoken-based token counting:** The current token estimation uses a simple `len/4` heuristic. For production accuracy, especially with non-English text, this should be replaced with a proper tokenizer (tiktoken or similar) that matches the target LLM's tokenization.
- **Curator integration:** The context compiler currently receives raw retrieval summaries. When the curator model (Qwen3-0.6B) is integrated, L2/L3 entries will contain curator-filtered excerpts rather than full summaries, further improving token efficiency.
- **Progressive loading hooks:** The spec describes a two-step pattern where the AI receives summaries first and then calls `expand` for full content. The context compiler currently works with summaries only. Integration with the expand flow for selective full-content injection is not yet implemented in the compiler.
