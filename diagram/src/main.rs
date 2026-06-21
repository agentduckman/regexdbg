mod page;
mod serve;

fn main() {
    let pattern_path: std::path::PathBuf = std::env::args()
        .nth(1)
        .expect("usage: regexdbg-diagram <pattern-file>")
        .into();

    let port = serve::run(pattern_path);
    open_browser(&format!("http://127.0.0.1:{port}/"));

    // Block the main thread — server work happens in spawned threads.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(target_os = "macos"))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}
