use rusqlite::Connection;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn connect(&self) -> anyhow::Result<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -20000;
            ",
        )?;
        Ok(conn)
    }

    pub fn init(&self) -> anyhow::Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS roots (
              id INTEGER PRIMARY KEY,
              path TEXT NOT NULL UNIQUE,
              enabled INTEGER NOT NULL DEFAULT 1,
              created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS packages (
              id INTEGER PRIMARY KEY,
              root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
              path TEXT NOT NULL,
              mtime INTEGER,
              total_size INTEGER NOT NULL DEFAULT 0,
              file_count INTEGER NOT NULL DEFAULT 0,
              chart_count INTEGER NOT NULL DEFAULT 0,
              last_scanned_at TEXT,
              UNIQUE(root_id, path)
            );

            CREATE TABLE IF NOT EXISTS charts (
              id INTEGER PRIMARY KEY,
              package_id INTEGER NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
              rel_path TEXT NOT NULL,
              ext TEXT NOT NULL,
              file_size INTEGER,
              mtime INTEGER,
              title TEXT,
              subtitle TEXT,
              artist TEXT,
              subartist TEXT,
              genre TEXT,
              playlevel TEXT,
              bpm TEXT,
              total TEXT,
              player TEXT,
              wav_count INTEGER,
              bmp_count INTEGER,
              wav_list_json TEXT,
              bmp_list_json TEXT,
              file_md5 TEXT,
              bms_norm_hash TEXT,
              object_stats_json TEXT,
              UNIQUE(package_id, rel_path)
            );
            CREATE INDEX IF NOT EXISTS idx_charts_package_rel ON charts(package_id, rel_path);
            CREATE INDEX IF NOT EXISTS idx_charts_file_md5 ON charts(file_md5);

            CREATE TABLE IF NOT EXISTS songs (
              id INTEGER PRIMARY KEY,
              canonical_title TEXT,
              canonical_artist TEXT
            );

            CREATE TABLE IF NOT EXISTS song_links (
              song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
              chart_id INTEGER NOT NULL REFERENCES charts(id) ON DELETE CASCADE,
              confidence REAL,
              user_confirmed INTEGER NOT NULL DEFAULT 0,
              PRIMARY KEY(song_id, chart_id)
            );

            CREATE TABLE IF NOT EXISTS table_sources (
              id INTEGER PRIMARY KEY,
              input_url TEXT NOT NULL UNIQUE,
              enabled INTEGER NOT NULL DEFAULT 1,
              last_fetch_at TEXT,
              last_success_at TEXT,
              last_error TEXT
            );

            CREATE TABLE IF NOT EXISTS tables (
              id INTEGER PRIMARY KEY,
              source_id INTEGER NOT NULL UNIQUE REFERENCES table_sources(id) ON DELETE CASCADE,
              page_url_resolved TEXT,
              header_url TEXT,
              data_url TEXT,
              data_final_url TEXT,
              name TEXT,
              symbol TEXT,
              tag TEXT,
              mode TEXT,
              level_order_json TEXT,
              attr_json TEXT,
              header_raw TEXT,
              data_raw TEXT,
              header_hash TEXT,
              data_hash TEXT,
              updated_at TEXT
            );

            CREATE TABLE IF NOT EXISTS table_entries (
              id INTEGER PRIMARY KEY,
              table_id INTEGER NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
              md5 TEXT NOT NULL,
              sha256 TEXT,
              level_text TEXT,
              title TEXT,
              artist TEXT,
              charter TEXT,
              url TEXT,
              url_diff TEXT,
              comment TEXT,
              raw_json TEXT NOT NULL,
              UNIQUE(table_id, md5)
            );

            CREATE TABLE IF NOT EXISTS table_groups (
              id INTEGER PRIMARY KEY,
              table_id INTEGER NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
              group_type TEXT NOT NULL,
              group_set_index INTEGER NOT NULL DEFAULT 0,
              name TEXT,
              style TEXT,
              constraints_json TEXT,
              trophies_json TEXT,
              raw_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS table_group_items (
              group_id INTEGER NOT NULL REFERENCES table_groups(id) ON DELETE CASCADE,
              md5 TEXT NOT NULL,
              title_hint TEXT,
              PRIMARY KEY(group_id, md5)
            );
            CREATE INDEX IF NOT EXISTS idx_table_group_items ON table_group_items(group_id, md5);

            CREATE VIRTUAL TABLE IF NOT EXISTS charts_fts USING fts5(
              title,
              artist,
              path,
              content=''
            );
            ",
        )?;
        Ok(())
    }
}
