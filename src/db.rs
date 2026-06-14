//! Histórico e favoritos persistentes em SQLite (via `rusqlite`).
//!
//! Uma única conexão por thread (a UI é single-thread). É leve: o arquivo fica
//! em `~/.local/share/tucano/tucano.db`.

use std::cell::RefCell;

use gtk::glib;
use rusqlite::{params, Connection};

thread_local! {
    static DB: RefCell<Option<Connection>> = RefCell::new(open());
}

fn open() -> Option<Connection> {
    let dir = glib::user_data_dir().join("tucano");
    let _ = std::fs::create_dir_all(&dir);
    let conn = Connection::open(dir.join("tucano.db")).ok()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history(
            id         INTEGER PRIMARY KEY,
            url        TEXT NOT NULL,
            title      TEXT,
            visited_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_history_visited ON history(visited_at DESC);
        CREATE TABLE IF NOT EXISTS bookmarks(
            id         INTEGER PRIMARY KEY,
            url        TEXT NOT NULL UNIQUE,
            title      TEXT,
            created_at INTEGER NOT NULL
        );",
    )
    .ok()?;
    Some(conn)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Registra uma visita no histórico (ignora URLs vazias e páginas internas).
pub fn record_visit(url: &str, title: &str) {
    if url.is_empty() || url.starts_with("about:") {
        return;
    }
    DB.with(|d| {
        if let Some(c) = d.borrow().as_ref() {
            let _ = c.execute(
                "INSERT INTO history(url, title, visited_at) VALUES(?1, ?2, ?3)",
                params![url, title, now()],
            );
        }
    });
}

/// `true` se a URL já está nos favoritos.
pub fn is_bookmarked(url: &str) -> bool {
    DB.with(|d| {
        d.borrow().as_ref().is_some_and(|c| {
            c.query_row("SELECT 1 FROM bookmarks WHERE url = ?1", [url], |_| Ok(()))
                .is_ok()
        })
    })
}

/// Adiciona/remove a URL dos favoritos. Retorna o novo estado (`true` = favorito).
pub fn toggle_bookmark(url: &str, title: &str) -> bool {
    DB.with(|d| {
        let borrow = d.borrow();
        let Some(c) = borrow.as_ref() else {
            return false;
        };
        if is_bookmarked(url) {
            let _ = c.execute("DELETE FROM bookmarks WHERE url = ?1", [url]);
            false
        } else {
            let _ = c.execute(
                "INSERT OR IGNORE INTO bookmarks(url, title, created_at) VALUES(?1, ?2, ?3)",
                params![url, title, now()],
            );
            true
        }
    })
}

/// Favoritos, mais recentes primeiro: `(url, título)`.
pub fn bookmarks() -> Vec<(String, String)> {
    query("SELECT url, COALESCE(NULLIF(title,''), url) FROM bookmarks ORDER BY created_at DESC", 200)
}

/// Histórico recente (sem repetir URL), mais recentes primeiro: `(url, título)`.
pub fn recent_history(limit: i64) -> Vec<(String, String)> {
    query(
        "SELECT url, COALESCE(NULLIF(title,''), url) FROM history
         GROUP BY url ORDER BY MAX(visited_at) DESC",
        limit,
    )
}

fn query(sql_without_limit: &str, limit: i64) -> Vec<(String, String)> {
    DB.with(|d| {
        let borrow = d.borrow();
        let Some(c) = borrow.as_ref() else {
            return Vec::new();
        };
        let sql = format!("{sql_without_limit} LIMIT {limit}");
        let Ok(mut stmt) = c.prepare(&sql) else {
            return Vec::new();
        };
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
        match rows {
            Ok(iter) => iter.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    })
}
