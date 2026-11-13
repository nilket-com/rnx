use super::*;

fn session() -> DomainSession {
	let mut context = Context::with_default_modules().unwrap();
	crate::install_core(&mut context).unwrap();
	DomainSession::new(context, usize::MAX).unwrap()
}

#[test]
#[ignore = "explicit 0268 ten-thousand-input native release-bound control"]
fn ten_thousand_rebindings_keep_one_defining_program() {
	let mut session = session();
	for value in 0..10_000 {
		drop(session.eval(&format!("let f=||{value};0")).unwrap());
		session.collect().unwrap();
		assert_eq!(session.retained_units(), 1);
		assert_eq!(session.maps.len(), 1);
	}
	let value = session.eval("f()").unwrap();
	assert_eq!(
		session.domain.scalar(&value).unwrap(),
		Some(Scalar::I64(9999))
	);
	drop(value);
	session.close().unwrap();
}

#[test]
fn failed_collection_preserves_maps_and_remaining_bindings() {
	let mut session = session();
	drop(session.eval("let f=||7;let v=[1,2,3];0").unwrap());
	// Model dropping the last externally retained handle before idle collection.
	// This leaves a dead candidate beside live ordinary data, as in the domain
	// walk-allocation control. Failed decisions must not prune source maps.
	session.state.remove("f");
	session.names.remove("f");
	let revisions: Vec<_> = session.maps.keys().copied().collect();
	let mut failed = false;
	let mut completed = false;
	for bytes in (0..2048).step_by(16) {
		let result = rune::alloc::limit::with(bytes, || session.collect()).call();
		match result {
			Err(_) => {
				failed = true;
				assert_eq!(session.maps.keys().copied().collect::<Vec<_>>(), revisions);
				assert!(session.binding("v").is_some());
			}
			Ok(()) => {
				completed = true;
				break;
			}
		}
	}
	assert!(failed && completed);
	assert!(session.maps.is_empty());
	let value = session.eval("v[2]").unwrap();
	assert_eq!(session.domain.scalar(&value).unwrap(), Some(Scalar::I64(3)));
	drop(value);
	session.close().unwrap();
}
