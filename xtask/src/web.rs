//! Builds the gallery for `wasm32-unknown-unknown` and serves it.
//!
//! The build is single-threaded (`gpui_platform` without `multithreaded`),
//! so it runs on stable Rust and needs no cross-origin isolation headers.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

const TARGET: &str = "wasm32-unknown-unknown";
const BIN: &str = "astryx-gallery";

pub fn run(args: &[String]) -> Result<()> {
    let release = args.iter().any(|a| a == "--release");
    let serve = !args.iter().any(|a| a == "--no-serve");
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .map(|p| p.parse::<u16>())
        .transpose()
        .context("--port takes a number")?
        .unwrap_or(8080);

    let root = workspace_root();
    let mut build = Command::new(env_or("CARGO", "cargo"));
    build
        .current_dir(&root)
        .args(["build", "-p", BIN, "--target", TARGET]);
    if release {
        build.arg("--release");
    }
    status(&mut build, "cargo build")?;

    let profile = if release { "release" } else { "debug" };
    let wasm = root
        .join("target")
        .join(TARGET)
        .join(profile)
        .join(format!("{BIN}.wasm"));
    let dist = root.join("apps/gallery/dist");
    fs::create_dir_all(&dist)?;
    let mut bindgen = Command::new("wasm-bindgen");
    bindgen
        .arg("--target")
        .arg("web")
        .arg("--no-typescript")
        .arg("--out-dir")
        .arg(&dist)
        .arg(&wasm);
    status(
        &mut bindgen,
        "wasm-bindgen (install: cargo install wasm-bindgen-cli --version <Cargo.lock version>)",
    )?;
    fs::copy(
        root.join("apps/gallery/web/index.html"),
        dist.join("index.html"),
    )?;
    println!("built {}", dist.display());

    if serve {
        serve_dir(&dist, port)?;
    }
    Ok(())
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

fn status(command: &mut Command, what: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to start {what}"))?;
    if !status.success() {
        bail!("{what} failed with {status}");
    }
    Ok(())
}

/// A minimal static file server for local testing only.
fn serve_dir(dir: &Path, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("port {port} is in use; pass --port"))?;
    println!("serving http://127.0.0.1:{port}/ (Ctrl+C to stop)");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if let Err(error) = respond(dir, stream) {
            eprintln!("request failed: {error:#}");
        }
    }
    Ok(())
}

fn respond(dir: &Path, mut stream: TcpStream) -> Result<()> {
    let mut request_line = String::new();
    BufReader::new(&stream).read_line(&mut request_line)?;
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");
    let relative = path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if relative.split('/').any(|part| part == "..") {
        return write_response(&mut stream, "403 Forbidden", "text/plain", b"forbidden");
    }
    let file: PathBuf = dir.join(relative);
    match fs::read(&file) {
        Ok(body) => write_response(&mut stream, "200 OK", mime(&file), &body),
        Err(_) => write_response(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn mime(file: &Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn write_response(stream: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}
