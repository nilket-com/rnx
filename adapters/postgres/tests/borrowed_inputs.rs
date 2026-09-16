use std::process::Command;

fn eval(source: &str) -> String {
	let output = Command::new(env!("CARGO_BIN_EXE_rnx-pg"))
		.args(["--color=never", "eval", source])
		.env_remove("RNX_MEMORY_CEILING")
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());
	String::from_utf8(output.stdout).unwrap()
}

#[test]
fn two_calls_leave_both_string_bindings_available() {
	let output = eval(
		r#"
        let url = "not a connection URL";
        let sql = "SELECT 1";
        let a = postgres::query(url, sql, [], #{}).await;
        let b = postgres::query(url, sql, [], #{}).await;
        [url, sql, a.is_err(), b.is_err()]
    "#,
	);
	assert_eq!(
		output,
		"[\"not a connection URL\", \"SELECT 1\", true, true]\n"
	);
}

#[test]
fn an_unpolled_future_does_not_keep_the_bindings_borrowed() {
	let output = eval(
		r#"
        let url = "old";
        let sql = "SELECT 1";
        let pending = postgres::query(url, sql, [], #{});
        url.push_str(" URL");
        sql.push_str(" AS n");
        [url, sql]
    "#,
	);
	assert_eq!(output, "[\"old URL\", \"SELECT 1 AS n\"]\n");
}
