//! Scans a music folder into `./library.db`, with the covers in `./cache/covers`, as the
//! UIs' "Abrir Carpeta" does, without a folder dialog: for live checks against a test library
//! (`tools/make_test_library.py`).
//!
//! Run it from the directory the app will run from (both UIs open `library.db` and
//! `cache/covers` relative to the working directory):
//!
//! ```text
//! cd <run dir> && cargo run --manifest-path <repo>/Cargo.toml --example scan_library -- <folder>
//! ```
//!
//! It waits for the scan, then for the cover cache to stop growing, and prints the totals.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audoxidy::db::database::Database;
use audoxidy::db::scanner::Scanner;

fn covers() -> usize {
    std::fs::read_dir("cache/covers").map_or(0, |dir| dir.count())
}

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        eprintln!("usage: scan_library <music folder>");
        std::process::exit(2);
    };
    if !Path::new(&folder).is_dir() {
        eprintln!("{folder} is not a directory");
        std::process::exit(2);
    }
    let database = match Database::new() {
        Ok(database) => database,
        Err(error) => {
            eprintln!("cannot open library.db: {error}");
            std::process::exit(1);
        }
    };
    let database = Arc::new(Mutex::new(database));
    let scanner = Scanner::new(Arc::clone(&database));
    let started = Instant::now();
    scanner.scan_folder_async(folder.clone());
    // The scan thread raises the flag as it starts.
    while !scanner.is_scanning.load(Ordering::SeqCst) && !scanner.db_dirty.load(Ordering::SeqCst) {
        if started.elapsed() > Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    while scanner.is_scanning.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }
    // The covers are encoded on a pool after the scan: wait until the cache stops growing.
    let (mut last, mut stable_since) = (covers(), Instant::now());
    while stable_since.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(250));
        let now = covers();
        if now != last {
            last = now;
            stable_since = Instant::now();
        }
    }
    let songs = database
        .lock()
        .ok()
        .and_then(|db| db.get_all_songs().ok())
        .map_or(0, |songs| songs.len());
    println!(
        "scanned {folder}: {songs} songs, {last} covers, in {:.1} s",
        started.elapsed().as_secs_f64()
    );
}
