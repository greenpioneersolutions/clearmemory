# backup/ -- Snapshot Backup and Restore

## Role in Architecture

The backup module handles creating, restoring, verifying, and scheduling backups of the entire Clear Memory data directory (`~/.clearmemory/`). The intended production format is `.cmb` files -- compressed tar archives containing the SQLite database (captured via SQLite Online Backup API), LanceDB vector snapshot, verbatim transcript files, config, and a checksums manifest. Backups are the primary disaster recovery mechanism and are also created automatically before destructive operations like purge and schema migrations.

Currently, this module contains **placeholder implementations** that write/read JSON metadata files. The full `.cmb` archive creation, SQLite Online Backup API integration, LanceDB snapshot capture, verbatim file hardlinking, and AES-256-GCM backup encryption are planned but not yet wired up. The scheduling and cleanup logic, however, is fully functional.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports: `restore`, `scheduler`, `snapshot`.

### snapshot.rs

Backup creation. Contains:

- **`BackupMetadata`** (private) -- A serializable struct with fields: `version` (&str, currently "0.1.0"), `created_at` (ISO 8601 timestamp), `db_path` (string path to SQLite file), `verbatim_dir` (string path to verbatim directory). This is the metadata that will eventually be embedded inside `.cmb` archives.

- **`create_backup(db_path, verbatim_dir, output_path) -> Result<()>`** -- Currently writes a JSON metadata file to `output_path`. Creates parent directories if they do not exist. The full implementation will: (1) use the SQLite Online Backup API for a consistent snapshot under concurrent access, (2) copy the LanceDB version snapshot (immutable append-only files), (3) hardlink verbatim files where supported (falling back to copy), (4) compress everything into a `.cmb` tar archive, and (5) optionally encrypt with AES-256-GCM.

### restore.rs

Backup restoration and verification. Contains:

- **`restore_backup(backup_path, target_dir) -> Result<()>`** -- Currently a placeholder that verifies the backup file exists, creates the target directory, and copies the backup file as `backup_metadata.json`. The full implementation will extract the `.cmb` archive, restore SQLite via Online Backup API, copy LanceDB snapshot and verbatim files, validate all checksums, and rebuild derived indexes.

- **`verify_backup(backup_path) -> Result<bool>`** -- Returns `true` if the file exists and is valid JSON. The full implementation will validate SHA-256 checksums for every file in the archive against the embedded checksums manifest.

### scheduler.rs

Scheduled backup management. This file is fully implemented. Contains:

- **`BackupSchedule`** -- Configuration struct with fields: `interval_hours` (u64), `backup_dir` (PathBuf), `retention_count` (u32, max number of `.cmb` files to keep), `encrypt` (bool), `last_backup` (Option<String>, ISO 8601 timestamp of the last completed backup).

- **`should_run_backup(schedule) -> bool`** -- Returns `true` if no backup has ever run, or if the elapsed time since `last_backup` exceeds `interval_hours`. Handles unparseable timestamps by returning `true` (fail-safe: when in doubt, back up).

- **`cleanup_old_backups(backup_dir, retention_count) -> Result<usize, io::Error>`** -- Lists all `.cmb` files in the backup directory, sorts by modification time (newest first), and deletes files beyond the retention count. Returns the number of files deleted. Ignores non-`.cmb` files. Returns 0 if the directory does not exist.

## Key Public Types Other Modules Depend On

- `create_backup` -- called by the CLI `backup` command and automatically before purge/migration operations
- `restore_backup` / `verify_backup` -- called by the CLI `restore` command
- `BackupSchedule` / `should_run_backup` -- used by the server's background task loop in `serve` mode
- `cleanup_old_backups` -- called after each successful backup to enforce retention

## Relevant config.toml Keys

```toml
[backup]
auto_backup_enabled = false          # enable scheduled backups in serve mode
auto_backup_interval_hours = 24      # hours between automatic backups
backup_directory = "~/.clearmemory/backups"
backup_retention_count = 7           # max .cmb files to keep
encrypt_backups = true               # AES-256-GCM encryption of .cmb files

[migrations]
backup_before_migrate = true         # auto-backup before applying schema migrations
```

## Deferred / Planned Functionality

- **SQLite Online Backup API integration**: Use `sqlite3_backup_*` via rusqlite for consistent snapshots under concurrent access
- **LanceDB snapshot capture**: Copy the immutable columnar files from the LanceDB directory
- **Verbatim file hardlinking**: Use filesystem hardlinks (instant, zero disk overhead) on supported platforms, with copy fallback
- **Compressed .cmb archive format**: Package all components into a single compressed tar file with a checksums manifest
- **Backup encryption**: AES-256-GCM encryption using a key derived from the master passphrase via Argon2id
- **Encrypted backup restore**: Prompt for passphrase (or read from `CLEARMEMORY_PASSPHRASE` env var) when restoring encrypted backups
- **Index rebuild after restore**: Automatically rebuild LanceDB index from restored SQLite and verbatim files
