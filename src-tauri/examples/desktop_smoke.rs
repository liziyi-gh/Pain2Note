//! Real WKWebView + production IPC + temporary SQLite smoke test on macOS.
//! Build the frontend first, then run with --release --features custom-protocol.
use std::time::Duration;
use tauri::Listener;

fn main() {
    let directory = tempfile::tempdir().expect("temporary smoke database");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/gg-six-max.txt")
        .canonicalize()
        .expect("sample hands");
    let script = include_str!("../tests/desktop-smoke.js");
    let mut context = pain2note_lib::context();
    context.config_mut().app.windows[0].incognito = true;
    context.config_mut().app.windows[0].visible = false;
    let app = pain2note_lib::builder(Some(directory.path().join("smoke.sqlite3")))
        // Only replace the OS picker. Tracker commands use production IPC,
        // and both the database and WebView storage are isolated.
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("dialog")
                .invoke_handler(move |invoke| {
                    if invoke.message.command() == "open" {
                        invoke.resolver.resolve(vec![fixture.clone()]);
                        true
                    } else {
                        false
                    }
                })
                .build(),
        )
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                webview.eval(script).expect("inject smoke driver");
            }
        })
        .build(context)
        .expect("build desktop smoke application");
    let handle = app.handle().clone();
    app.listen("desktop-smoke-result", move |event| {
        let result: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        println!("Desktop smoke: {result}");
        handle.exit(if result["ok"] == true { 0 } else { 1 });
    });
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        eprintln!("Desktop smoke timed out");
        handle.exit(1);
    });
    app.run(|_, _| {});
}
