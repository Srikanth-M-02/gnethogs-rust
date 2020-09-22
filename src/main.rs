use std::{collections::HashMap, env::args, ffi::CStr};

use gio::prelude::*;
use glib::Sender;
use gtk::{prelude::*, TreeIter};
use once_cell::sync::OnceCell;

static SENDER: OnceCell<Sender<NetHogsData>> = OnceCell::new();

#[derive(Debug, Clone, Copy)]
enum Columns {
    Pid,
    User,
    Program,
    Device,
    Sent,
    Received,
}

struct NetHogsData {
    action: i32,
    id: i32,
    pid: i32,
    user: u32,
    program: String,
    device: String,
    sent: f32,
    received: f32,
}

enum NetHogsAppAction {
    Set = 1,
    Remove = 2,
}

unsafe extern "C" fn callback(action: i32, data: *const nethogs_sys::NethogsMonitorRecord) {
    SENDER
        .get()
        .unwrap()
        .send(NetHogsData {
            action,
            id: (*data).record_id,
            pid: (*data).pid,
            user: (*data).uid,
            program: CStr::from_ptr((*data).name).to_str().unwrap().to_owned(),
            device: CStr::from_ptr((*data).device_name)
                .to_str()
                .unwrap()
                .to_owned(),
            sent: (*data).sent_kbs,
            received: (*data).recv_kbs,
        })
        .unwrap()
}
fn build_ui(application: &gtk::Application) {
    let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    SENDER.set(sender).unwrap();
    unsafe {
        std::thread::spawn(|| {
            nethogs_sys::nethogsmonitor_loop(Some(callback), std::ptr::null_mut(), 100)
        });
    }
    let window = gtk::ApplicationWindow::new(application);
    window.connect_destroy(|_window| unsafe {
        nethogs_sys::nethogsmonitor_breakloop();
    });
    window.set_title("GNethogs");
    window.set_border_width(10);
    window.set_default_size(1000, 600);
    window.set_position(gtk::WindowPosition::Center);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    window.add(&vbox);
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sent_label = gtk::Label::new(Some(""));
    let received_label = gtk::Label::new(Some(""));
    hbox.add(&sent_label);
    hbox.add(&received_label);
    hbox.set_homogeneous(true);
    let sw = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    sw.set_shadow_type(gtk::ShadowType::EtchedIn);
    sw.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    vbox.add(&sw);
    vbox.add(&hbox);

    let col_types = [
        glib::Type::I32,
        glib::Type::String,
        glib::Type::String,
        glib::Type::String,
        glib::Type::F32,
        glib::Type::F32,
    ];

    let store = gtk::ListStore::new(&col_types);
    let columns = [
        Columns::Pid,
        Columns::User,
        Columns::Program,
        Columns::Device,
        Columns::Sent,
        Columns::Received,
    ];
    let col_indices = (Columns::Pid as u32..=Columns::Received as u32).collect::<Vec<_>>();

    let treeview = gtk::TreeView::with_model(&store);
    treeview.set_vexpand(true);
    treeview.set_search_column(Columns::Program as i32);

    sw.add(&treeview);

    for i in columns.iter() {
        let renderer = gtk::CellRendererText::new();
        let column = gtk::TreeViewColumn::new();
        column.pack_start(&renderer, true);
        column.set_title(&format!("{:?}", i));
        column.set_resizable(true);
        column.add_attribute(&renderer, "text", *i as i32);
        column.set_sort_column_id(*i as i32);
        treeview.append_column(&column);
    }
    window.show_all();
    use users::{Users, UsersCache};

    let cache = UsersCache::new();
    let mut map: HashMap<i32, (TreeIter, f32, f32)> = HashMap::new();
    receiver.attach(None, move |msg| {
        if msg.action == NetHogsAppAction::Remove as i32 {
            store.remove(&map.get(&msg.id).unwrap().0);
            map.remove(&msg.id);
            sent_label
                .set_text(format!("Sent: {}kbps",map.iter().map(|e|e.1.1).sum::<f32>()).as_str());
            received_label.set_text(
                format!("Received: {}kbps",map.iter().map(|e|e.1.2).sum::<f32>()).as_str(),
            )
        } else if msg.action == NetHogsAppAction::Set as i32 {
            if map.contains_key(&msg.id) {
                let user: String = cache
                    .get_user_by_uid(msg.user)
                    .unwrap()
                    .name()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let values: [&dyn ToValue; 6] = [
                    &msg.pid,
                    &user,
                    &msg.program,
                    &msg.device,
                    &msg.sent,
                    &msg.received,
                ];
                let iter = map.get(&msg.id).unwrap().0.clone();
                store.set(&iter, &col_indices, &values);
                map.insert(msg.id, (iter, msg.sent, msg.received));
                sent_label.set_text(
                    format!("Sent: {}kbps",map.iter().map(|e|e.1.1).sum::<f32>()).as_str(),
                );
                received_label.set_text(
                    format!("Received: {}kbps",map.iter().map(|e|e.1.2).sum::<f32>()).as_str(),
                )
            } else {
                let user: String = cache
                    .get_user_by_uid(msg.user)
                    .unwrap()
                    .name()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let values: [&dyn ToValue; 6] = [
                    &msg.pid,
                    &user,
                    &msg.program,
                    &msg.device,
                    &msg.sent,
                    &msg.received,
                ];
                let iter = store.append();
                store.set(&iter, &col_indices, &values);
                map.insert(msg.id, (iter, msg.sent, msg.received));
                sent_label.set_text(
                    format!("Sent: {}kbps",map.iter().map(|e|e.1.1).sum::<f32>()).as_str(),
                );
                received_label.set_text(
                    format!("Received: {}kbps",map.iter().map(|e|e.1.2).sum::<f32>()).as_str(),
                )
            }
        }
        Continue(true)
    });
}

fn main() {
    let application = gtk::Application::new(Some("com.example.gnethogs"), Default::default())
        .expect("Initialization failed...");

    application.connect_activate(|app| {
        build_ui(app);
    });

    application.run(&args().collect::<Vec<_>>());
}
