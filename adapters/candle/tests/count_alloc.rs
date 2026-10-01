//! Record 0136: `enc.count_tokens(texts)` refuses a list over `MAX_TEXTS`
//! at the Rune boundary before it builds a table per text, observed through
//! the allocator's peak (review round 1: it had built two tables of the
//! whole list first). One test, so no parallel test moves the peak.
use candle_core::{DType, Device};
use candle_nn::{VarBuilder, VarMap};
use candle_transformers::models::bert::{BertModel, Config};
use rnx::allocation::{peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A list that large, refused, may cost a few small allocations only.
const SMALL: usize = 64 << 10;

fn write(d: &Path, name: &str, v: &serde_json::Value) {
	let p = d.join(name);
	std::fs::create_dir_all(p.parent().unwrap()).unwrap();
	std::fs::write(p, v.to_string()).unwrap();
}

/// The smallest model the loader admits: a whole-word tokenizer.
fn model() -> PathBuf {
	let d = std::env::temp_dir().join(format!("rnx-0136-count-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&d);
	std::fs::create_dir_all(&d).unwrap();
	let config = json!({
		"model_type": "bert", "hidden_act": "gelu", "position_embedding_type": "absolute",
		"vocab_size": 8, "hidden_size": 8, "num_hidden_layers": 1, "num_attention_heads": 2,
		"intermediate_size": 16, "hidden_dropout_prob": 0.0, "max_position_embeddings": 32,
		"type_vocab_size": 2, "initializer_range": 0.02, "layer_norm_eps": 1e-12, "pad_token_id": 0
	});
	write(&d, "config.json", &config);
	write(
		&d,
		"modules.json",
		&json!([
			{"idx": 0, "name": "0", "path": "", "type": "sentence_transformers.models.Transformer"},
			{"idx": 1, "name": "1", "path": "1_Pooling", "type": "sentence_transformers.models.Pooling"},
			{"idx": 2, "name": "2", "path": "2_Normalize", "type": "sentence_transformers.models.Normalize"}
		]),
	);
	write(
		&d,
		"1_Pooling/config.json",
		&json!({"word_embedding_dimension": 8, "pooling_mode_mean_tokens": true}),
	);
	write(
		&d,
		"sentence_bert_config.json",
		&json!({"max_seq_length": 16, "do_lower_case": false}),
	);
	write(
		&d,
		"tokenizer.json",
		&json!({
			"version": "1.0", "truncation": null, "padding": null, "added_tokens": [],
			"normalizer": null, "pre_tokenizer": {"type": "WhitespaceSplit"},
			"post_processor": {"type": "BertProcessing", "sep": ["[SEP]", 3], "cls": ["[CLS]", 2]},
			"decoder": null,
			"model": {"type": "WordLevel", "unk_token": "[UNK]",
				"vocab": {"[PAD]": 0, "[UNK]": 1, "[CLS]": 2, "[SEP]": 3, "a": 4}}
		}),
	);
	let varmap = VarMap::new();
	let typed: Config = serde_json::from_value(config).unwrap();
	BertModel::load(
		VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu),
		&typed,
	)
	.unwrap();
	varmap.save(d.join("model.safetensors")).unwrap();
	d
}

fn vm() -> Vm {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = r#"
		pub fn load(d) { candle::TextEncoder::load(d) }
		pub fn many(n) { let s = "a"; let xs = []; for _ in 0..n { xs.push(s); } xs }
		pub fn count(enc, xs) { enc.count_tokens(xs) }
	"#;
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn result(v: Value) -> Result<Value, String> {
	rune::from_value::<Result<Value, Value>>(v)
		.unwrap()
		.map_err(|e| rune::from_value::<String>(e).unwrap())
}

#[test]
fn an_oversized_list_is_refused_before_any_table_is_built() {
	let d = model();
	let mut vm = vm();
	let enc = result(vm.call(["load"], (d.to_str().unwrap(),)).unwrap()).unwrap();
	// within the limit, it counts (and warms the method)
	let few = vm.call(["many"], (3i64,)).unwrap();
	let counts = result(vm.call(["count"], (enc.clone(), few)).unwrap()).unwrap();
	assert_eq!(rune::from_value::<Vec<i64>>(counts).unwrap(), [3, 3, 3]);
	// 131,072 shared strings, four times the limit
	let xs = vm.call(["many"], (131_072i64,)).unwrap();
	reset_peak();
	let base = peak();
	let out = result(vm.call(["count"], (enc.clone(), xs)).unwrap());
	let used = peak() - base;
	let e = out.unwrap_err();
	assert!(e.contains("131072 texts, want 1 to 32768"), "{e}");
	assert!(used < SMALL, "the refusal allocated {used} bytes");
	// an empty list is refused the same way
	let none = vm.call(["many"], (0i64,)).unwrap();
	let e = result(vm.call(["count"], (enc, none)).unwrap()).unwrap_err();
	assert!(e.contains("0 texts"), "{e}");
	let _ = std::fs::remove_dir_all(&d);
}
