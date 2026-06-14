use adw::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;
use webkit::WebView;

/// Página inicial / motor de busca padrão.
const HOME: &str = "https://duckduckgo.com";

/// Monta a janela principal e a primeira aba.
pub fn build_window(app: &adw::Application) {
    let tab_view = adw::TabView::new();
    tab_view.set_vexpand(true);

    // --- Botões de navegação ---
    let back_btn = gtk::Button::from_icon_name("go-previous-symbolic");
    back_btn.set_sensitive(false);
    let forward_btn = gtk::Button::from_icon_name("go-next-symbolic");
    forward_btn.set_sensitive(false);
    let reload_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    let new_tab_btn = gtk::Button::from_icon_name("tab-new-symbolic");

    let nav_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.append(&back_btn);
    nav_box.append(&forward_btn);

    // --- Barra de endereço (topo) ---
    let url_entry = gtk::Entry::builder()
        .hexpand(true)
        .placeholder_text("Pesquise ou digite um endereço")
        .input_purpose(gtk::InputPurpose::Url)
        .build();

    // --- Cabeçalho estilo GNOME ---
    let header = adw::HeaderBar::new();
    header.pack_start(&nav_box);
    header.pack_start(&reload_btn);
    header.set_title_widget(Some(&url_entry));
    header.pack_end(&new_tab_btn);

    // Abas ficam ocultas no topo; o gerenciamento é feito pela roda flutuante.
    let content = gtk::Overlay::new();
    content.set_vexpand(true);
    content.set_child(Some(&tab_view));
    crate::wheel::attach(&content, &tab_view);

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&header);
    layout.append(&content);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(750)
        .title("Tucano")
        .content(&layout)
        .build();

    // --- Botões operam sobre a aba atual ---
    back_btn.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(wv) = current_webview(&tab_view) { wv.go_back(); }
    ));
    forward_btn.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(wv) = current_webview(&tab_view) { wv.go_forward(); }
    ));
    reload_btn.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| if let Some(wv) = current_webview(&tab_view) { wv.reload(); }
    ));
    new_tab_btn.connect_clicked(glib::clone!(
        #[weak] tab_view, #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn,
        move |_| add_tab(&tab_view, &url_entry, &back_btn, &forward_btn)
    ));

    // Enter na barra do topo: navega na aba atual (ou inicia a aba em branco).
    url_entry.connect_activate(glib::clone!(
        #[weak] tab_view, #[weak] back_btn, #[weak] forward_btn,
        move |entry| {
            let uri = normalize_url(&entry.text());
            match current_webview(&tab_view) {
                Some(wv) => wv.load_uri(&uri),
                None => if let Some(c) = current_container(&tab_view) {
                    open_in_container(&c, &uri, &tab_view, entry, &back_btn, &forward_btn);
                }
            }
        }
    ));

    // Trocar de aba sincroniza barra de endereço e botões.
    tab_view.connect_selected_page_notify(glib::clone!(
        #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn,
        move |tv| sync_chrome(tv, &url_entry, &back_btn, &forward_btn)
    ));
    // Fechar a última aba fecha a janela.
    tab_view.connect_close_page(glib::clone!(
        #[weak] window,
        #[upgrade_or] glib::Propagation::Proceed,
        move |tv, _| {
            if tv.n_pages() <= 1 { window.close(); }
            glib::Propagation::Proceed
        }
    ));

    register_shortcuts(app, &window, &tab_view, &url_entry, &back_btn, &forward_btn);

    add_tab(&tab_view, &url_entry, &back_btn, &forward_btn);
    window.present();
}

/// Registra ações com atalhos de teclado (estilo GNOME, via GAction + accels).
fn register_shortcuts(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    let add = |name: &str, accels: &[&str], action: gio::SimpleAction| {
        window.add_action(&action);
        app.set_accels_for_action(&format!("win.{name}"), accels);
        action
    };

    let act_new = gio::SimpleAction::new("new-tab", None);
    act_new.connect_activate(glib::clone!(
        #[weak] tab_view, #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn,
        move |_, _| add_tab(&tab_view, &url_entry, &back_btn, &forward_btn)
    ));
    add("new-tab", &["<Ctrl>t"], act_new);

    let act_close = gio::SimpleAction::new("close-tab", None);
    act_close.connect_activate(glib::clone!(
        #[weak] tab_view,
        move |_, _| if let Some(p) = tab_view.selected_page() { tab_view.close_page(&p); }
    ));
    add("close-tab", &["<Ctrl>w"], act_close);

    let act_focus = gio::SimpleAction::new("focus-url", None);
    act_focus.connect_activate(glib::clone!(
        #[weak] url_entry,
        move |_, _| { url_entry.grab_focus(); url_entry.select_region(0, -1); }
    ));
    add("focus-url", &["<Ctrl>l"], act_focus);

    let act_reload = gio::SimpleAction::new("reload", None);
    act_reload.connect_activate(glib::clone!(
        #[weak] tab_view,
        move |_, _| if let Some(wv) = current_webview(&tab_view) { wv.reload(); }
    ));
    add("reload", &["<Ctrl>r", "F5"], act_reload);

    let act_back = gio::SimpleAction::new("back", None);
    act_back.connect_activate(glib::clone!(
        #[weak] tab_view,
        move |_, _| if let Some(wv) = current_webview(&tab_view) { wv.go_back(); }
    ));
    add("back", &["<Alt>Left"], act_back);

    let act_forward = gio::SimpleAction::new("forward", None);
    act_forward.connect_activate(glib::clone!(
        #[weak] tab_view,
        move |_, _| if let Some(wv) = current_webview(&tab_view) { wv.go_forward(); }
    ));
    add("forward", &["<Alt>Right"], act_forward);
}

