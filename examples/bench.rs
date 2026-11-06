//! Benchmark behind the landing page's "Server time per redirect" and startup figures.
//!
//! ```sh
//! cargo run --release --example bench
//! ```
//!
//! Builds and starts the release server on a throwaway data dir (never the repo's `.env`), seeds an
//! existing database, then reports what the server itself logs per request (`latency_us` in its debug
//! `request` lines):
//!
//! - From memory: repeat redirects of one cached slug.
//! - From the database: the first redirect of each slug, before it's cached.
//! - Ready after start: launch until `/healthz` answers, on a fresh and on an existing database.
//!
//! Requests are sequential over one keep-alive loopback connection.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

const LINKS: u32 = 100_000;
const MISSES: u32 = 2_000;
const HITS: u32 = 5_000;
const BOOTS: usize = 5;
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = Command::new(cargo)
        .args(["build", "--release", "--quiet", "--bin", "tiiny"])
        .current_dir(&root)
        .status()
        .expect("running cargo");
    assert!(built.success(), "cargo build --release failed");
    let bin = root.join("target/release/tiiny");

    let dir = tempfile::tempdir().expect("temp dir");
    let server = Server::new(&bin, dir.path());

    let first_boot = server.start().stop();
    seed(&dir.path().join("tiiny.db"));
    let mut boots: Vec<f64> = (0..BOOTS).map(|_| server.start().stop()).collect();

    let logs = dir.path().join("logs");
    for f in std::fs::read_dir(&logs).expect("log dir").flatten() {
        let _ = std::fs::remove_file(f.path());
    }
    let running = server.start();
    let mut conn = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let step = LINKS / MISSES;
    for i in 1..=MISSES {
        get(&mut conn, &format!("/b{}", i * step)); // first hit: from the database
    }
    for _ in 0..HITS {
        get(&mut conn, &format!("/b{step}")); // cached by the loop above
    }
    drop(conn);
    sleep(Duration::from_millis(500)); // log writer flush
    running.stop();

    let lat = redirect_latencies(&logs);
    assert_eq!(lat.len() as u32, MISSES + HITS, "redirects logged");
    let (mut db, mut memory) = (
        lat[..MISSES as usize].to_vec(),
        lat[MISSES as usize..].to_vec(),
    );

    println!(
        "{} {}, {LINKS} links seeded, {} redirects, sequential",
        std::env::consts::OS,
        std::env::consts::ARCH,
        lat.len()
    );
    println!(
        "From memory        median {}  p99 {}",
        ms(median(&mut memory)),
        ms(p99(&mut memory))
    );
    println!(
        "From the database  median {}  p99 {}",
        ms(median(&mut db)),
        ms(p99(&mut db))
    );
    println!(
        "Ready after start  {:.2} s existing database (median of {BOOTS}), first boot {first_boot:.2} s",
        median_f(&mut boots)
    );
}

struct Server {
    bin: PathBuf,
    dir: PathBuf,
    port: u16,
}

/// The server process and how long it took to answer /healthz.
struct Running(Child, f64);

impl Server {
    fn new(bin: &Path, dir: &Path) -> Self {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("free port")
            .port();
        Self {
            bin: bin.into(),
            dir: dir.into(),
            port,
        }
    }

    /// Returns once /healthz answers.
    fn start(&self) -> Running {
        let t = Instant::now();
        let mut child = Command::new(&self.bin)
            // Not the repo root: its .env would be loaded.
            .current_dir(&self.dir)
            .env("HOST", "127.0.0.1")
            .env("PORT", self.port.to_string())
            .env("TIINY_DATA_DIR", &self.dir)
            .env("TIINY_LOG", "tiiny=debug")
            .env("TIINY_LOG_FILE", "true")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("starting server");
        loop {
            if let Ok(mut c) = TcpStream::connect(("127.0.0.1", self.port))
                && request(&mut c, "/healthz") == 200
            {
                return Running(child, t.elapsed().as_secs_f64());
            }
            if let Some(code) = child.try_wait().expect("server status") {
                panic!("server exited during startup: {code}");
            }
            sleep(Duration::from_millis(2));
        }
    }
}

impl Running {
    /// Stops the server gracefully (SIGTERM, so its log writer flushes); returns its startup time in seconds.
    fn stop(mut self) -> f64 {
        #[cfg(unix)]
        let _ = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status();
        #[cfg(not(unix))]
        let _ = self.0.kill();
        let _ = self.0.wait();
        self.1
    }
}

fn seed(db: &Path) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let conn = libsql::Builder::new_local(db)
            .build()
            .await
            .expect("open db")
            .connect()
            .expect("connect db");
        conn.execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
             INSERT INTO links (guest_key_hash, slug, url, created_at, updated_at)
             SELECT 'bench', 'b' || i, 'https://example.com/landing/' || i || '?utm_source=newsletter', 1, 1 FROM n",
            [LINKS],
        )
        .await
        .expect("seed links");
    });
}

fn get(conn: &mut TcpStream, path: &str) {
    let status = request(conn, path);
    assert_eq!(status, 302, "GET {path}");
}

/// One keep-alive HTTP/1.1 GET; returns the status and drains the body.
fn request(conn: &mut TcpStream, path: &str) -> u16 {
    write!(
        conn,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUser-Agent: {UA}\r\nReferer: https://news.example.com/post\r\n\r\n"
    )
    .expect("write request");
    let mut r = BufReader::new(conn);
    let mut line = String::new();
    r.read_line(&mut line).expect("status line");
    let status = line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut len = 0;
    loop {
        line.clear();
        r.read_line(&mut line).expect("header");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).expect("body");
    status
}

/// `latency_us` of every 302 the server logged, in request order.
fn redirect_latencies(logs: &Path) -> Vec<u64> {
    let mut files: Vec<_> = std::fs::read_dir(logs)
        .expect("log dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("tiiny."))
        })
        .collect();
    files.sort();
    let mut out = vec![];
    for f in files {
        for line in std::fs::read_to_string(f).expect("read log").lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            // Event fields are flattened into the JSON line.
            if v["status"] == 302
                && let Some(us) = v["latency_us"].as_u64()
            {
                out.push(us);
            }
        }
    }
    out
}

fn median(xs: &mut [u64]) -> u64 {
    xs.sort_unstable();
    xs[xs.len() / 2]
}

fn p99(xs: &mut [u64]) -> u64 {
    xs.sort_unstable();
    xs[(xs.len() * 99 / 100).saturating_sub(1)]
}

fn median_f(xs: &mut [f64]) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

fn ms(us: u64) -> String {
    format!("{:.3} ms", us as f64 / 1000.0)
}
