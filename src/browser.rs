use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;
use webkit::WebView;

/// Tempo (s) de inatividade até uma aba em segundo plano ser suspensa.
const DISCARD_SECS_DEFAULT: u64 = 300;

/// Estado de atividade por aba: último acesso e, se suspensa, a URL para restaurar.
type Activity = Rc<RefCell<HashMap<usize, (Instant, Option<String>)>>>;

thread_local! {
    /// Botão de favorito do cabeçalho, para refletir o estado da página atual.
    static STAR: RefCell<Option<gtk::Button>> = const { RefCell::new(None) };
}

/// Página inicial / motor de busca padrão.
const HOME: &str = "https://www.google.com";

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

    let star_btn = gtk::Button::from_icon_name("non-starred-symbolic");
    star_btn.set_tooltip_text(Some("Adicionar aos favoritos"));
    star_btn.set_sensitive(false);
    let library_btn = gtk::MenuButton::new();
    library_btn.set_icon_name("view-list-symbolic");
    library_btn.set_tooltip_text(Some("Favoritos e histórico"));

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
    header.pack_end(&library_btn);
    header.pack_end(&star_btn);

    // Abas ficam ocultas no topo; o gerenciamento é feito pela roda flutuante.
    let content = gtk::Overlay::new();
    content.set_vexpand(true);
    content.set_child(Some(&tab_view));

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
            navigate(&tab_view, entry, &back_btn, &forward_btn, &uri);
        }
    ));

    // Disponibiliza o botão de favorito para os demais pontos da UI.
    STAR.with(|cell| *cell.borrow_mut() = Some(star_btn.clone()));

    // Estrela: adiciona/remove a página atual dos favoritos.
    star_btn.connect_clicked(glib::clone!(
        #[weak] tab_view,
        move |_| {
            if let Some(wv) = current_webview(&tab_view) {
                let url = wv.uri().unwrap_or_default();
                if !url.is_empty() {
                    let title = wv.title().unwrap_or_default();
                    crate::db::toggle_bookmark(&url, &title);
                    update_star(&tab_view);
                }
            }
        }
    ));

    // Menu de favoritos e histórico.
    build_library(&library_btn, &tab_view, &url_entry, &back_btn, &forward_btn);

    // Estado de atividade das abas (para suspender as inativas).
    let activity: Activity = Rc::new(RefCell::new(HashMap::new()));

    // Trocar de aba sincroniza a barra e restaura a aba se ela estava suspensa.
    tab_view.connect_selected_page_notify(glib::clone!(
        #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn, #[strong] activity,
        move |tv| on_select(tv, &url_entry, &back_btn, &forward_btn, &activity)
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
    // A roda de abas e seus atalhos (Ctrl+E, Ctrl+Tab) precisam da janela pronta.
    crate::wheel::attach(&content, &tab_view, app, &window);

    // Verifica periodicamente e suspende abas em segundo plano paradas.
    glib::timeout_add_seconds_local(
        30,
        glib::clone!(
            #[weak] tab_view, #[strong] activity,
            #[upgrade_or] glib::ControlFlow::Break,
            move || {
                discard_inactive(&tab_view, &activity);
                glib::ControlFlow::Continue
            }
        ),
    );

    add_tab(&tab_view, &url_entry, &back_btn, &forward_btn);

    // Para depuração: TUCANO_URL=<url[,url2,...]> abre direto essas páginas.
    if let Ok(urls) = std::env::var("TUCANO_URL") {
        let mut parts = urls.split(',').map(str::trim).filter(|s| !s.is_empty());
        if let Some(first) = parts.next() {
            if let Some(c) = current_container(&tab_view) {
                open_in_container(&c, first, &tab_view, &url_entry, &back_btn, &forward_btn);
            }
            for u in parts {
                add_tab(&tab_view, &url_entry, &back_btn, &forward_btn);
                if let Some(c) = current_container(&tab_view) {
                    open_in_container(&c, u, &tab_view, &url_entry, &back_btn, &forward_btn);
                }
            }
        }
    }

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

    // Compartilha o bloqueador e a sessão persistente (cookies/login) entre abas.
    let webview = WebView::builder()
        .user_content_manager(&crate::adblock::content_manager())
        .network_session(&crate::session::session())
        .build();
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    tune_settings(&webview);

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
            update_star(&tab_view);
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
            if event == webkit::LoadEvent::Finished {
                // Registra a visita no histórico.
                let url = wv.uri().unwrap_or_default();
                let title = wv.title().unwrap_or_default();
                crate::db::record_visit(&url, &title);
            }
            if is_current(&tab_view, wv) {
                if event == webkit::LoadEvent::Finished {
                    url_entry.set_progress_fraction(0.0);
                }
                back_btn.set_sensitive(wv.can_go_back());
                forward_btn.set_sensitive(wv.can_go_forward());
            }
        }
    ));

    // Reporta falhas reais de carregamento (ignora cancelamentos, que são
    // normais quando a própria página dispara um redirect/recarregamento).
    webview.connect_load_failed(|_, _event, uri, error| {
        if !error.matches(webkit::NetworkError::Cancelled) {
            eprintln!("[tucano] falha ao carregar {uri}: {}", error.message());
        }
        false // deixa o WebKit exibir sua página de erro padrão
    });
    webview.connect_web_process_terminated(|_, reason| {
        eprintln!("[tucano] processo web encerrado: {reason:?}");
    });
}

