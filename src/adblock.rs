//! Bloqueador de conteúdo nativo do WebKit.
//!
//! Compila uma lista de regras (formato content-blocker do Safari) para
//! bytecode eficiente via `UserContentFilterStore` e a aplica a um
//! `UserContentManager` único, compartilhado por todas as abas. Bloquear no
//! nível do engine reduz CPU, rede e memória — além de acelerar as páginas.

use gtk::{gio, glib};

thread_local! {
    static MANAGER: webkit::UserContentManager = build();
}

/// `UserContentManager` compartilhado (com o filtro) para criar os WebViews.
pub fn content_manager() -> webkit::UserContentManager {
    MANAGER.with(|m| m.clone())
}

fn build() -> webkit::UserContentManager {
    let manager = webkit::UserContentManager::new();

    let dir = glib::user_cache_dir().join("tucano");
    let _ = std::fs::create_dir_all(&dir);
    let Some(dir_str) = dir.to_str() else {
        return manager;
    };

    // Compila a lista (assíncrono) e adiciona o filtro quando estiver pronto.
    let store = webkit::UserContentFilterStore::new(dir_str);
    let rules = glib::Bytes::from_static(include_bytes!("../assets/blocklist.json"));
    store.save(
        "tucano-blocklist",
        &rules,
        gio::Cancellable::NONE,
        glib::clone!(
            #[strong] manager,
            move |result| match result {
                Ok(filter) => manager.add_filter(&filter),
                Err(e) => eprintln!("[tucano] bloqueador desativado: {}", e.message()),
            }
        ),
    );

    manager
}