/// Cria uma aba nova mostrando a página de início (busca centralizada, estilo Arc).
/// O `WebView` só é criado quando o usuário escolhe um destino.
fn add_tab(
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    // Container que alterna entre a página de início e o WebView.
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let title = gtk::Label::new(Some("Tucano"));
    title.add_css_class("title-1");

    let search = gtk::Entry::builder()
        .placeholder_text("Pesquise ou digite uma URL")
        .primary_icon_name("system-search-symbolic")
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    search.set_size_request(520, 44);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 18);
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.set_hexpand(true);
    card.set_vexpand(true);
    card.append(&title);
    card.append(&search);
    container.append(&card);

    let page = tab_view.append(&container);
    page.set_title("Nova aba");

    // Enter na busca da página de início abre o destino na própria aba.
    search.connect_activate(glib::clone!(
        #[weak] tab_view, #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn, #[weak] container,
        move |e| open_in_container(
            &container, &normalize_url(&e.text()), &tab_view, &url_entry, &back_btn, &forward_btn,
        )
    ));

    tab_view.set_selected_page(&page);
    url_entry.set_text("");
    search.grab_focus();
}

/// Substitui a página de início pelo WebView e começa a carregar `uri`.
fn open_in_container(
    container: &gtk::Box,
    uri: &str,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let webview = WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    let page = tab_view.page(container);
    page.set_loading(true);
    wire_webview(&webview, &page, tab_view, url_entry, back_btn, forward_btn);

    webview.load_uri(uri);
    container.append(&webview);

    url_entry.set_text(uri);
    webview.grab_focus();
}

/// Conecta os sinais do WebView que atualizam aba e cabeçalho.
fn wire_webview(
    webview: &WebView,
    page: &adw::TabPage,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    webview.connect_title_notify(glib::clone!(
        #[weak] page,
        move |wv| {
            let title = wv.title().unwrap_or_default();
            page.set_title(if title.is_empty() { "Nova aba" } else { title.as_str() });
        }
    ));

    webview.connect_uri_notify(glib::clone!(
        #[weak] tab_view, #[weak] url_entry,
        move |wv| if is_current(&tab_view, wv) {
            url_entry.set_text(&wv.uri().unwrap_or_default());
        }
    ));

    webview.connect_estimated_load_progress_notify(glib::clone!(
        #[weak] tab_view, #[weak] url_entry,
        move |wv| if is_current(&tab_view, wv) {
            let p = wv.estimated_load_progress();
            url_entry.set_progress_fraction(if p >= 1.0 { 0.0 } else { p });
        }
    ));

    webview.connect_load_changed(glib::clone!(
        #[weak] tab_view, #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn, #[weak] page,
        move |wv, event| {
            page.set_loading(event != webkit::LoadEvent::Finished);
            if is_current(&tab_view, wv) {
                if event == webkit::LoadEvent::Finished {
                    url_entry.set_progress_fraction(0.0);
                }
                back_btn.set_sensitive(wv.can_go_back());
                forward_btn.set_sensitive(wv.can_go_forward());
            }
        }
    ));
}

/// Container (página) da aba selecionada.
fn current_container(tab_view: &adw::TabView) -> Option<gtk::Box> {
    tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<gtk::Box>().ok())
}

/// WebView da aba selecionada — `None` se a aba ainda está na página de início.
fn current_webview(tab_view: &adw::TabView) -> Option<WebView> {
    current_container(tab_view)
        .and_then(|c| c.first_child())
        .and_then(|child| child.downcast::<WebView>().ok())
}

/// `true` se `wv` é o WebView da aba ativa.
fn is_current(tab_view: &adw::TabView, wv: &WebView) -> bool {
    current_webview(tab_view).as_ref() == Some(wv)
}

/// Sincroniza barra de endereço e botões com a aba ativa.
fn sync_chrome(
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    match current_webview(tab_view) {
        Some(wv) => {
            url_entry.set_text(&wv.uri().unwrap_or_default());
            back_btn.set_sensitive(wv.can_go_back());
            forward_btn.set_sensitive(wv.can_go_forward());
        }
        None => {
            url_entry.set_text("");
            back_btn.set_sensitive(false);
            forward_btn.set_sensitive(false);
        }
    }
}

/// Transforma o texto digitado em URL: respeita esquemas, completa domínios
/// e, caso contrário, pesquisa no buscador padrão.
fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return HOME.to_string();
    }
    if s.contains("://") || s.starts_with("about:") {
        return s.to_string();
    }
    if !s.contains(' ') && s.contains('.') {
        return format!("https://{s}");
    }
    let q = glib::Uri::escape_string(s, None, false);
    format!("https://duckduckgo.com/?q={q}")
}
