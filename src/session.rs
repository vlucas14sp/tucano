//! Sessão de rede persistente: cookies, cache e armazenamento ficam em disco,
//! então logins e dados de sites sobrevivem entre execuções.
//!
//! Uma única `NetworkSession` é compartilhada por todas as abas (um "perfil").

use gtk::glib;

thread_local! {
    static SESSION: webkit::NetworkSession = build();
}

/// `NetworkSession` persistente compartilhada para criar os WebViews.
pub fn session() -> webkit::NetworkSession {
    SESSION.with(|s| s.clone())
}

fn build() -> webkit::NetworkSession {
    let data = glib::user_data_dir().join("tucano");
    let cache = glib::user_cache_dir().join("tucano");
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::create_dir_all(&cache);

    let session = webkit::NetworkSession::new(data.to_str(), cache.to_str());

    // Cookies persistentes em SQLite (sobrevivem ao fechar o navegador).
    if let Some(cookies) = session.cookie_manager() {
        if let Some(path) = data.join("cookies.sqlite").to_str() {
            cookies.set_persistent_storage(path, webkit::CookiePersistentStorage::Sqlite);
        }
    }

    session
}
