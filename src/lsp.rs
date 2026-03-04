use std::collections::HashSet;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde_json::{json, Value};

pub struct LspClient {
    _child: Child,
    stdin: BufWriter<ChildStdin>,
    rx: Receiver<Value>,
    next_id: i64,
    pub initialized: bool,
    opened_uris: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
}

/// Returns LSP language identifier for a file extension.
pub fn language_id(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" | "pyi" => Some("python"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("javascriptreact"),
        "ts" | "mts" => Some("typescript"),
        "tsx" => Some("typescriptreact"),
        "go" => Some("go"),
        _ => None,
    }
}

fn server_command(lang: &str) -> Option<(&'static str, &'static [&'static str])> {
    match lang {
        "rust" => Some(("rust-analyzer", &[])),
        "python" => Some(("pylsp", &[])),
        "c" | "cpp" => Some(("clangd", &[])),
        "javascript" | "javascriptreact" | "typescript" | "typescriptreact" => {
            Some(("typescript-language-server", &["--stdio"]))
        }
        "go" => Some(("gopls", &[])),
        _ => None,
    }
}

pub fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Resolve the actual binary path, bypassing broken rustup shims.
fn resolve_binary(name: &str) -> String {
    // For rust-analyzer the rustup shim may fail; prefer system install
    if name == "rust-analyzer" {
        for candidate in &[
            "/usr/bin/rust-analyzer",
            "/usr/local/bin/rust-analyzer",
        ] {
            if std::path::Path::new(candidate).exists() {
                return candidate.to_string();
            }
        }
        // Rustup toolchain component
        if let Ok(home) = std::env::var("HOME") {
            for tc in &["stable", "nightly"] {
                let p = format!(
                    "{}/.rustup/toolchains/{}-x86_64-unknown-linux-gnu/bin/rust-analyzer",
                    home, tc
                );
                if std::path::Path::new(&p).exists() {
                    return p;
                }
            }
        }
    }
    name.to_string()
}

impl LspClient {
    /// Spawn the language server for `lang` with `root` as workspace.
    /// Returns None if the server binary is not found.
    pub fn start(lang: &str, root: &str) -> Option<Self> {
        let (cmd, args) = server_command(lang)?;
        let cmd = resolve_binary(cmd);

        let mut child = Command::new(&cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let stdin = BufWriter::new(child.stdin.take()?);
        let (tx, rx) = mpsc::channel();

        // Background thread: parse LSP stdout (Content-Length framing)
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut content_length: Option<usize> = None;
                // Read headers
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                        content_length = rest.trim().parse().ok();
                    }
                }
                let len = match content_length {
                    Some(l) if l > 0 => l,
                    _ => continue,
                };
                let mut body = vec![0u8; len];
                use std::io::Read;
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
            }
        });

        let mut client = Self {
            _child: child,
            stdin,
            rx,
            next_id: 1,
            initialized: false,
            opened_uris: HashSet::new(),
        };

        client.send_initialize(root);
        Some(client)
    }

    fn write_msg(&mut self, msg: Value) {
        let body = msg.to_string();
        let _ = write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = self.stdin.flush();
    }

    fn send_initialize(&mut self, root: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.write_msg(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": format!("file://{}", root),
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false },
                        "completion": {
                            "completionItem": { "snippetSupport": false }
                        }
                    }
                }
            }
        }));
    }

    /// Poll pending messages from the server.
    /// Returns completion items if `pending_id` response arrived.
    pub fn poll(&mut self, pending_id: Option<i64>) -> Vec<CompletionItem> {
        let mut completions = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            // Detect initialize response
            if !self.initialized {
                if msg.get("result").is_some() && msg.get("id").is_some() {
                    self.initialized = true;
                    self.write_msg(json!({
                        "jsonrpc": "2.0",
                        "method": "initialized",
                        "params": {}
                    }));
                }
                continue;
            }
            // Check for completion response
            if let Some(pid) = pending_id {
                let matches = msg.get("id")
                    .and_then(|id| id.as_i64())
                    .map(|id| id == pid)
                    .unwrap_or(false);
                if matches {
                    if let Some(result) = msg.get("result") {
                        completions = parse_completions(result);
                    }
                }
            }
        }
        completions
    }

    /// Send textDocument/didOpen if this URI hasn't been opened yet.
    pub fn ensure_open(&mut self, uri: &str, lang: &str, text: &str) {
        if !self.initialized {
            return;
        }
        if self.opened_uris.insert(uri.to_string()) {
            self.write_msg(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": lang,
                        "version": 1,
                        "text": text
                    }
                }
            }));
        }
    }

    pub fn notify_change(&mut self, uri: &str, version: i64, text: &str) {
        if !self.initialized {
            return;
        }
        self.write_msg(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }
        }));
    }

    pub fn request_completion(&mut self, uri: &str, line: u32, character: u32) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write_msg(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        }));
        id
    }
}

fn parse_completions(result: &Value) -> Vec<CompletionItem> {
    let items = if let Some(arr) = result.as_array() {
        arr
    } else if let Some(arr) = result.get("items").and_then(|v| v.as_array()) {
        arr
    } else {
        return Vec::new();
    };

    items
        .iter()
        .take(20)
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let insert_text = item
                .get("insertText")
                .and_then(|v| v.as_str())
                .unwrap_or(&label)
                .to_string();
            Some(CompletionItem { label, insert_text })
        })
        .collect()
}
