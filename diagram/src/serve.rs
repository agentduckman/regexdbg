use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

pub fn run(pattern_path: PathBuf) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind HTTP server");
    let port = listener.local_addr().unwrap().port();
    let page = Arc::new(crate::page::html());
    let path = Arc::new(pattern_path);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let path = Arc::clone(&path);
            let page = Arc::clone(&page);
            thread::spawn(move || handle_connection(stream, &path, &page));
        }
    });
    port
}

fn handle_connection(mut stream: TcpStream, pattern_path: &std::path::Path, page: &str) {
    let route = {
        let mut reader = BufReader::new(&stream);
        let mut req_line = String::new();
        if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
            return;
        }
        // Drain remaining request headers.
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        req_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_string()
    };

    match route.as_str() {
        "/" => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                page.len(),
                page
            );
            let _ = stream.write_all(response.as_bytes());
        }
        "/pattern" => {
            let body = std::fs::read_to_string(pattern_path).unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    }
}
