use adw::prelude::*;
use gtk::glib;
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

    // --- Barra de endereço ---
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

    let tab_bar = adw::TabBar::builder().view(&tab_view).build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&header);
    layout.append(&tab_bar);
    layout.append(&tab_view);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(750)
        .title("Tucano")
        .content(&layout)
        .build();

    // --- Ações dos botões (operam sobre a aba atual) ---
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
        move |_| {
            add_tab(&tab_view, &url_entry, &back_btn, &forward_btn, HOME);
            url_entry.grab_focus();
        }
    ));

    // Enter na barra de endereço: navega na aba atual (ou abre uma nova).
    url_entry.connect_activate(glib::clone!(
        #[weak] tab_view, #[weak] back_btn, #[weak] forward_btn,
        move |entry| {
            let uri = normalize_url(&entry.text());
            match current_webview(&tab_view) {
                Some(wv) => wv.load_uri(&uri),
                None => add_tab(&tab_view, entry, &back_btn, &forward_btn, &uri),
            }
        }
    ));

    // Trocar de aba atualiza a barra de endereço e a navegação.
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

    add_tab(&tab_view, &url_entry, &back_btn, &forward_btn, HOME);
    window.present();
}

/// Cria uma aba nova com um WebView e conecta os sinais de atualização.
fn add_tab(
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
    uri: &str,
) {
    let webview = WebView::new();
    webview.set_vexpand(true);
    webview.load_uri(uri);

    let page = tab_view.append(&webview);
    page.set_title("Nova aba");
    page.set_loading(true);

    // Título da aba acompanha o título da página.
    webview.connect_title_notify(glib::clone!(
        #[weak] page,
        move |wv| {
            let title = wv.title().unwrap_or_default();
            page.set_title(if title.is_empty() { "Nova aba" } else { title.as_str() });
        }
    ));

    // URL exibida acompanha a navegação (apenas se for a aba ativa).
    webview.connect_uri_notify(glib::clone!(
        #[weak] tab_view, #[weak] url_entry,
        move |wv| if is_current(&tab_view, wv) {
            url_entry.set_text(&wv.uri().unwrap_or_default());
        }
    ));

    // Progresso de carregamento mostrado dentro da barra de endereço.
    webview.connect_estimated_load_progress_notify(glib::clone!(
        #[weak] tab_view, #[weak] url_entry,
        move |wv| if is_current(&tab_view, wv) {
            let p = wv.estimated_load_progress();
            url_entry.set_progress_fraction(if p >= 1.0 { 0.0 } else { p });
        }
    ));

    // Estados de carregamento: spinner da aba + sensibilidade dos botões.
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

    tab_view.set_selected_page(&page);
}

/// WebView da aba selecionada, se houver.
fn current_webview(tab_view: &adw::TabView) -> Option<WebView> {
    tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<WebView>().ok())
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
