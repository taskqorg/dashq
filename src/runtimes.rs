//! Agent runtimes and AgentQ clients (dashq.md D4.5) from one data table, `runtimes.json`: the AgentQ runtime
//! descriptor (name, label, brand, family, capabilities) plus the TaskQ codes, login field and model accounting
//! DashQ reads for each runtime. Product code never branches on a runtime or client name (specq agent runtimes).

use serde_json::{Value, json};
use std::sync::OnceLock;

const TABLE: &str = include_str!("runtimes.json");

pub struct Runtime {
    pub name: &'static str,
    pub label: &'static str,
    pub brand: &'static str,
    capabilities: Vec<&'static str>,
    /// TaskQ runtime codes (telemetry, observations, policy targets) that name this runtime.
    pub codes: Vec<&'static str>,
    /// The field of a TaskQ `login` host observation that carries this runtime's sign-in, if any.
    pub login: Option<&'static str>,
    /// Model accounting: `{source}` when qualified, else `{reason}`; null when not listed.
    pub model: &'static Value,
    descriptor: Value,
}

impl Runtime {
    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// An AgentQ client profile (D8.4): its key, display label, producer agent names and Direct routing note.
pub struct Client {
    pub key: &'static str,
    pub label: &'static str,
    agents: Vec<&'static str>,
    pub direct: Option<&'static str>,
    pub model: &'static Value,
}

fn table() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(TABLE).expect("runtime table"))
}

fn strs(v: &'static Value) -> Vec<&'static str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

fn s(v: &'static Value, k: &str) -> &'static str {
    v[k].as_str().unwrap_or_else(|| panic!("runtime table: missing {k}"))
}

pub fn runtimes() -> &'static [Runtime] {
    static V: OnceLock<Vec<Runtime>> = OnceLock::new();
    V.get_or_init(|| {
        strs_rows(&table()["runtimes"])
            .map(|r| Runtime {
                name: s(r, "name"),
                label: s(r, "label"),
                brand: s(r, "brand"),
                capabilities: strs(&r["capabilities"]),
                codes: strs(&r["codes"]),
                login: r["login"].as_str(),
                model: &r["model"],
                descriptor: json!({"name": r["name"], "label": r["label"], "brand": r["brand"], "family": r["family"], "capabilities": r["capabilities"]}),
            })
            .collect()
    })
}

pub fn clients() -> &'static [Client] {
    static V: OnceLock<Vec<Client>> = OnceLock::new();
    V.get_or_init(|| {
        strs_rows(&table()["clients"])
            .map(|c| Client { key: s(c, "key"), label: s(c, "label"), agents: strs(&c["agents"]), direct: c["direct"].as_str(), model: &c["model"] })
            .collect()
    })
}

fn strs_rows(v: &'static Value) -> impl Iterator<Item = &'static Value> {
    v.as_array().into_iter().flatten()
}

/// The runtime a TaskQ runtime code names.
pub fn by_code(code: &str) -> Option<&'static Runtime> {
    runtimes().iter().find(|r| r.codes.contains(&code))
}

/// Every TaskQ runtime code of the table.
pub fn codes() -> impl Iterator<Item = &'static str> {
    runtimes().iter().flat_map(|r| r.codes.iter().copied())
}

/// The client of a client key or producer agent name; other clients (Claude Desktop chat) have none.
pub fn client(agent: &str) -> Option<&'static Client> {
    clients().iter().find(|c| c.key == agent || c.agents.contains(&agent))
}

/// The client of exactly this key (a `/data` agent filter).
pub fn client_key(key: &str) -> Option<&'static Client> {
    clients().iter().find(|c| c.key == key)
}

/// Producer agent names of every client.
pub fn agents() -> impl Iterator<Item = &'static str> {
    clients().iter().flat_map(|c| c.agents.iter().copied())
}

/// `/data.runtimes`: the AgentQ descriptors, in table order.
pub fn descriptors() -> Value {
    Value::Array(runtimes().iter().map(|r| r.descriptor.clone()).collect())
}

/// A session link the runtime's descriptor offers (`session-url`): an https URL of plain characters, else none.
/// The URL stays opaque: no host or path pattern is checked.
pub fn session_url<'a>(code: &str, url: &'a str) -> Option<&'a str> {
    let ok = by_code(code).is_some_and(|r| r.has("session-url"))
        && url.len() <= 300
        && url.strip_prefix("https://").is_some_and(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-~".contains(&b)));
    ok.then_some(url)
}

