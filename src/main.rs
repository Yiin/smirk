mod data;
mod recents;

use adw::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use data::{Catalog, GROUPS};
use recents::Recents;

const APP_ID: &str = "lt.yiin.smirk";
const COLUMNS: u32 = 8;

const CSS: &str = "
.emoji-cell { font-size: 22px; padding: 3px; }
gridview > child { border-radius: 10px; }
gridview > child:hover { background: alpha(currentColor, 0.08); }
gridview > child:selected { background: alpha(@accent_bg_color, 0.25); }
.category-bar button { font-size: 15px; padding: 2px 6px; min-height: 28px; }
";

struct Ui {
    window: adw::ApplicationWindow,
    entry: gtk::SearchEntry,
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.add_main_option(
        "start-hidden",
        glib::Char::from(b's'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Start without showing the window",
        None,
    );

    let start_hidden = Rc::new(Cell::new(false));
    app.connect_handle_local_options({
        let start_hidden = start_hidden.clone();
        move |_, options| {
            start_hidden.set(options.contains("start-hidden"));
            std::ops::ControlFlow::Continue(())
        }
    });

    app.connect_startup(|app| {
        // Stay resident even if every window is destroyed.
        std::mem::forget(app.hold());

        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("no display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    let ui: Rc<RefCell<Option<Ui>>> = Rc::new(RefCell::new(None));
    app.connect_activate({
        let ui = ui.clone();
        let start_hidden = start_hidden.clone();
        move |app| {
            let mut ui_ref = ui.borrow_mut();
            let ui = ui_ref.get_or_insert_with(|| build_ui(app));
            if start_hidden.replace(false) {
                return;
            }
            ui.window.present();
            ui.entry.grab_focus();
        }
    });

    app.run()
}

fn build_ui(app: &adw::Application) -> Ui {
    let catalog = Rc::new(Catalog::load());
    let recents = Rc::new(RefCell::new(Recents::load()));
    let category = Rc::new(Cell::new(None::<emojis::Group>));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("smirk")
        .default_width(400)
        .default_height(500)
        .build();

    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Search emoji")
        .hexpand(true)
        .build();

    let model = gtk::StringList::new(&[]);
    let selection = gtk::SingleSelection::new(Some(model.clone()));

    let grid = gtk::GridView::builder()
        .model(&selection)
        .min_columns(COLUMNS)
        .max_columns(COLUMNS)
        .single_click_activate(true)
        .build();

    // One shared closure: copy the emoji, record it, hide, paste.
    let on_pick: Rc<dyn Fn(&str)> = Rc::new({
        let window = window.clone();
        let entry = entry.clone();
        let recents = recents.clone();
        move |emoji: &str| {
            let _ = Command::new("wl-copy").arg(emoji).status();
            recents.borrow_mut().push(emoji);
            entry.set_text("");
            window.set_visible(false);
            // Give the compositor time to refocus the previous window first.
            glib::timeout_add_local_once(Duration::from_millis(120), || {
                let _ = Command::new("wtype")
                    .args(["-M", "ctrl", "v", "-m", "ctrl"])
                    .status();
            });
        }
    });

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup({
        let on_pick = on_pick.clone();
        move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.add_css_class("emoji-cell");
            item.set_child(Some(&label));

            // Right-click: skin tone variants.
            let gesture = gtk::GestureClick::builder().button(3).build();
            gesture.connect_pressed({
                let label = label.clone();
                let on_pick = on_pick.clone();
                move |_, _, _, _| show_skintones(&label, &on_pick)
            });
            label.add_controller(gesture);
        }
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let s = item.item().and_downcast::<gtk::StringObject>().unwrap();
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&s.string());
        label.set_tooltip_text(emojis::get(&s.string()).map(|e| e.name()));
    });
    grid.set_factory(Some(&factory));

    grid.connect_activate({
        let on_pick = on_pick.clone();
        move |grid, pos| {
            if let Some(s) = grid
                .model()
                .and_then(|m| m.item(pos))
                .and_downcast::<gtk::StringObject>()
            {
                on_pick(&s.string());
            }
        }
    });

    let refilter: Rc<dyn Fn()> = Rc::new({
        let catalog = catalog.clone();
        let recents = recents.clone();
        let category = category.clone();
        let entry = entry.clone();
        let model = model.clone();
        let grid = grid.clone();
        move || {
            let query = entry.text();
            let results = catalog.search(&query, category.get());
            let mut items: Vec<&str> = Vec::with_capacity(results.len() + 32);
            if query.trim().is_empty() && category.get().is_none() {
                let recents = recents.borrow();
                items.extend(recents.emojis());
                items.extend(results.iter().map(|e| e.as_str()));
                model.splice(0, model.n_items(), &items);
            } else {
                items.extend(results.iter().map(|e| e.as_str()));
                model.splice(0, model.n_items(), &items);
            }
            if model.n_items() > 0 {
                grid.scroll_to(0, gtk::ListScrollFlags::NONE, None);
            }
        }
    });
    refilter();

    entry.connect_search_changed({
        let refilter = refilter.clone();
        move |_| refilter()
    });

    // Enter in search: pick the first (best-ranked) result.
    entry.connect_activate({
        let on_pick = on_pick.clone();
        let model = model.clone();
        move |_| {
            if let Some(s) = model.item(0).and_downcast::<gtk::StringObject>() {
                on_pick(&s.string());
            }
        }
    });

    // Down arrow moves from search into the grid.
    let entry_keys = gtk::EventControllerKey::new();
    entry_keys.connect_key_pressed({
        let grid = grid.clone();
        move |_, keyval, _, _| {
            if keyval == gdk::Key::Down {
                grid.grab_focus();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    entry.add_controller(entry_keys);

    // Type anywhere: keystrokes land in the search entry.
    entry.set_key_capture_widget(Some(&window));

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&grid)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();

    // Category bar: "all" plus one toggle per emoji group.
    let bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .homogeneous(true)
        .css_classes(["category-bar"])
        .build();
    let all_btn = gtk::ToggleButton::builder().label("⭐").active(true).build();
    all_btn.connect_toggled({
        let category = category.clone();
        let refilter = refilter.clone();
        move |b| {
            if b.is_active() {
                category.set(None);
                refilter();
            }
        }
    });
    bar.append(&all_btn);
    for (icon, group) in GROUPS {
        let btn = gtk::ToggleButton::builder().label(*icon).build();
        btn.set_group(Some(&all_btn));
        btn.connect_toggled({
            let category = category.clone();
            let refilter = refilter.clone();
            let group = *group;
            move |b| {
                if b.is_active() {
                    category.set(Some(group));
                    refilter();
                }
            }
        });
        bar.append(&btn);
    }

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .build();
    content.append(&entry);
    content.append(&scrolled);
    content.append(&bar);
    window.set_content(Some(&content));

    // Escape and window close both hide; the process stays resident.
    let hide = Rc::new({
        let window = window.clone();
        let entry = entry.clone();
        move || {
            entry.set_text("");
            window.set_visible(false);
        }
    });

    let window_keys = gtk::EventControllerKey::new();
    window_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    window_keys.connect_key_pressed({
        let hide = hide.clone();
        move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                hide();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    window.add_controller(window_keys);

    window.connect_close_request({
        let hide = hide.clone();
        move |_| {
            hide();
            glib::Propagation::Stop
        }
    });

    Ui { window, entry }
}

fn show_skintones(label: &gtk::Label, on_pick: &Rc<dyn Fn(&str)>) {
    let text = label.text();
    let Some(emoji) = emojis::get(&text) else { return };
    let Some(tones) = emoji.skin_tones() else { return };

    let flow = gtk::FlowBox::builder()
        .max_children_per_line(6)
        .selection_mode(gtk::SelectionMode::None)
        .build();

    let popover = gtk::Popover::new();
    popover.set_parent(label);
    for variant in tones {
        let btn = gtk::Button::builder().label(variant.as_str()).build();
        btn.add_css_class("emoji-cell");
        btn.add_css_class("flat");
        btn.connect_clicked({
            let on_pick = on_pick.clone();
            let popover = popover.clone();
            let s = variant.as_str();
            move |_| {
                popover.popdown();
                on_pick(s);
            }
        });
        flow.append(&btn);
    }
    popover.set_child(Some(&flow));
    popover.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    popover.popup();
}
