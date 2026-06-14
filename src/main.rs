mod browser;

use adw::prelude::*;
use gtk::glib;

const APP_ID: &str = "io.github.vlucas14sp.Tucano";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(browser::build_window);
    app.run()
}
