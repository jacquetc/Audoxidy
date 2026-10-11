//! The Teksilo UI of Audoxidy (`teksilo-ui` feature). The Iced UI stays the
//! default binary, `audoxidy`.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    audoxidy::teksilo_ui::run();
}
