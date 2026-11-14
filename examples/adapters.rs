//! The graphics adapters of this machine, in the order the default renderer tries them.
//!
//! Run it with `cargo run -p zgui-examples --example adapters`, or add part of an adapter's name
//! to see the order with that adapter preferred: `--example adapters -- intel`.
//!
//! It opens no window. An application passes the same preference to
//! `App::with_adapter_preference`.

use zgui::AdapterPreference;

fn main() {
    let preference = match std::env::args().nth(1) {
        Some(name) => AdapterPreference::NameContains(name),
        None => AdapterPreference::Automatic,
    };

    for info in zgui::app::adapters(&preference) {
        println!(
            "{} ({:?}, {:?}, vendor 0x{:04x}, device 0x{:04x})",
            info.name, info.device_type, info.backend, info.vendor, info.device
        );
    }
}
