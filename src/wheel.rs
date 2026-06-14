//! Roda de abas radial estilo "seleção de armas".
//!
//! As abas ficam ocultas no topo; um botão flutuante de tucano, no centro-
//! esquerda, revela meia-roda de abas que gira conforme o scroll do mouse.
//! A aba que estiver no centro da roda é selecionada ao vivo.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib, pango};

// Geometria da roda (a "metade" visível fica à direita do centro do círculo).
const FIXED_W: i32 = 380;
const FIXED_H: i32 = 640;
const RADIUS: f64 = 300.0;
const CENTER_X: f64 = -40.0; // centro do círculo fica à esquerda da área visível
const CENTER_Y: f64 = 320.0;
const SPACING: f64 = 0.42; // distância angular entre abas (~24°)
const VISIBLE: f64 = 1.65; // abas além deste ângulo (rad) ficam ocultas
const ITEM_W: f64 = 220.0;
const ITEM_H: f64 = 40.0;

struct WheelState {
    /// Posição (fracionária) exibida no momento; interpolada a cada frame.
    rotation: f64,
    /// Índice de destino para onde a roda está girando.
    target: f64,
    /// Se há uma animação de giro em andamento.
    animating: bool,
    /// `true` quando a roda foi acionada via Ctrl+Tab (fecha ao soltar o Ctrl).
    ctrl_cycling: bool,
    /// Timer de inatividade que oculta a roda sozinha (modo scroll/manual).
    hide_source: Option<glib::SourceId>,
    items: Vec<(gtk::Button, adw::TabPage)>,
}

/// Fração interpolada por frame (ease-out): quanto maior, mais rápido o giro.
const EASE: f64 = 0.20;
/// Tempo de inatividade (ms) até a roda se ocultar sozinha.
const HIDE_MS: u64 = 1800;

type State = Rc<RefCell<WheelState>>;

/// Adiciona o botão de tucano e a roda como sobreposições do `overlay`,
/// e registra os atalhos de teclado que acionam a roda.
pub fn attach(
    overlay: &gtk::Overlay,
    tab_view: &adw::TabView,
    app: &adw::Application,
    window: &adw::ApplicationWindow,
) {
    // Desativa os atalhos nativos do AdwTabView (Ctrl+Tab etc.) para que a roda
    // assuma esses gestos.
    tab_view.set_shortcuts(adw::TabViewShortcuts::empty());

    let fixed = gtk::Fixed::new();
    fixed.set_size_request(FIXED_W, FIXED_H);

    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .reveal_child(false)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .child(&fixed)
        .build();

    let state: State = Rc::new(RefCell::new(WheelState {
        rotation: 0.0,
        target: 0.0,
        animating: false,
        ctrl_cycling: false,
        hide_source: None,
        items: Vec::new(),
    }));

    // Scroll gira a roda e seleciona a aba central.
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(glib::clone!(
        #[strong] state, #[weak] fixed, #[weak] tab_view, #[weak] revealer,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, _dx, dy| {
            let dir = if dy > 0.0 { 1.0 } else if dy < 0.0 { -1.0 } else { 0.0 };
            {
                let mut st = state.borrow_mut();
                let n = st.items.len();
                if n == 0 {
                    return glib::Propagation::Proceed;
                }
                // Ao rolar com o mouse, a roda passa ao modo manual (não fecha
                // ao soltar o Ctrl) e passa a contar o timer de auto-ocultar.
                st.ctrl_cycling = false;
                st.target = (st.target + dir).clamp(0.0, (n - 1) as f64);
            }
            start_animation(&fixed, &state, &tab_view);
            schedule_hide(&state, &revealer);
            glib::Propagation::Stop
        }
    ));
    fixed.add_controller(scroll);

    // Botão flutuante de tucano: abre/fecha a roda.
    let toucan = gtk::Button::builder().child(&toucan_image()).build();
    toucan.add_css_class("circular");
    toucan.add_css_class("osd");
    toucan.set_size_request(56, 56);
    toucan.set_halign(gtk::Align::Start);
    toucan.set_valign(gtk::Align::Center);
    toucan.set_margin_start(10);
    toucan.set_tooltip_text(Some("Abas — clique e role para girar"));

    toucan.connect_clicked(glib::clone!(
        #[strong] state, #[weak] fixed, #[weak] tab_view, #[weak] revealer,
        move |_| toggle_wheel(&fixed, &state, &tab_view, &revealer)
    ));

    overlay.add_overlay(&revealer);
    overlay.add_overlay(&toucan);

    // Mantém a roda em dia quando abas são abertas/fechadas com ela aberta.
    tab_view.connect_page_attached(glib::clone!(
        #[strong] state, #[weak] fixed, #[weak] revealer,
        move |tv, _, _| if revealer.reveals_child() {
            rebuild(&fixed, &state, tv, &revealer);
        }
    ));
    tab_view.connect_page_detached(glib::clone!(
        #[strong] state, #[weak] fixed, #[weak] revealer,
        move |tv, _, _| if revealer.reveals_child() {
            rebuild(&fixed, &state, tv, &revealer);
        }
    ));

    // --- Atalhos de teclado que acionam a roda ---

    // Ctrl+E: liga/desliga a roda (depois use o scroll para girar).
    let act_toggle = gio::SimpleAction::new("wheel-toggle", None);
    act_toggle.connect_activate(glib::clone!(
        #[weak] fixed, #[strong] state, #[weak] tab_view, #[weak] revealer,
        move |_, _| toggle_wheel(&fixed, &state, &tab_view, &revealer)
    ));
    window.add_action(&act_toggle);
    app.set_accels_for_action("win.wheel-toggle", &["<Ctrl>e"]);

    // Ctrl+Tab / Ctrl+Shift+Tab: aciona a roda e avança/volta uma aba.
    let act_next = gio::SimpleAction::new("wheel-next", None);
    act_next.connect_activate(glib::clone!(
        #[weak] fixed, #[strong] state, #[weak] tab_view, #[weak] revealer,
        move |_, _| cycle_wheel(&fixed, &state, &tab_view, &revealer, 1.0)
    ));
    window.add_action(&act_next);
    app.set_accels_for_action("win.wheel-next", &["<Ctrl>Tab"]);

    let act_prev = gio::SimpleAction::new("wheel-prev", None);
    act_prev.connect_activate(glib::clone!(
        #[weak] fixed, #[strong] state, #[weak] tab_view, #[weak] revealer,
        move |_, _| cycle_wheel(&fixed, &state, &tab_view, &revealer, -1.0)
    ));
    window.add_action(&act_prev);
    app.set_accels_for_action(
        "win.wheel-prev",
        &["<Ctrl><Shift>Tab", "<Ctrl><Shift>ISO_Left_Tab"],
    );

    // Soltar o Ctrl fecha a roda quando ela foi acionada via Ctrl+Tab.
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    key.connect_key_released(glib::clone!(
        #[strong] state, #[weak] revealer,
        move |_, keyval, _code, _mods| {
            if matches!(keyval, gdk::Key::Control_L | gdk::Key::Control_R)
                && state.borrow().ctrl_cycling
            {
                close_wheel(&state, &revealer);
            }
        }
    ));
    window.add_controller(key);
}

