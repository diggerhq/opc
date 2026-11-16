//! The test suite's fake Computer, standing alone, to type `opc` commands
//! against by hand:
//!
//!     cargo run --example fake_computer            # prints the exports
//!     eval "$(cat /tmp/opc-fake.env)"              # in another terminal
//!     opc env
//!
//! `OPC_FAKE_STEP_MS` paces the scripted agent (default 120);
//! `OPC_FAKE_NAME` names the person.

#[path = "../tests/support/mod.rs"]
mod support;

fn main() {
    let fake = support::Fake::start();
    let step = std::env::var("OPC_FAKE_STEP_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    fake.sh.knobs.lock().unwrap().step_ms = step;
    let keep = ["OPENCOMPUTER_API_URL", "OPENCOMPUTER_API_KEY", "OPC_WIRE_URL", "OPC_STATE_DIR", "OPENCOMPUTER_CONFIG"];
    let exports: String = fake.env().into_iter().filter(|(k, _)| keep.contains(&k.as_str())).map(|(k, v)| format!("export {k}={v}\n")).collect();
    let file = std::env::temp_dir().join("opc-fake.env");
    std::fs::write(&file, &exports).expect("write the exports");
    print!("{exports}");
    eprintln!("\nfake Computer on {} (exports also in {}); Ctrl+C stops it", fake.url, file.display());
    loop {
        std::thread::park();
    }
}