/// `selftest`: the table parses and every entry carries its required fields.
pub fn check() -> Result<(), String> {
    let t = table();
    let rows = |k: &str| t[k].as_array().filter(|a| !a.is_empty()).ok_or(format!("runtime table: no {k}"));
    for r in rows("runtimes")? {
        if ["name", "label", "brand", "family"].iter().any(|k| r[*k].as_str().is_none_or(str::is_empty)) || r["codes"].as_array().is_none_or(|a| a.is_empty()) {
            return Err("runtime table: incomplete runtime".into());
        }
    }
    for c in rows("clients")? {
        if ["key", "label"].iter().any(|k| c[*k].as_str().is_none_or(str::is_empty)) {
            return Err("runtime table: incomplete client".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Risk: the table loses a descriptor field or a runtime, and links or sign-in columns vanish silently.
    #[test]
    fn runtime_table_matches_agentq_descriptor() {
        check().unwrap();
        assert_eq!(descriptors()[0], json!({"name": "claude", "label": "Claude Code", "brand": "Anthropic", "family": "claude",
            "capabilities": ["turn", "resume", "caller-session-id", "compact-hook", "session-url", "quota-snapshot", "background", "title"]}));
        assert_eq!(runtimes().iter().map(|r| r.name).collect::<Vec<_>>(), ["claude", "codex", "antigravity"]);
        assert_eq!(by_code("codex-app").map(|r| r.name), Some("codex"));
        assert_eq!(client("claude").map(|c| c.key), Some("claude-code"));
        assert!(client("claude-desktop").is_none() && client_key("claude").is_none());
    }

    // Risk (#249 A3): a session link changes per runtime when the claude.ai prefix check became a capability check.
    #[test]
    fn session_links_follow_the_descriptor_capability() {
        assert_eq!(session_url("claude", "https://claude.ai/code/session_ab12"), Some("https://claude.ai/code/session_ab12"));
        assert_eq!(session_url("codex", "codex://threads/local-only"), None, "no session-url capability: no off-host link");
        assert_eq!(session_url("codex-app", "https://example.test/x"), None);
        assert_eq!(session_url("agy", "https://example.test/x"), None);
        assert_eq!(session_url("other", "https://example.test/x"), None, "unknown runtime: no link");
        for bad in ["http://claude.ai/code/session_x", "https://", "javascript:alert(1)", "https://a/b?c=<x>", "https://a b"] {
            assert_eq!(session_url("claude", bad), None, "{bad}");
        }
    }

    // specq agent runtimes check: runtime and client names live only in this table, tests and data, never in product
    // code. Risk: a new `== "codex"` branch or URL pattern returns (#249 A1).
    #[test]
    fn product_code_has_no_runtime_names() {
        let names: Vec<&str> = runtimes().iter().flat_map(|r| std::iter::once(r.name).chain(r.codes.iter().copied())).chain(agents()).chain(clients().iter().map(|c| c.key)).collect();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/");
        let mut files: Vec<String> = std::fs::read_dir(format!("{dir}src")).unwrap().map(|e| e.unwrap().path().display().to_string()).filter(|p| p.ends_with(".rs")).collect();
        let mut dirs = vec![std::path::PathBuf::from(format!("{dir}assets"))];
        while let Some(d) = dirs.pop() {
            for p in std::fs::read_dir(d).unwrap().map(|e| e.unwrap().path()) {
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|e| e == "js") {
                    files.push(p.display().to_string());
                }
            }
        }
        let mut hits = vec![];
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let product = text.split("#[cfg(test)]").next().unwrap();
            for (i, line) in product.lines().enumerate() {
                let code = line.split("//").next().unwrap();
                let quoted = names.iter().any(|n| [format!("\"{n}\""), format!("'{n}'")].iter().any(|q| code.contains(q.as_str())));
                if quoted || code.contains("claude.ai") || code.contains("codex:") {
                    hits.push(format!("{f}:{}: {}", i + 1, line.trim()));
                }
            }
        }
        // Open exception (#252): assets/taskq holds pinned TaskQ release bytes (D10), which DashQ never edits; up to
        // v4.6.65 they still branch on runtime names. #252 repins to a descriptor-reading release and drops this line.
        hits.retain(|h| !h.replace('\\', "/").contains("/assets/taskq/"));
        assert!(hits.is_empty(), "runtime names outside the runtime table:\n{}", hits.join("\n"));
    }
}