/// Ajusta as configurações do WebView (mídia/MSE, WebGL e User-Agent moderno).
fn tune_settings(webview: &WebView) {
    let Some(s) = webkit::prelude::WebViewExt::settings(webview) else {
        return;
    };
    s.set_enable_mediasource(true); // MSE — necessário para o player do YouTube
    s.set_enable_encrypted_media(true); // EME (DRM) — exige um CDM Widevine no WebKitGTK
    s.set_enable_media_capabilities(true); // API que os streamings usam p/ negociar codec/DRM
    s.set_enable_webgl(true);
    s.set_enable_smooth_scrolling(true);
    s.set_media_playback_requires_user_gesture(false);
    // User-Agent de desktop limpo (Safari), evita que alguns sites tratem o
    // navegador como desconhecido.
    s.set_user_agent(Some(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) \
         Version/17.0 Safari/605.1.15",
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

/// Chave estável de uma aba enquanto ela existe (ponteiro do objeto).
fn page_key(page: &adw::TabPage) -> usize {
    page.as_ptr() as usize
}

fn discard_secs() -> u64 {
    std::env::var("TUCANO_DISCARD_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DISCARD_SECS_DEFAULT)
}

/// Ao selecionar uma aba: marca como ativa, restaura se estava suspensa e
/// sincroniza a barra de endereço/botões.
fn on_select(
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
    activity: &Activity,
) {
    let Some(page) = tab_view.selected_page() else {
        sync_chrome(tab_view, url_entry, back_btn, forward_btn);
        return;
    };

    let discarded_url = {
        let mut act = activity.borrow_mut();
        let entry = act.entry(page_key(&page)).or_insert((Instant::now(), None));
        entry.0 = Instant::now();
        entry.1.take()
    };

    if let Some(url) = discarded_url {
        if let Ok(container) = page.child().downcast::<gtk::Box>() {
            // Recria o WebView e recarrega a página suspensa.
            open_in_container(&container, &url, tab_view, url_entry, back_btn, forward_btn);
        }
        update_star(tab_view);
        return;
    }

    sync_chrome(tab_view, url_entry, back_btn, forward_btn);
    update_star(tab_view);
}

/// Navega na aba atual; se ela ainda estiver na página de início, abre ali.
fn navigate(
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
    url: &str,
) {
    match current_webview(tab_view) {
        Some(wv) => wv.load_uri(url),
        None => {
            if let Some(c) = current_container(tab_view) {
                open_in_container(&c, url, tab_view, url_entry, back_btn, forward_btn);
            }
        }
    }
}

/// Atualiza o ícone da estrela conforme a página atual estar (ou não) favoritada.
fn update_star(tab_view: &adw::TabView) {
    STAR.with(|cell| {
        let borrow = cell.borrow();
        let Some(star) = borrow.as_ref() else {
            return;
        };
        let url = current_webview(tab_view)
            .and_then(|wv| wv.uri())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if url.is_empty() {
            star.set_sensitive(false);
            star.set_icon_name("non-starred-symbolic");
            return;
        }
        star.set_sensitive(true);
        if crate::db::is_bookmarked(&url) {
            star.set_icon_name("starred-symbolic");
            star.set_tooltip_text(Some("Remover dos favoritos"));
        } else {
            star.set_icon_name("non-starred-symbolic");
            star.set_tooltip_text(Some("Adicionar aos favoritos"));
        }
    });
}

/// Monta o menu (popover) de favoritos e histórico, recarregado ao abrir.
fn build_library(
    menu_btn: &gtk::MenuButton,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list.set_margin_top(6);
    list.set_margin_bottom(6);
    list.set_margin_start(6);
    list.set_margin_end(6);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(480)
        .propagate_natural_height(true)
        .child(&list)
        .build();

    let popover = gtk::Popover::new();
    popover.set_child(Some(&scroller));
    popover.set_size_request(360, -1);
    menu_btn.set_popover(Some(&popover));

    menu_btn.connect_active_notify(glib::clone!(
        #[weak] list, #[weak] popover, #[weak] tab_view,
        #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn,
        move |btn| if btn.is_active() {
            fill_library(&list, &popover, &tab_view, &url_entry, &back_btn, &forward_btn);
        }
    ));
}

/// Preenche o popover com seções de Favoritos e Histórico recente.
fn fill_library(
    list: &gtk::Box,
    popover: &gtk::Popover,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    add_section(list, "Favoritos", crate::db::bookmarks(), popover, tab_view, url_entry, back_btn, forward_btn);
    add_section(list, "Histórico recente", crate::db::recent_history(50), popover, tab_view, url_entry, back_btn, forward_btn);
}

#[allow(clippy::too_many_arguments)]
fn add_section(
    list: &gtk::Box,
    title: &str,
    items: Vec<(String, String)>,
    popover: &gtk::Popover,
    tab_view: &adw::TabView,
    url_entry: &gtk::Entry,
    back_btn: &gtk::Button,
    forward_btn: &gtk::Button,
) {
    let header = gtk::Label::new(Some(title));
    header.set_xalign(0.0);
    header.add_css_class("heading");
    header.set_margin_top(4);
    list.append(&header);

    if items.is_empty() {
        let empty = gtk::Label::new(Some("Vazio"));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        list.append(&empty);
        return;
    }

    for (url, label) in items {
        let lbl = gtk::Label::new(Some(&label));
        lbl.set_xalign(0.0);
        lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        lbl.set_max_width_chars(42);

        let row = gtk::Button::builder().child(&lbl).css_classes(["flat"]).build();
        row.set_tooltip_text(Some(&url));
        row.connect_clicked(glib::clone!(
            #[weak] popover, #[weak] tab_view, #[weak] url_entry, #[weak] back_btn, #[weak] forward_btn,
            move |_| {
                navigate(&tab_view, &url_entry, &back_btn, &forward_btn, &url);
                popover.popdown();
            }
        ));
        list.append(&row);
    }
}

/// Suspende abas em segundo plano paradas há mais de `discard_secs()`:
/// remove o WebView (liberando o WebProcess) e deixa um marcador no lugar.
fn discard_inactive(tab_view: &adw::TabView, activity: &Activity) {
    let Some(selected) = tab_view.selected_page() else {
        return;
    };
    let selected_key = page_key(&selected);
    let now = Instant::now();
    let limit = discard_secs();
    let mut act = activity.borrow_mut();

    for i in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(i);
        let key = page_key(&page);
        let entry = act.entry(key).or_insert((now, None));

        if key == selected_key {
            entry.0 = now; // a aba ativa nunca é suspensa
            continue;
        }
        if entry.1.is_some() {
            continue; // já suspensa
        }
        if now.duration_since(entry.0).as_secs() < limit {
            continue;
        }

        let Ok(container) = page.child().downcast::<gtk::Box>() else {
            continue;
        };
        let Some(child) = container.first_child() else {
            continue;
        };
        let Ok(webview) = child.downcast::<WebView>() else {
            continue; // página de início, nada a suspender
        };
        let url = webview.uri().map(|s| s.to_string()).unwrap_or_default();
        if url.is_empty() {
            continue;
        }

        container.remove(&webview); // descarta o WebView → libera o WebProcess
        container.append(&suspended_placeholder());
        eprintln!("[tucano] aba suspensa: {url}");
        entry.1 = Some(url);
        page.set_loading(false);
    }
}

/// Marcador exibido no lugar de uma aba suspensa.
fn suspended_placeholder() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
    b.set_halign(gtk::Align::Center);
    b.set_valign(gtk::Align::Center);
    b.set_hexpand(true);
    b.set_vexpand(true);

    let icon = gtk::Image::from_icon_name("content-loading-symbolic");
    icon.set_pixel_size(48);
    icon.set_opacity(0.5);

    let label = gtk::Label::new(Some(
        "Aba suspensa para poupar memória\nSelecione-a para recarregar",
    ));
    label.set_justify(gtk::Justification::Center);
    label.add_css_class("dim-label");

    b.append(&icon);
    b.append(&label);
    b
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
    format!("https://www.google.com/search?q={q}")
}
