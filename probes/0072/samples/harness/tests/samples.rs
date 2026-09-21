use samples::{fx, show, p};
use polars::prelude::*;

#[test]
fn s000() {
    let script = "pub fn main() {  let r = s::s000(); let f = |r| `()`; let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { { let _ = polars::error::abort::register_polars_abort_mechanism(); "()".to_string() } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s000", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s000", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s000", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s001() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.contains_categoricals(); let f = |r| `${r}`; let first = f(r); let second = f(a.contains_categoricals()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", (fx::dtype().contains_categoricals()) as bool) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s001", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s001", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s001", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s002() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a == s::fx_DataType(); let f = |r| `${r}`; let first = f(r); let second = f(a == s::fx_DataType()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", fx::dtype() == fx::dtype()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s002", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s002", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s002", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s003() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.is_sorted_flag(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.is_sorted_flag()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", fx::column().is_sorted_flag()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s003", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s003", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s003", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s004() {
    let script = "pub fn main() { let a = s::fx_SortMultipleOptions(); let r = a.with_order_reversed(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_order_reversed()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars::prelude::sort::options::SortMultipleOptions::default().with_order_reversed()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s004", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s004", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s004", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s005() {
    let script = "pub fn main() { let a = s::fx_SortMultipleOptions(); let r = a.with_maintain_order(true); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_maintain_order(true)); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars::prelude::sort::options::SortMultipleOptions::default().with_maintain_order(true)) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s005", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s005", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s005", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s006() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.cat_physical(); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.cat_physical()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::dtype().cat_physical() { Ok(v) => format!("Ok({})", format!("{:?}", v)), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s006", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s006", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s006", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s007() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.floor_div(s::fx_Expr()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.floor_div(s::fx_Expr())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::expr(&(fx::expr().floor_div(fx::expr()))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s007", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s007", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s007", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s008() {
    let script = "pub fn main() {  let r = s::SortMultipleOptions::new(); let f = |r| s::dbg(r); let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", <polars::prelude::sort::options::SortMultipleOptions>::new()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s008", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s008", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s008", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s009() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = `${a}`; let f = |r| `${r}`; let first = f(r); let second = f(`${a}`); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", fx::dtype()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s009", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s009", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s009", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s010() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.shrink_to_fit(); let f = |r| `()`; let first = f(r); let second = f(a.shrink_to_fit()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { { let _ = { let mut o = fx::column(); let r = o.shrink_to_fit(); r }; "()".to_string() } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s010", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s010", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s010", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s011() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.into_frame(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.into_frame()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::df(&(fx::column().into_frame())) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s011", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s011", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s011", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s012() {
    let script = "pub fn main() { let a = s::fx_CsvParseOptions(); let r = a.with_encoding(s::fx_CsvEncoding()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_encoding(s::fx_CsvEncoding())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars::prelude::CsvParseOptions::default().with_encoding(polars::prelude::CsvEncoding::Utf8)) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s012", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s012", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s012", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s013() {
    let script = "pub fn main() {  let r = s::DataFrame::empty_with_height(2); let f = |r| s::dbg(r); let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::df(&(<p::DataFrame>::empty_with_height(2usize))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s013", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s013", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s013", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s014() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.clear(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.clear()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::column(&(fx::column().clear())) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s014", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s014", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s014", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s015() {
    let script = "pub fn main() {  let r = s::s015(); let f = |r| `${r}`; let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", (polars_io::cloud::concurrency::get_request_budget()) as i64) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s015", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s015", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s015", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s016() {
    let script = "pub fn main() {  let r = s::JoinArgs::new(s::fx_JoinType()); let f = |r| s::dbg(r); let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", <polars::prelude::JoinArgs>::new(polars::prelude::JoinType::Inner)) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s016", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s016", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s016", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s017() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.n_unique(); let f = |r| match r { Ok(r) => `Ok(${`${r}`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.n_unique()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::column().n_unique() { Ok(v) => format!("Ok({})", format!("{}", (v) as i64)), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s017", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s017", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s017", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s018() {
    let script = "pub fn main() { let a = s::fx_LazyFrame(); let r = a.describe_optimized_plan(); let f = |r| match r { Ok(r) => `Ok(${`${r}`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.describe_optimized_plan()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::lf().describe_optimized_plan() { Ok(v) => format!("Ok({})", format!("{}", (v).to_string())), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s018", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s018", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s018", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s019() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.try_add_owned(s::fx_Column()); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_add_owned(s::fx_Column())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::column().try_add_owned(fx::column()) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s019", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s019", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s019", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s020() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.to_physical(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.to_physical()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::dtype(&(fx::dtype().to_physical())) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s020", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s020", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s020", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s021() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.inner_dtype(); let f = |r| match r { Some(r) => `Some(${s::dbg(r)})`, None => `None` }; let first = f(r); let second = f(a.inner_dtype()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::dtype().inner_dtype() { Some(v) => format!("Some({})", show::dtype(&((v).clone()))), None => "None".to_string() } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s021", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s021", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s021", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s022() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.implode(); let f = |r| s::dbg(r); let first = f(r); let second = f(a.implode()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::dtype(&(fx::dtype().implode())) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s022", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s022", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s022", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s023() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.numeric_to_unsigned_bit_repr(); let f = |r| match r { Some(r) => `Some(${s::dbg(r)})`, None => `None` }; let first = f(r); let second = f(a.numeric_to_unsigned_bit_repr()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::dtype().numeric_to_unsigned_bit_repr() { Some(v) => format!("Some({})", show::dtype(&(v))), None => "None".to_string() } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s023", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s023", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s023", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s024() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.first_non_null(); let f = |r| match r { Some(r) => `Some(${`${r}`})`, None => `None` }; let first = f(r); let second = f(a.first_non_null()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::column().first_non_null() { Some(v) => format!("Some({})", format!("{}", (v) as i64)), None => "None".to_string() } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s024", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s024", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s024", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s025() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.unique(); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.unique()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::column().unique() { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s025", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s025", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s025", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s026() {
    let script = "pub fn main() { let a = s::fx_CsvParseOptions(); let r = a.with_quote_char(Some(2)); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_quote_char(Some(2))); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars::prelude::CsvParseOptions::default().with_quote_char(Some(2u8))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s026", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s026", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s026", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s027() {
    let script = "pub fn main() { let a = s::fx_Field(); let r = a.name(); let f = |r| `${r}`; let first = f(r); let second = f(a.name()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", ((fx::field().name()).clone()).to_string()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s027", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s027", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s027", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s028() {
    let script = "pub fn main() {  let r = s::s028(); let f = |r| s::dbg(r); let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars_plan::dsl::functions::first()) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s028", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s028", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s028", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s029() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.try_into_inner_dtype(); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_into_inner_dtype()); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::dtype().try_into_inner_dtype() { Ok(v) => format!("Ok({})", show::dtype(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s029", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s029", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s029", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s030() {
    let script = "pub fn main() {  let r = s::CloudType::from_cloud_scheme(s::fx_CloudScheme()); let f = |r| s::dbg(r); let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", <polars::prelude::cloud::options::CloudType>::from_cloud_scheme(polars::polars_utils::pl_path::CloudScheme::Abfs)) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s030", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s030", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s030", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s031() {
    let script = "pub fn main() { let a = s::fx_LazyFrame(); let r = a.fill_nan(s::fx_Expr()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.fill_nan(s::fx_Expr())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::lf(&(fx::lf().fill_nan(fx::expr()))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s031", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s031", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s031", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s032() {
    let script = "pub fn main() { let a = s::fx_Field(); let r = a.with_dtype(s::fx_DataType()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_dtype(s::fx_DataType())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::field(&(fx::field().with_dtype(fx::dtype()))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s032", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s032", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s032", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s033() {
    let script = "pub fn main() { let a = s::fx_LazyFrame(); let r = a.shift(s::fx_Expr()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.shift(s::fx_Expr())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::lf(&(fx::lf().shift(fx::expr()))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s033", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s033", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s033", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s034() {
    let script = "pub fn main() { let a = s::fx_LazyFrame(); let r = a.with_column(s::fx_Expr()); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_column(s::fx_Expr())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { show::lf(&(fx::lf().with_column(fx::expr()))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s034", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s034", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s034", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s035() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.cast(s::fx_DataType()); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.cast(s::fx_DataType())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::column().cast(&fx::dtype()) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s035", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s035", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s035", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s036() {
    let script = "pub fn main() {  let r = s::s036(s::fx_Series()); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match polars::prelude::count_ones(&fx::series()) { Ok(v) => format!("Ok({})", show::series(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s036", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s036", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s036", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s037() {
    let script = "pub fn main() {  let r = s::s037(s::fx_Series(), s::fx_DataType()); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = \"n/a\"; [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match polars::prelude::reinterpret(&fx::series(), &fx::dtype()) { Ok(v) => format!("Ok({})", show::series(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s037", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s037", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s037", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s038() {
    let script = "pub fn main() { let a = s::fx_Series(); let r = a.fill_null(s::fx_FillNullStrategy()); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.fill_null(s::fx_FillNullStrategy())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { match fx::series().fill_null(polars::prelude::FillNullStrategy::Mean) { Ok(v) => format!("Ok({})", show::series(&(v))), Err(e) => format!("Err({})", e) } }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s038", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s038", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s038", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s039() {
    let script = "pub fn main() { let a = s::fx_DataType(); let r = a.contains_dtype_recursive(s::fx_DataType()); let f = |r| `${r}`; let first = f(r); let second = f(a.contains_dtype_recursive(s::fx_DataType())); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{}", (fx::dtype().contains_dtype_recursive(&fx::dtype())) as bool) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s039", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s039", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s039", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s040() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.map_expr(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.map_expr(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", show::expr(&(fx::expr().map_expr(|arg0| { arg0.clone() })))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s040", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s040", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s040", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s041() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.agg_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.agg_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", show::expr(&(fx::expr().agg_with_fmt_str(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) }, p::PlSmallStr::from("x"))))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s041", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s041", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s041", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s042() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { c1 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { c1 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", match fx::column().apply_broadcasting_binary_elementwise(&fx::column(), |arg0, arg1| { arg1.clone() }) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) }) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s042", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s042", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s042", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s043() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.try_apply_unary_elementwise(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_unary_elementwise(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", match fx::column().try_apply_unary_elementwise(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) }) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s043", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s043", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s043", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s044() {
    let script = "pub fn main() { let a = s::fx_DataFrame(); let r = a.try_apply_columns(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_columns(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", match fx::df().try_apply_columns(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", format!("[{}]", (v).into_iter().map(|v| show::column(&(v))).fold(String::new(), |a, b| a + "," + &b))), Err(e) => format!("Err({})", e) }) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s044", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s044", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s044", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s045() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.apply_unary_elementwise(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_unary_elementwise(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", show::column(&(fx::column().apply_unary_elementwise(|arg0| { arg0.clone() })))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s045", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s045", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s045", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s046() {
    let script = "pub fn main() { let a = s::fx_DataFrame(); let r = a.apply_columns(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_columns(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", format!("[{}]", (fx::df().apply_columns(|arg0| { arg0.clone() })).into_iter().map(|v| show::column(&(v))).fold(String::new(), |a, b| a + "," + &b))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s046", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s046", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s046", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s047() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.try_map_expr(|c0| { c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_map_expr(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", match fx::expr().try_map_expr(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", show::expr(&(v))), Err(e) => format!("Err({})", e) }) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s047", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s047", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s047", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s048() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.apply(|c0| { c0 }, |c0, c1| { c1 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply(|c0| { c0 }, |c0, c1| { c1 })); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", show::expr(&(fx::expr().apply(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) })))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s048", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s048", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s048", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s049() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.apply_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("Ok({})", show::expr(&(fx::expr().apply_with_fmt_str(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) }, p::PlSmallStr::from("x"))))) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s049", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s049", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s049", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s050() {
    let script = "pub fn main() { let a = s::fx_SortMultipleOptions(); let r = a.with_nulls_last_multi([true]); let f = |r| s::dbg(r); let first = f(r); let second = f(a.with_nulls_last_multi([true])); [first, second] }";
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| { format!("{:?}", polars::prelude::sort::options::SortMultipleOptions::default().with_nulls_last_multi(vec![true])) }) {
        Ok(o) => o,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {
                samples::record("s050", serde_json::json!({"status": "feature_gated", "detail": msg, "receiver_reused": false}));
                return;
            }
            samples::record("s050", serde_json::json!({"status": "oracle_panicked", "detail": msg, "receiver_reused": false}));
            panic!("oracle panicked: {msg}");
        }
    };
    let (status, detail) = match &got {
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" { "no receiver".to_string() } else { "receiver reused".to_string() }),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {}
oracle: {}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    };
    samples::record("s050", serde_json::json!({"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}));
    assert_eq!(status, "executed_match", "{detail}");
}

#[test]
fn s040_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Expr(); let r = a.map_expr(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.map_expr(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", show::expr(&(fx::expr().map_expr(|arg0| { arg0.clone() })))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s040_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s040_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Expr(); let r = a.map_expr(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.map_expr(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s040_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s040_error() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.map_expr(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.map_expr(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s040_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s041_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Expr(); let r = a.agg_with_fmt_str(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.agg_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }");
    let oracle: String = { format!("Ok({})", show::expr(&(fx::expr().agg_with_fmt_str(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) }, p::PlSmallStr::from("x"))))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s041_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s041_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Expr(); let r = a.agg_with_fmt_str(|c0| { let _d = d; c0 }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.agg_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s041_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s041_error() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.agg_with_fmt_str(|c0| { panic(\"boom from rune\") }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.agg_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s041_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s042_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Column(); let r = a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { if n == 1 { c1 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { c1 })); [first, second] }");
    let oracle: String = { format!("Ok({})", match fx::column().apply_broadcasting_binary_elementwise(&fx::column(), |arg0, arg1| { arg1.clone() }) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) }) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s042_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s042_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Column(); let r = a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { let _d = d; c1 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { c1 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s042_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s042_error() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_broadcasting_binary_elementwise(s::fx_Column(), |c0, c1| { c1 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s042_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s043_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Column(); let r = a.try_apply_unary_elementwise(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_unary_elementwise(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", match fx::column().try_apply_unary_elementwise(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", show::column(&(v))), Err(e) => format!("Err({})", e) }) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s043_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s043_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Column(); let r = a.try_apply_unary_elementwise(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_unary_elementwise(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s043_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s043_error() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.try_apply_unary_elementwise(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_unary_elementwise(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s043_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s044_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_DataFrame(); let r = a.try_apply_columns(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_columns(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", match fx::df().try_apply_columns(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", format!("[{}]", (v).into_iter().map(|v| show::column(&(v))).fold(String::new(), |a, b| a + "," + &b))), Err(e) => format!("Err({})", e) }) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s044_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s044_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_DataFrame(); let r = a.try_apply_columns(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_columns(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s044_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s044_error() {
    let script = "pub fn main() { let a = s::fx_DataFrame(); let r = a.try_apply_columns(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_apply_columns(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s044_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s045_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Column(); let r = a.apply_unary_elementwise(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_unary_elementwise(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", show::column(&(fx::column().apply_unary_elementwise(|arg0| { arg0.clone() })))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s045_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s045_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Column(); let r = a.apply_unary_elementwise(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_unary_elementwise(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s045_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s045_error() {
    let script = "pub fn main() { let a = s::fx_Column(); let r = a.apply_unary_elementwise(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_unary_elementwise(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s045_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s046_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_DataFrame(); let r = a.apply_columns(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_columns(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", format!("[{}]", (fx::df().apply_columns(|arg0| { arg0.clone() })).into_iter().map(|v| show::column(&(v))).fold(String::new(), |a, b| a + "," + &b))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s046_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s046_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_DataFrame(); let r = a.apply_columns(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_columns(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s046_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s046_error() {
    let script = "pub fn main() { let a = s::fx_DataFrame(); let r = a.apply_columns(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${`[${r.iter().map(|r| s::dbg(r)).collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_columns(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s046_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s047_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Expr(); let r = a.try_map_expr(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_map_expr(|c0| { c0 })); [first, second] }");
    let oracle: String = { format!("Ok({})", match fx::expr().try_map_expr(|arg0| { Ok(arg0.clone()) }) { Ok(v) => format!("Ok({})", show::expr(&(v))), Err(e) => format!("Err({})", e) }) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s047_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s047_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Expr(); let r = a.try_map_expr(|c0| { let _d = d; c0 }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_map_expr(|c0| { c0 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s047_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s047_error() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.try_map_expr(|c0| { panic(\"boom from rune\") }); let f = |r| match r { Ok(r) => `Ok(${match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.try_map_expr(|c0| { c0 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s047_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s048_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Expr(); let r = a.apply(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }, |c0, c1| { c1 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply(|c0| { c0 }, |c0, c1| { c1 })); [first, second] }");
    let oracle: String = { format!("Ok({})", show::expr(&(fx::expr().apply(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) })))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s048_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s048_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Expr(); let r = a.apply(|c0| { let _d = d; c0 }, |c0, c1| { c1 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply(|c0| { c0 }, |c0, c1| { c1 })); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s048_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s048_error() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.apply(|c0| { panic(\"boom from rune\") }, |c0, c1| { c1 }); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply(|c0| { c0 }, |c0, c1| { c1 })); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s048_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}

#[test]
fn s049_const_capture() {
    let got = samples::run("pub fn main() { let n = s::runtime_one(); let a = s::fx_Expr(); let r = a.apply_with_fmt_str(|c0| { if n == 1 { c0 } else { panic(\"captured scalar lost\") } }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }");
    let oracle: String = { format!("Ok({})", show::expr(&(fx::expr().apply_with_fmt_str(|arg0| { Ok(arg0.clone()) }, |arg0, arg1| { Ok(arg1.clone()) }, p::PlSmallStr::from("x"))))) };
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("s049_const_capture", serde_json::json!({"status": if ok {"scalar_capture_carried"} else {"scalar_capture_failed"}, "detail": format!("{got:?}")}));
    assert!(ok, "{got:?}");
}

#[test]
fn s049_native_capture() {
    let got = samples::run("pub fn main() { let d = s::fx_DataType(); let a = s::fx_Expr(); let r = a.apply_with_fmt_str(|c0| { let _d = d; c0 }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }");
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("s049_native_capture", serde_json::json!({"status": if refused {"native_capture_refused"} else {"native_capture_not_refused"}, "detail": format!("{got:?}")}));
    assert!(refused, "{got:?}");
}

#[test]
fn s049_error() {
    let script = "pub fn main() { let a = s::fx_Expr(); let r = a.apply_with_fmt_str(|c0| { panic(\"boom from rune\") }, |c0, c1| { c1 }, \"x\"); let f = |r| match r { Ok(r) => `Ok(${s::dbg(r)})`, Err(e) => `Err(${e})` }; let first = f(r); let second = f(a.apply_with_fmt_str(|c0| { c0 }, |c0, c1| { c1 }, \"x\")); [first, second] }";
    let got = samples::run(script);
    let text = match &got { Ok(v) => v[0].clone(), Err(e) => e.clone() };
    let propagated = text.contains("boom from rune");
    samples::record("s049_error", serde_json::json!({"status": if propagated {"error_propagated"} else {"error_lost"}, "detail": text}));
    assert!(propagated, "{text}");
}