/// Abre a roda (reconstruindo os itens) se ainda não estiver visível.
fn open_wheel(fixed: &gtk::Fixed, state: &State, tab_view: &adw::TabView, revealer: &gtk::Revealer) {
    if !revealer.reveals_child() {
        rebuild(fixed, state, tab_view, revealer);
        revealer.set_reveal_child(true);
    }
}

/// Fecha a roda, cancela o timer de auto-ocultar e zera a ciclagem por Ctrl.
fn close_wheel(state: &State, revealer: &gtk::Revealer) {
    if let Some(id) = state.borrow_mut().hide_source.take() {
        id.remove();
    }
    revealer.set_reveal_child(false);
    state.borrow_mut().ctrl_cycling = false;
}

/// (Re)agenda o fechamento automático da roda após `HIDE_MS` de inatividade.
fn schedule_hide(state: &State, revealer: &gtk::Revealer) {
    if let Some(id) = state.borrow_mut().hide_source.take() {
        id.remove();
    }
    let id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(HIDE_MS),
        glib::clone!(
            #[strong] state, #[weak] revealer,
            move || {
                state.borrow_mut().hide_source = None;
                close_wheel(&state, &revealer);
            }
        ),
    );
    state.borrow_mut().hide_source = Some(id);
}

/// Alterna a roda no modo manual (auto-oculta após inatividade).
fn toggle_wheel(fixed: &gtk::Fixed, state: &State, tab_view: &adw::TabView, revealer: &gtk::Revealer) {
    if revealer.reveals_child() {
        close_wheel(state, revealer);
    } else {
        state.borrow_mut().ctrl_cycling = false;
        open_wheel(fixed, state, tab_view, revealer);
        schedule_hide(state, revealer);
    }
}

/// Aciona a roda em modo Ctrl+Tab e gira `dir` aba(s) (fecha ao soltar o Ctrl).
fn cycle_wheel(
    fixed: &gtk::Fixed,
    state: &State,
    tab_view: &adw::TabView,
    revealer: &gtk::Revealer,
    dir: f64,
) {
    open_wheel(fixed, state, tab_view, revealer);
    {
        let mut st = state.borrow_mut();
        // Em modo Ctrl+Tab o fechamento é por soltar o Ctrl: cancela o timer.
        if let Some(id) = st.hide_source.take() {
            id.remove();
        }
        st.ctrl_cycling = true;
        let n = st.items.len();
        if n == 0 {
            return;
        }
        st.target = (st.target + dir).clamp(0.0, (n - 1) as f64);
    }
    start_animation(fixed, state, tab_view);
}

