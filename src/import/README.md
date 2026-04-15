# import/ — Multi-Format Memory Import Pipeline

## Role in the Architecture

The `import` module is Clear Memory's ingestion layer. It parses conversation transcripts and documents from seven different source formats into a common `RawMemory` representation, which the engine then stores through the standard retain path (secret scanning, encryption, verbatim storage, SQLite, LanceDB embedding, entity resolution, audit logging).

This module is critical for onboarding: users typically have years of conversation history in Claude Code, ChatGPT, Copilot, Slack, or plain markdown files. The import module makes all of that history immediately searchable. It also includes a CSV-to-Clear-Format converter for enterprise workflows where non-technical users produce data in spreadsheets.

## File-by-File Descriptions

### mod.rs

The central dispatcher. Defines the common `RawMemory` type, the `ImportFormat` enum, auto-detection logic, and the top-level `parse` function.

**Key types:**

- `RawMemory` — The common intermediate representation for all imports. Fields: `content: String`, `summary: Option<String>`, `source_format: String`, `date: Option<String>`, `author: Option<String>`, `tags: Vec<(String, String)>`, `metadata: serde_json::Value`. Derives `Serialize`, `Deserialize`, `Debug`, `Clone`.
- `ImportFormat` — Enum: `ClaudeCode`, `Copilot`, `ChatGpt`, `Slack`, `Markdown`, `ClearFormat`, `Auto`. Derives `PartialEq`, `Eq`, `Copy`.

**Key functions:**

- `ImportFormat::parse_name(s)` — Converts string to format enum. Accepted values: `"claude_code"`, `"copilot"`, `"chatgpt"`, `"slack"`, `"markdown"`, `"clear"`, `"auto"`. Returns `ImportError::UnsupportedFormat` for unknown strings.
- `detect_format(path)` — Auto-detection logic. Checks in order:
  1. File extension: `.clear` -> ClearFormat, `.md`/`.txt` -> Markdown, `.csv`/`.xlsx` -> ClearFormat
  2. JSON file inspection: checks for `"clear_format_version"` (ClearFormat) or `"conversations"`/`"mapping"` (ChatGPT)
  3. Directory structure: subdirectories containing JSON files -> Slack; default -> Markdown
- `parse(path, format)` — Resolves `Auto` format via `detect_format`, then dispatches to the appropriate parser module.

### claude_code.rs

Parser for Claude Code session transcripts (JSON files from `~/.claude/`).

**Key function:**

- `parse(path)` — Handles both single files and directories (scans for `.json` files). Internally calls `parse_file`.
- `parse_file(path)` — Reads JSON. Tries to find a message array (either the root array or a `"messages"` key). Concatenates messages as `"[{role}]: {text}\n\n"`. Summary is the first line (truncated to 200 chars). If the JSON doesn't have a recognizable message structure, the entire content is stored as a single memory.

### copilot.rs

Parser for Copilot CLI session logs (`.log` or `.json` files).

**Key function:**

- `parse(path)` — Handles directories (scans for `.log` and `.json` files) and single files.
- `parse_file(path)` — First tries JSON parsing with the same message-array structure as Claude Code. If JSON parsing fails or the file isn't JSON, treats the raw text content as a single memory. Summary is the first line (truncated to 200 chars).

### chatgpt.rs

