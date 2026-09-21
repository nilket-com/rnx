//! Render rustdoc types to compact strings for the evidence and for rules.
use rustdoc_types::{GenericArg, GenericArgs, GenericBound, Type};

pub fn ty(t: &Type) -> String {
    match t {
        Type::ResolvedPath(p) => path(&p.path, p.args.as_deref()),
        Type::DynTrait(d) => {
            let b: Vec<String> = d.traits.iter().map(|t| path(&t.trait_.path, t.trait_.args.as_deref())).collect();
            format!("dyn {}", b.join(" + "))
        }
        Type::Generic(g) => g.clone(),
        Type::Primitive(p) => p.clone(),
        Type::FunctionPointer(f) => {
            let ins: Vec<String> = f.sig.inputs.iter().map(|(_, t)| ty(t)).collect();
            match &f.sig.output {
                Some(o) => format!("fn({}) -> {}", ins.join(", "), ty(o)),
                None => format!("fn({})", ins.join(", ")),
            }
        }
        Type::Tuple(ts) => format!("({})", ts.iter().map(ty).collect::<Vec<_>>().join(", ")),
        Type::Slice(s) => format!("[{}]", ty(s)),
        Type::Array { type_, len } => format!("[{}; {}]", ty(type_), len),
        Type::Pat { type_, .. } => ty(type_),
        Type::ImplTrait(bounds) => format!("impl {}", bounds_str(bounds)),
        Type::Infer => "_".into(),
        Type::RawPointer { is_mutable, type_ } => format!("*{} {}", if *is_mutable { "mut" } else { "const" }, ty(type_)),
        Type::BorrowedRef { is_mutable, type_, .. } => format!("&{}{}", if *is_mutable { "mut " } else { "" }, ty(type_)),
        Type::QualifiedPath { name, self_type, trait_, .. } => match trait_ {
            Some(tr) if !tr.path.is_empty() => format!("<{} as {}>::{}", ty(self_type), last(&tr.path), name),
            _ => format!("{}::{}", ty(self_type), name),
        },
    }
}

pub fn bounds_str(bounds: &[GenericBound]) -> String {
    let v: Vec<String> = bounds
        .iter()
        .filter_map(|b| match b {
            GenericBound::TraitBound { trait_, modifier, .. } => {
                let m = match modifier {
                    rustdoc_types::TraitBoundModifier::Maybe => "?",
                    _ => "",
                };
                Some(format!("{}{}", m, path(&trait_.path, trait_.args.as_deref())))
            }
            GenericBound::Outlives(l) => Some(l.clone()),
            GenericBound::Use(_) => None,
        })
        .collect();
    v.join(" + ")
}

pub fn path(p: &str, args: Option<&GenericArgs>) -> String {
    let base = last(p).to_string();
    match args {
        None => base,
        Some(GenericArgs::AngleBracketed { args, constraints }) => {
            let mut parts: Vec<String> = args
                .iter()
                .filter_map(|a| match a {
                    GenericArg::Type(t) => Some(ty(t)),
                    GenericArg::Const(c) => Some(c.expr.clone()),
                    GenericArg::Lifetime(_) => None,
                    GenericArg::Infer => Some("_".into()),
                })
                .collect();
            for c in constraints {
                let rhs = match &c.binding {
                    rustdoc_types::AssocItemConstraintKind::Equality(term) => match term {
                        rustdoc_types::Term::Type(t) => ty(t),
                        rustdoc_types::Term::Constant(c) => c.expr.clone(),
                    },
                    rustdoc_types::AssocItemConstraintKind::Constraint(b) => bounds_str(b),
                };
                parts.push(format!("{} = {}", c.name, rhs));
            }
            if parts.is_empty() { base } else { format!("{}<{}>", base, parts.join(", ")) }
        }
        Some(GenericArgs::Parenthesized { inputs, output }) => {
            let ins: Vec<String> = inputs.iter().map(ty).collect();
            match output {
                Some(o) => format!("{}({}) -> {}", base, ins.join(", "), ty(o)),
                None => format!("{}({})", base, ins.join(", ")),
            }
        }
        Some(GenericArgs::ReturnTypeNotation) => format!("{}(..)", base),
    }
}

pub fn last(p: &str) -> &str {
    p.rsplit("::").next().unwrap_or(p)
}
