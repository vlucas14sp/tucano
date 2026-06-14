//! Downloads: salva em `~/Downloads` e mostra progresso num popover.

use std::path::PathBuf;

use adw::prelude::*;
use gtk::{gio, glib};

/// Liga o botão de downloads aos eventos da sessão de rede compartilhada.
pub fn attach(menu_btn: &gtk::MenuButton) {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list.set_margin_top(6);
    list.set_margin_bottom(6);
    list.set_margin_start(6);
    list.set_margin_end(6);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(420)
        .propagate_natural_height(true)
        .child(&list)
        .build();

    let popover = gtk::Popover::new();
    popover.set_child(Some(&scroller));
    popover.set_size_request(340, -1);
    menu_btn.set_popover(Some(&popover));
    menu_btn.set_sensitive(false); // habilita no primeiro download

    crate::session::session().connect_download_started(glib::clone!(
        #[weak] menu_btn, #[weak] list,
        move |_, download| on_download(&menu_btn, &list, download)
    ));
}

fn on_download(menu_btn: &gtk::MenuButton, list: &gtk::Box, download: &webkit::Download) {
    menu_btn.set_sensitive(true);

    let name = gtk::Label::new(Some("Baixando…"));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(38);

    let bar = gtk::ProgressBar::new();
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");
    footer.append(&status);

    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.append(&name);
    row.append(&bar);
    row.append(&footer);
    list.prepend(&row);

    // Escolhe o destino em ~/Downloads (sem sobrescrever).
    download.connect_decide_destination(glib::clone!(
        #[weak] name,
        #[upgrade_or] false,
        move |d, suggested| {
            let dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                .unwrap_or_else(glib::home_dir);
            let dest = unique_path(dir.join(suggested));
            if let Some(file) = dest.file_name() {
                name.set_text(&file.to_string_lossy());
            }
            if let Some(path) = dest.to_str() {
                d.set_destination(path);
            }
            true
        }
    ));

    download.connect_estimated_progress_notify(glib::clone!(
        #[weak] bar,
        move |d| bar.set_fraction(d.estimated_progress())
    ));

    download.connect_finished(glib::clone!(
        #[weak] bar, #[weak] status, #[weak] footer,
        move |d| {
            bar.set_fraction(1.0);
            status.set_text("Concluído");
            let dest = d.destination().unwrap_or_default().to_string();
            if !dest.is_empty() {
                let open = gtk::Button::with_label("Abrir");
                open.add_css_class("flat");
                open.connect_clicked(move |_| {
                    let uri = format!("file://{dest}");
                    let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
                });
                footer.append(&open);
            }
        }
    ));

    download.connect_failed(glib::clone!(
        #[weak] status,
        move |_, error| status.set_text(&format!("Falhou: {}", error.message()))
    ));
}

/// Evita sobrescrever: acrescenta " (n)" ao nome se o arquivo já existir.
fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let parent = path.parent().map(PathBuf::from).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for i in 1.. {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}
