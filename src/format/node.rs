//! Read-only value adapters. They expose shape first; children are copied or
//! borrowed only after the shared renderer admits descent.
use super::*;
use rune::experimental_core::{
	error::TypeName,
	heap::{NominalShape, ValueIdentity, ValueView, ViewKind},
	value::{Scalar, Type},
};
use rune::runtime::BorrowRef;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Identity {
	Legacy(usize),
	Execution(ValueIdentity),
}
pub(super) enum Node<'v, 's> {
	Legacy(Value),
	Execution(ValueView<'v, 's>),
}
type NamedChildren<'v, 's> = Vec<(String, usize, Node<'v, 's>)>;

pub(super) enum Shape<'s> {
	Text(&'s str),
	LegacyText(BorrowRef<'s, str>),
	Bytes(Vec<u8>, usize),
	Token(String, Option<Style>),
	Wrapper(&'static str, bool),
	Container {
		identity: Identity,
		total: usize,
		style: Container,
	},
}
pub(super) enum Container {
	List {
		open: &'static str,
		close: &'static str,
		tuple: bool,
	},
	Object,
	TupleStruct(String),
	Struct(String),
}
fn scalar<'a>(value: Scalar) -> Shape<'a> {
	let (text, style) = match value {
		Scalar::Unit => ("()".into(), Style::Number),
		Scalar::Bool(value) => (value.to_string(), Style::Number),
		Scalar::I64(value) => (value.to_string(), Style::Number),
		Scalar::U64(value) => (value.to_string(), Style::Number),
		Scalar::Char(value) => (format!("{value:?}"), Style::Literal),
		Scalar::F64(value) => {
			let mut text = value.to_string();
			if !text.contains('.') && !text.contains('e') && value.is_finite() {
				text.push_str(".0");
			}
			(text, Style::Number)
		}
		Scalar::Ordering(_) => unreachable!("ordering uses opaque presentation"),
	};
	Shape::Token(text, Some(style))
}
fn error(error: rune::experimental_core::error::Error) -> String {
	format!("{}: {:?}", error.operation, error.kind)
}

impl<'v, 's> Node<'v, 's> {
	pub fn shape(&self, limits: &Limits) -> Result<Shape<'_>, String> {
		match self {
			Self::Execution(node) => {
				let kind = node.kind().map_err(error)?;
				let identity = node.identity().map(Identity::Execution);
				Ok(match kind {
					ViewKind::String(text) => Shape::Text(text),
					ViewKind::Scalar(Scalar::Ordering(_))
					| ViewKind::Moved(_)
					| ViewKind::Opaque(_)
					| ViewKind::Type(_) =>
					// Diagnostic names elide absolute item prefixes. Value TypeInfo
					// includes them, also inside built-in generic arguments.
					{
						let name = node.type_name().map_err(error)?;
						let text = match name {
							// Script namespaces are literal identity, including a
							// user's nested module named std. Only built-in names
							// need absolute std item prefixes.
							TypeName::Struct(_) => name.to_string(),
							_ => name.to_string().replace("std::", "::std::"),
						};
						Shape::Token(format!("<{text}>"), None)
					}
					ViewKind::Scalar(value) => scalar(value),
					ViewKind::Function => Shape::Token("<function>".into(), None),
					ViewKind::Option { some: false } => {
						Shape::Token("None".into(), Some(Style::Number))
					}
					ViewKind::Option { some: true } => Shape::Wrapper("Some(", true),
					ViewKind::Result { ok } => {
						Shape::Wrapper(if ok { "Ok(" } else { "Err(" }, false)
					}
					ViewKind::List { tag, len } => Shape::Container {
						identity: identity.expect("list identity"),
						total: len,
						style: Container::List {
							open: if tag == Type::Tuple { "(" } else { "[" },
							close: if tag == Type::Tuple { ")" } else { "]" },
							tuple: tag == Type::Tuple,
						},
					},
					ViewKind::Object { len } => Shape::Container {
						identity: identity.expect("object identity"),
						total: len,
						style: Container::Object,
					},
					ViewKind::Nominal { len } => {
						let info = node.nominal().map_err(error)?.expect("nominal metadata");
						let name = type_name(info.name().to_owned());
						match info.shape {
							NominalShape::Empty => Shape::Token(name, None),
							NominalShape::Tuple(_) => Shape::Container {
								identity: identity.expect("nominal identity"),
								total: len,
								style: Container::TupleStruct(name),
							},
							NominalShape::Named => Shape::Container {
								identity: identity.expect("nominal identity"),
								total: len,
								style: Container::Struct(name),
							},
						}
					}
				})
			}
			Self::Legacy(value) => {
				if let Ok(text) = value.borrow_string_ref() {
					return Ok(Shape::LegacyText(text));
				}
				if let Ok(values) = value.borrow_ref::<RuneVec>() {
					return Ok(Shape::Container {
						identity: Identity::Legacy(&*values as *const RuneVec as usize),
						total: values.len(),
						style: Container::List {
							open: "[",
							close: "]",
							tuple: false,
						},
					});
				}
				if value.borrow_ref::<Function>().is_ok() {
					return Ok(Shape::Token("<function>".into(), None));
				}
				if let Ok(bytes) = value.borrow_ref::<rune::runtime::Bytes>() {
					return Ok(Shape::Bytes(
						bytes.iter().take(limits.string_bytes).copied().collect(),
						bytes.len(),
					));
				}
				if let Ok(option) = value.borrow_ref::<Option<Value>>() {
					return Ok(if option.is_some() {
						Shape::Wrapper("Some(", true)
					} else {
						Shape::Token("None".into(), Some(Style::Number))
					});
				}
				if let Ok(result) = value.borrow_ref::<Result<Value, Value>>() {
					return Ok(Shape::Wrapper(
						if result.is_ok() { "Ok(" } else { "Err(" },
						false,
					));
				}
				let shape = match value.as_type_value() {
					Ok(TypeValue::Unit) => scalar(Scalar::Unit),
					Ok(TypeValue::Tuple(values)) => Shape::Container {
						identity: Identity::Legacy(&*values as *const OwnedTuple as usize),
						total: values.len(),
						style: Container::List {
							open: "(",
							close: ")",
							tuple: true,
						},
					},
					Ok(TypeValue::Object(values)) => Shape::Container {
						identity: Identity::Legacy(&*values as *const Object as usize),
						total: values.len(),
						style: Container::Object,
					},
					Ok(TypeValue::EmptyStruct(value)) => {
						Shape::Token(type_name(value.rtti().item().to_string()), None)
					}
					Ok(TypeValue::TupleStruct(value)) => Shape::Container {
						identity: Identity::Legacy(value.data().as_ptr() as usize),
						total: value.data().len(),
						style: Container::TupleStruct(type_name(value.rtti().item().to_string())),
					},
					Ok(TypeValue::Struct(value)) => Shape::Container {
						identity: Identity::Legacy(value.data().as_ptr() as usize),
						total: value.data().len(),
						style: Container::Struct(type_name(value.rtti().item().to_string())),
					},
					Ok(TypeValue::NotTypedInline(_)) => {
						if let Ok(value) = rune::from_value::<bool>(value.clone()) {
							scalar(Scalar::Bool(value))
						} else if let Ok(value) = rune::from_value::<char>(value.clone()) {
							scalar(Scalar::Char(value))
						} else if let Ok(value) = value.as_integer::<i64>() {
							scalar(Scalar::I64(value))
						} else if let Ok(value) = value.as_integer::<u64>() {
							scalar(Scalar::U64(value))
						} else if let Ok(value) = rune::from_value::<f64>(value.clone()) {
							scalar(Scalar::F64(value))
						} else {
							Shape::Token(format!("<{}>", value.type_info()), None)
						}
					}
					_ => Shape::Token(format!("<{}>", value.type_info()), None),
				};
				Ok(shape)
			}
		}
	}

	pub fn child(&self, index: usize) -> Result<Self, String> {
		match self {
			Self::Execution(node) => node.child(index).map(Self::Execution).map_err(error),
			Self::Legacy(value) => {
				if let Ok(values) = value.borrow_ref::<RuneVec>() {
					return Ok(Self::Legacy(values[index].clone()));
				}
				if let Ok(value) = value.borrow_ref::<Option<Value>>() {
					return Ok(Self::Legacy((*value).as_ref().expect("some child").clone()));
				}
				if let Ok(value) = value.borrow_ref::<Result<Value, Value>>() {
					return Ok(Self::Legacy(match &*value {
						Ok(value) | Err(value) => value.clone(),
					}));
				}
				match value.as_type_value() {
					Ok(TypeValue::Tuple(values)) => Ok(Self::Legacy(values[index].clone())),
					Ok(TypeValue::TupleStruct(value)) => {
						Ok(Self::Legacy(value.data()[index].clone()))
					}
					Ok(TypeValue::Struct(value)) => Ok(Self::Legacy(value.data()[index].clone())),
					_ => Err("value has no indexed children".into()),
				}
			}
		}
	}

	pub fn items(
		&self,
		total: usize,
		limits: &Limits,
		work: &mut Work,
	) -> Result<Vec<Self>, String> {
		let items = (0..total.min(limits.length))
			.map(|index| self.child(index))
			.collect::<Result<Vec<_>, _>>()?;
		if matches!(self,Self::Legacy(value) if value.borrow_ref::<RuneVec>().is_ok()) {
			work.items_copied += items.len();
		}
		Ok(items)
	}

	pub fn entries(
		&self,
		limits: &Limits,
		work: &mut Work,
	) -> Result<Vec<(String, usize, Self)>, String> {
		let mut entries = match self {
			Self::Legacy(value) => {
				let object = value
					.borrow_ref::<Object>()
					.map_err(|_| "object is unavailable")?;
				object
					.iter()
					.take(limits.length)
					.map(|(key, value)| {
						(
							prefix(key, limits.string_bytes).to_owned(),
							key.len(),
							Self::Legacy(value.clone()),
						)
					})
					.collect::<Vec<_>>()
			}
			Self::Execution(node) => {
				let ViewKind::Object { len } = node.kind().map_err(error)? else {
					return Err("value is not an object".into());
				};
				let mut entries = Vec::new();
				for index in 0..len.min(limits.length) {
					let key = node.field_name(index).map_err(error)?;
					entries.push((
						prefix(key, limits.string_bytes).to_owned(),
						key.len(),
						Self::Execution(node.child(index).map_err(error)?),
					));
				}
				entries
			}
		};
		work.entries_examined += entries.len();
		work.key_bytes_copied += entries.iter().map(|(key, _, _)| key.len()).sum::<usize>();
		entries.sort_by(|a, b| a.0.cmp(&b.0));
		Ok(entries)
	}

	pub fn named(
		&self,
		name: &str,
		total: usize,
		fields: Option<&Fields>,
		limits: &Limits,
	) -> Result<Option<NamedChildren<'v, 's>>, String> {
		let Some(fields) = fields else {
			return Ok(None);
		};
		match self {
			Self::Legacy(value) => {
				let Ok(TypeValue::Struct(value)) = value.as_type_value() else {
					return Err("named value is unavailable".into());
				};
				let Some(fields) = fields.fitting(name, total, |key| value.get(key).is_some())
				else {
					return Ok(None);
				};
				Ok(Some(
					fields
						.iter()
						.take(limits.length)
						.map(|key| {
							(
								key.clone(),
								key.len(),
								Self::Legacy(
									value.get(key.as_str()).expect("verified field").clone(),
								),
							)
						})
						.collect(),
				))
			}
			Self::Execution(node) => {
				let mut indices = std::collections::HashMap::with_capacity(total);
				for index in 0..total {
					indices.insert(node.field_name(index).map_err(error)?, index);
				}
				let Some(fields) = fields.fitting(name, total, |key| indices.contains_key(key))
				else {
					return Ok(None);
				};
				let mut entries = Vec::new();
				for key in fields.iter().take(limits.length) {
					entries.push((
						key.clone(),
						key.len(),
						Self::Execution(node.child(indices[key.as_str()]).map_err(error)?),
					));
				}
				Ok(Some(entries))
			}
		}
	}
}