Parser for ChatGPT export JSON (the `conversations.json` format from OpenAI's data export).

**Key function:**

- `parse(path)` — Expects a JSON array of conversation objects. For each conversation, extracts:
  - `title` from the conversation object
  - `create_time` (Unix timestamp, converted to RFC 3339)
  - Messages from the nested `mapping` structure (each node has `message.author.role` and `message.content.parts`)
  - Concatenates messages as `"[{role}]: {text}\n\n"`
  - Summary formatted as `"ChatGPT: {title}"`
  - Metadata includes the original title

### slack.rs

Parser for Slack workspace exports (directory structure with channels as subdirectories).

**Key function:**

- `parse(path)` — Requires a directory. Iterates subdirectories (each representing a channel). Calls `parse_channel` for each.
- `parse_channel(dir, channel)` — Reads all `.json` files in the channel directory. Each file is a JSON array of message objects with `user`, `text`, and `ts` fields. Messages are concatenated as `"[{user}]: {text}\n"`. The first message's `ts` is parsed as a Unix timestamp for the date. Summary is `"Slack #{channel}"`. Metadata includes the channel name.

### markdown.rs

Parser for generic markdown and text files (`.md`, `.txt`).

**Key function:**

- `parse(path)` — Handles directories (scans for `.md` and `.txt` files) and single files.
- `parse_file(path)` — Splits content by headings (`# ` or `## ` at line start) into sections. Each section with more than 10 characters of content becomes a separate `RawMemory`. Summary is the first non-empty line with heading markers stripped (truncated to 200 chars).
- `split_by_headings(content)` — Internal function. Accumulates lines into sections, splitting whenever a `# ` or `## ` heading is encountered (and the current section is non-empty).

### clear_format.rs

Parser for Clear Memory's native `.clear` format (JSON with a defined schema).

**Key internal types:**

- `ClearFile` — Deserialization target: `clear_format_version: String`, `memories: Vec<ClearMemory>`.
- `ClearMemory` — Per-memory record: `date: Option<String>`, `author: Option<String>`, `content: String`, `tags: ClearTags`, `metadata: serde_json::Value`.
- `ClearTags` — Optional tag fields: `team`, `repo`, `project`, `domain` (all `Option<String>`). Implements `Default`.

**Key function:**

- `parse(path)` — Reads and deserializes the `.clear` JSON file. Filters out memories with empty content. Maps `ClearTags` fields to `(tag_type, tag_value)` pairs. Summary is the first line of content (truncated to 200 chars).

### converter.rs

CSV-to-Clear-Format converter for enterprise workflows.

**Key internal types:**

- `ColMap` — Column index mapping: `date: Option<usize>`, `author: Option<usize>`, `content: Option<usize>`.

**Key functions:**

- `csv_to_clear(input, mapping)` — Reads a CSV file, maps columns to Clear Format fields, and produces a `.clear` JSON string. Uses the `csv` crate for parsing.
- `auto_map(headers)` — Automatic column mapping: looks for headers containing "date"/"time", "author"/"user"/"name", "content"/"text"/"note"/"message". Falls back to the last column for content if no match is found.
- `parse_mapping(mapping, headers)` — Parses explicit mapping strings like `"date=Column A,author=Column B,content=Column D"`. Returns error if a referenced column is not found in headers.

## Key Public Types Other Modules Depend On

- `RawMemory` — The universal import output type. Consumed by `engine::Engine` to feed into the retain pipeline.
- `ImportFormat` — Used by the CLI and engine to specify or auto-detect import format.
- `detect_format(path)` — Used by the engine when `--format auto` is specified.
- `parse(path, format)` — The main entry point called by the engine's import operation.

## Relevant config.toml Keys

- `[security] max_import_size_mb` — Maximum import file size (default: `500`)
- `[security] max_memory_size_mb` — Maximum size per individual memory (default: `10`)

## Deferred / Planned Functionality

- **Excel-to-Clear conversion:** The CLAUDE.md spec mentions `clearmemory convert excel-to-clear` for `.xlsx` files. The `converter.rs` module only implements CSV conversion. Excel support would require an additional dependency (e.g., `calamine` crate).
- **Clear Format validation command:** `clearmemory validate myfile.clear` is specified but not implemented as a standalone operation. The `clear_format::parse` function does JSON schema validation implicitly through deserialization, but there is no dedicated validation-with-error-reporting function.
- **Auto-tagging on import:** The spec mentions `--auto-tag` on import to automatically assign tags based on content analysis. This is not yet implemented.
- **Import size limits:** The `max_import_size_mb` and `max_memory_size_mb` config values are defined but not enforced in the import parsers.
- **Bulk import through engine:** The parsers produce `Vec<RawMemory>`, but the engine's bulk import path (storing all memories with embeddings, entity resolution, etc.) is not yet a distinct optimized flow -- each memory would go through the individual `retain` path.