/// Inicia (se já não houver) a animação de giro rumo ao `target`.
fn start_animation(fixed: &gtk::Fixed, state: &State, tab_view: &adw::TabView) {
    {
        let mut st = state.borrow_mut();
        if st.animating {
            return; // a animação em curso já seguirá o novo alvo
        }
        st.animating = true;
    }
    fixed.add_tick_callback(glib::clone!(
        #[strong] state, #[weak] tab_view,
        #[upgrade_or] glib::ControlFlow::Break,
        move |fixed, _clock| {
            let (rotation, target) = {
                let st = state.borrow();
                (st.rotation, st.target)
            };
            let diff = target - rotation;
            if diff.abs() < 0.002 {
                {
                    let mut st = state.borrow_mut();
                    st.rotation = target;
                    st.animating = false;
                }
                reposition(fixed, &state.borrow(), &tab_view);
                return glib::ControlFlow::Break;
            }
            state.borrow_mut().rotation += diff * EASE;
            reposition(fixed, &state.borrow(), &tab_view);
            glib::ControlFlow::Continue
        }
    ));
}

/// Recria os botões da roda a partir das abas atuais e centraliza na ativa.
fn rebuild(fixed: &gtk::Fixed, state: &State, tab_view: &adw::TabView, revealer: &gtk::Revealer) {
    for (btn, _) in state.borrow_mut().items.drain(..) {
        fixed.remove(&btn);
    }

    for i in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(i);

        let label = gtk::Label::new(None);
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_max_width_chars(18);
        page.bind_property("title", &label, "label").sync_create().build();

        let btn = gtk::Button::builder().child(&label).build();
        btn.add_css_class("osd");
        btn.add_css_class("pill");
        btn.set_size_request(ITEM_W as i32, ITEM_H as i32);
        fixed.put(&btn, 0.0, 0.0);

        // Clicar numa aba a centraliza, seleciona e fecha a roda.
        btn.connect_clicked(glib::clone!(
            #[strong] state, #[weak] fixed, #[weak] tab_view, #[weak] revealer, #[strong] page,
            move |_| {
                let pos = state.borrow().items.iter().position(|(_, p)| p == &page);
                if let Some(pos) = pos {
                    let mut st = state.borrow_mut();
                    st.rotation = pos as f64;
                    st.target = pos as f64;
                }
                reposition(&fixed, &state.borrow(), &tab_view);
                tab_view.set_selected_page(&page);
                revealer.set_reveal_child(false);
            }
        ));

        state.borrow_mut().items.push((btn, page));
    }

    if let Some(sel) = tab_view.selected_page() {
        let pos = tab_view.page_position(&sel) as f64;
        let mut st = state.borrow_mut();
        st.rotation = pos;
        st.target = pos;
        st.animating = false;
    }
    reposition(fixed, &state.borrow(), tab_view);
}

/// Posiciona cada aba no arco conforme a rotação e seleciona a central.
fn reposition(fixed: &gtk::Fixed, st: &WheelState, tab_view: &adw::TabView) {
    if st.items.is_empty() {
        return;
    }
    for (i, (btn, _)) in st.items.iter().enumerate() {
        let theta = (i as f64 - st.rotation) * SPACING;
        if theta.abs() > VISIBLE {
            btn.set_visible(false);
            continue;
        }
        let x = CENTER_X + RADIUS * theta.cos() - ITEM_W / 2.0;
        let y = CENTER_Y + RADIUS * theta.sin() - ITEM_H / 2.0;
        btn.set_visible(true);
        fixed.move_(btn, x, y);

        // Abas mais afastadas do centro ficam menores/translúcidas.
        let depth = theta.cos().clamp(0.0, 1.0);
        btn.set_opacity(0.30 + 0.70 * depth);
        if theta.abs() < SPACING / 2.0 {
            btn.add_css_class("suggested-action");
        } else {
            btn.remove_css_class("suggested-action");
        }
    }

    let idx = st.rotation.round() as usize;
    if let Some((_, page)) = st.items.get(idx) {
        if tab_view.selected_page().as_ref() != Some(page) {
            tab_view.set_selected_page(page);
        }
    }
}

/// Carrega o ícone do tucano (SVG embutido) com fallback para um ícone do tema.
fn toucan_image() -> gtk::Image {
    let bytes = glib::Bytes::from_static(include_bytes!("../assets/tucano.svg"));
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let image = match gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, 40, 40, true, gio::Cancellable::NONE) {
        Ok(pixbuf) => {
            let texture = gdk::Texture::for_pixbuf(&pixbuf);
            gtk::Image::from_paintable(Some(&texture))
        }
        Err(_) => gtk::Image::from_icon_name("starred-symbolic"),
    };
    image.set_pixel_size(34);
    image
}
