//! Localizar na página (Ctrl+F) usando o `FindController` do WebKit.

use adw::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;

use crate::browser::current_webview;

/// `FindController` da aba atual, se houver um WebView.
fn controller(tab_view: &adw::TabView) -> Option<webkit::FindController> {
    current_webview(tab_view).and_then(|wv| wv.find_controller())
}

/// Constrói a barra de busca dentro do `search_bar` e registra o atalho Ctrl+F.
pub fn wire(
    search_bar: &gtk::SearchBar,
    tab_view: &adw::TabView,
    app: &adw::Application,
    window: &adw::ApplicationWindow,
) {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("Localizar na página"));

    let prev = gtk::Button::from_icon_name("go-up-symbolic");
    prev.set_tooltip_text(Some("Anterior"));
    let next = gtk::Button::from_icon_name("go-down-symbolic");
    next.set_tooltip_text(Some("Próximo"));

    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bx.append(&entry);
    bx.append(&prev);
    bx.append(&next);
    search_bar.set_child(Some(&bx));
    search_bar.connect_entry(&entry);

    let opts = (webkit::FindOptions::CASE_INSENSITIVE | webkit::FindOptions::WRAP_AROUND).bits();

    // Busca conforme digita.
    entry.connect_search_changed(glib::clone!(
        #[weak] tab_view,
        move |e| if let Some(fc) = controller(&tab_view) {
            let text = e.text();
            if text.is_empty() {
                fc.search_finish();
            } else {
                fc.search(&text, opts, u32::MAX);
            }
        }
    ));
    entry.connect_activate(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(fc) = controller(&tab_view) { fc.search_next(); }
    ));
    next.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(fc) = controller(&tab_view) { fc.search_next(); }
    ));
    prev.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(fc) = controller(&tab_view) { fc.search_previous(); }
    ));

    // Ao fechar a barra, limpa os destaques.
    search_bar.connect_search_mode_enabled_notify(glib::clone!(
        #[weak] tab_view,
        move |sb| if !sb.is_search_mode() {
            if let Some(fc) = controller(&tab_view) {
                fc.search_finish();
            }
        }
    ));

    // Ctrl+F abre a barra e foca o campo.
    let act = gio::SimpleAction::new("find", None);
    act.connect_activate(glib::clone!(
        #[weak] search_bar, #[weak] entry,
        move |_, _| {
            search_bar.set_search_mode(true);
            entry.grab_focus();
        }
    ));
    window.add_action(&act);
    app.set_accels_for_action("win.find", &["<Ctrl>f"]);
}
