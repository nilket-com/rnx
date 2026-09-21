//! A small parser over the inventory's canonical type strings.
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Path { path: String, args: Vec<Ty> },
    Ref { mutable: bool, inner: Box<Ty> },
    Slice(Box<Ty>),
    Tuple(Vec<Ty>),
    Impl(Vec<Bound>),
    Generic(String),
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub path: String,
    pub args: Vec<Ty>,
    /// `Item = X` on `IntoIterator`
    pub item: Option<Box<Ty>>,
}

pub fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if ch == ',' && depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

pub fn parse(s: &str) -> Ty {
    let s = s.trim();
    if let Some(r) = s.strip_prefix("&'static ") {
        return Ty::Other(format!("&'static {r}"));
    }
    if let Some(r) = s.strip_prefix("&mut ") {
        return Ty::Ref { mutable: true, inner: Box::new(parse(r)) };
    }
    if let Some(r) = s.strip_prefix('&') {
        return Ty::Ref { mutable: false, inner: Box::new(parse(r)) };
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if let Some((t, _len)) = inner.rsplit_once("; ") {
            return Ty::Other(format!("[{}; ..]", parse(t).render()));
        }
        return Ty::Slice(Box::new(parse(inner)));
    }
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        if inner.is_empty() {
            return Ty::Tuple(vec![]);
        }
        return Ty::Tuple(split_top(inner).iter().map(|p| parse(p)).collect());
    }
    if let Some(b) = s.strip_prefix("impl ") {
        return Ty::Impl(b.split(" + ").map(parse_bound).collect());
    }
    if s.starts_with("dyn ") || s.starts_with("fn(") || s.starts_with('?') {
        return Ty::Other(s.to_string());
    }
    if let Some(i) = s.find('<') {
        if s.ends_with('>') {
            let path = &s[..i];
            if is_path(path) {
                return Ty::Path { path: path.to_string(), args: split_top(&s[i + 1..s.len() - 1]).iter().map(|a| parse(a)).collect() };
            }
        }
    }
    if is_path(s) {
        if !s.contains("::") && s.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) && s.len() <= 2 {
            return Ty::Generic(s.to_string());
        }
        if !s.contains("::") && !PRIMS.contains(&s) {
            return Ty::Generic(s.to_string());
        }
        return Ty::Path { path: s.to_string(), args: vec![] };
    }
    Ty::Other(s.to_string())
}

const PRIMS: &[&str] = &["bool", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize", "f32", "f64", "char", "str"];

fn is_path(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':') && !s.contains(' ')
}

fn parse_bound(b: &str) -> Bound {
    let b = b.trim();
    if let Some(i) = b.find('<') {
        if b.ends_with('>') {
            let path = b[..i].to_string();
            let inner = &b[i + 1..b.len() - 1];
            let mut args = Vec::new();
            let mut item = None;
            for a in split_top(inner) {
                if let Some(rest) = a.strip_prefix("Item = ") {
                    item = Some(Box::new(parse(rest)));
                } else {
                    args.push(parse(&a));
                }
            }
            return Bound { path, args, item };
        }
    }
    Bound { path: b.to_string(), args: vec![], item: None }
}

impl Ty {
    pub fn render(&self) -> String {
        match self {
            Ty::Path { path, args } => {
                if args.is_empty() { path.clone() } else { format!("{}<{}>", path, args.iter().map(|a| a.render()).collect::<Vec<_>>().join(", ")) }
            }
            Ty::Ref { mutable, inner } => format!("&{}{}", if *mutable { "mut " } else { "" }, inner.render()),
            Ty::Slice(i) => format!("[{}]", i.render()),
            Ty::Tuple(ts) => format!("({})", ts.iter().map(|t| t.render()).collect::<Vec<_>>().join(", ")),
            Ty::Impl(bs) => format!("impl {}", bs.iter().map(|b| b.path.clone()).collect::<Vec<_>>().join(" + ")),
            Ty::Generic(g) => g.clone(),
            Ty::Other(s) => s.clone(),
        }
    }
}

pub fn last(p: &str) -> &str {
    p.rsplit("::").next().unwrap_or(p)
}
