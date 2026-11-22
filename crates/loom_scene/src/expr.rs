//! The Expression controller: its expression language (scalar and vector
//! values, operators, functions, `if` / `vif`) and the controller itself
//! (scalar and vector variables, each a constant or another track).

use crate::param::Controller;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// An expression value: a scalar or a three-component vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val {
    S(f32),
    V(Vec3),
}

impl Val {
    fn s(self) -> Option<f32> {
        match self {
            Val::S(x) => Some(x),
            Val::V(_) => None,
        }
    }

    fn v(self) -> Option<Vec3> {
        match self {
            Val::V(x) => Some(x),
            Val::S(_) => None,
        }
    }
}

/// The functions an expression can call (the dialog's Function List).
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("sin(p)", "sine (degrees)"),
    ("cos(p)", "cosine (degrees)"),
    ("tan(p)", "tangent (degrees)"),
    ("asin(p)", "arc sine (degrees)"),
    ("acos(p)", "arc cosine (degrees)"),
    ("atan(p)", "arc tangent (degrees)"),
    ("sinh(p)", "hyperbolic sine"),
    ("cosh(p)", "hyperbolic cosine"),
    ("tanh(p)", "hyperbolic tangent"),
    ("radToDeg(p)", "radians to degrees"),
    ("degToRad(p)", "degrees to radians"),
    ("ceil(p)", "smallest integer >= p"),
    ("floor(p)", "largest integer <= p"),
    ("ln(p)", "natural logarithm"),
    ("log(p)", "base 10 logarithm"),
    ("exp(p)", "e to the power p"),
    ("pow(p,q)", "p to the power q"),
    ("sqrt(p)", "square root"),
    ("abs(p)", "absolute value"),
    ("min(p,q)", "minimum"),
    ("max(p,q)", "maximum"),
    ("mod(p,q)", "remainder of p / q"),
    ("if(p,q,r)", "q if p is nonzero, else r"),
    ("vif(c,V,W)", "V if c is nonzero, else W"),
    ("length(V)", "length of V"),
    ("comp(V,i)", "i'th component (0, 1, 2)"),
    ("unit(V)", "unit vector along V"),
    ("noise(p,q,r)", "3D noise at (p, q, r)"),
];

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f32),
    Id(String),
    Op(&'static str),
}

const OPS: [&str; 17] = ["**", "<=", ">=", "+", "-", "*", "/", "^", ".", "=", "<", ">", "|", "&", "(", ")", ","];

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, usize> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            // An exponent only when digits follow (`2e` is 2 times e).
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            out.push((Tok::Num(src[start..i].parse().map_err(|_| i)?), start));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push((Tok::Id(src[start..i].to_owned()), start));
            continue;
        }
        if c == '[' || c == ']' {
            out.push((Tok::Op(if c == '[' { "[" } else { "]" }), start));
            i += 1;
            continue;
        }
        let op = OPS.iter().find(|o| src[i..].starts_with(**o)).ok_or(i + 1)?;
        out.push((Tok::Op(op), start));
        i += op.len();
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    len: usize,
    vars: &'a dyn Fn(&str) -> Option<Val>,
}

type R = Result<Val, usize>;

impl Parser<'_> {
    /// Where an error is reported: the start of the current token.
    fn here(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1 + 1).unwrap_or(self.len)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Id(s)) if s.eq_ignore_ascii_case(w))
    }

    fn expect(&mut self, op: &str) -> Result<(), usize> {
        if self.is_op(op) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.here())
        }
    }

    fn expr(&mut self) -> R {
        let mut a = self.cmp()?;
        loop {
            let or = if self.is_op("|") || self.is_word("or") {
                true
            } else if self.is_op("&") || self.is_word("and") {
                false
            } else {
                return Ok(a);
            };
            let at = self.here();
            self.pos += 1;
            let b = self.cmp()?;
            let (x, y) = (a.s().ok_or(at)?, b.s().ok_or(at)?);
            a = Val::S(f32::from(if or { x != 0.0 || y != 0.0 } else { x != 0.0 && y != 0.0 }));
        }
    }

    fn cmp(&mut self) -> R {
        let mut a = self.add()?;
        while let Some(Tok::Op(op @ ("=" | "<" | ">" | "<=" | ">="))) = self.peek() {
            let op = *op;
            let at = self.here();
            self.pos += 1;
            let b = self.add()?;
            let (x, y) = (a.s().ok_or(at)?, b.s().ok_or(at)?);
            let r = match op {
                "=" => x == y,
                "<" => x < y,
                ">" => x > y,
                "<=" => x <= y,
                _ => x >= y,
            };
            a = Val::S(f32::from(r));
        }
        Ok(a)
    }

    fn add(&mut self) -> R {
        let mut a = self.mul()?;
        while let Some(Tok::Op(op @ ("+" | "-"))) = self.peek() {
            let minus = *op == "-";
            let at = self.here();
            self.pos += 1;
            let b = self.mul()?;
            a = match (a, b) {
                (Val::S(x), Val::S(y)) => Val::S(if minus { x - y } else { x + y }),
                (Val::V(x), Val::V(y)) => Val::V(if minus { x - y } else { x + y }),
                _ => return Err(at),
            };
        }
        Ok(a)
    }

    fn mul(&mut self) -> R {
        let mut a = self.cross()?;
        while let Some(Tok::Op(op @ ("*" | "/"))) = self.peek() {
            let div = *op == "/";
            let at = self.here();
            self.pos += 1;
            let b = self.cross()?;
            a = match (a, b, div) {
                (Val::S(x), Val::S(y), false) => Val::S(x * y),
                (Val::S(x), Val::S(y), true) => Val::S(x / y),
                (Val::S(x), Val::V(y), false) | (Val::V(y), Val::S(x), false) => Val::V(y * x),
                (Val::V(x), Val::V(y), false) => Val::S(x.dot(y)),
                (Val::V(x), Val::S(y), true) => Val::V(x / y),
                _ => return Err(at),
            };
        }
        Ok(a)
    }

    /// `V X W`: an `X` (or `x`) where an operator belongs is the cross product.
    fn cross(&mut self) -> R {
        let mut a = self.pow()?;
        while matches!(self.peek(), Some(Tok::Id(s)) if s == "X" || s == "x") {
            let at = self.here();
            self.pos += 1;
            let b = self.pow()?;
            a = Val::V(a.v().ok_or(at)?.cross(b.v().ok_or(at)?));
        }
        Ok(a)
    }

    fn pow(&mut self) -> R {
        let a = self.unary()?;
        if self.is_op("^") || self.is_op("**") {
            let at = self.here();
            self.pos += 1;
            let b = self.pow()?;
            return Ok(Val::S(a.s().ok_or(at)?.powf(b.s().ok_or(at)?)));
        }
        Ok(a)
    }

    fn unary(&mut self) -> R {
        if self.is_op("-") || self.is_op("+") {
            let neg = self.is_op("-");
            self.pos += 1;
            let v = self.unary()?;
            return Ok(match (v, neg) {
                (Val::S(x), true) => Val::S(-x),
                (Val::V(x), true) => Val::V(-x),
                (v, false) => v,
            });
        }
        let mut v = self.primary()?;
        while self.is_op(".") {
            let at = self.here();
            self.pos += 1;
            let i = match self.peek() {
                Some(Tok::Id(s)) if s.eq_ignore_ascii_case("x") => 0,
                Some(Tok::Id(s)) if s.eq_ignore_ascii_case("y") => 1,
                Some(Tok::Id(s)) if s.eq_ignore_ascii_case("z") => 2,
                _ => return Err(self.here()),
            };
            self.pos += 1;
            v = Val::S(v.v().ok_or(at)?[i]);
        }
        Ok(v)
    }

    fn args(&mut self) -> Result<Vec<(Val, usize)>, usize> {
        self.expect("(")?;
        let mut out = Vec::new();
        if self.is_op(")") {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            let at = self.here();
            out.push((self.expr()?, at));
            if self.is_op(",") {
                self.pos += 1;
                continue;
            }
            self.expect(")")?;
            return Ok(out);
        }
    }

    /// `if c then a else if d then b else e` (after the condition).
    fn then_else(&mut self, cond: Val, at: usize) -> R {
        if !self.is_word("then") {
            return Err(self.here());
        }
        self.pos += 1;
        let a = self.expr()?;
        if !self.is_word("else") {
            return Err(self.here());
        }
        self.pos += 1;
        let b = self.expr()?;
        Ok(if cond.s().ok_or(at)? != 0.0 { a } else { b })
    }

    fn primary(&mut self) -> R {
        let at = self.here();
        let Some((tok, _)) = self.toks.get(self.pos).cloned() else { return Err(self.len) };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Val::S(n)),
            Tok::Op("(") => {
                let v = self.expr()?;
                self.expect(")")?;
                Ok(v)
            }
            Tok::Op("[") => {
                let mut c = [0.0; 3];
                for (i, slot) in c.iter_mut().enumerate() {
                    let a = self.here();
                    *slot = self.expr()?.s().ok_or(a)?;
                    self.expect(if i < 2 { "," } else { "]" })?;
                }
                Ok(Val::V(Vec3::from(c)))
            }
            Tok::Id(name) if name.eq_ignore_ascii_case("if") => {
                if self.is_op("(") {
                    // if(c, a, b) or if(c then a else b).
                    self.pos += 1;
                    let ca = self.here();
                    let c = self.expr()?;
                    if self.is_word("then") {
                        let v = self.then_else(c, ca)?;
                        self.expect(")")?;
                        return Ok(v);
                    }
                    if self.is_op(")") {
                        self.pos += 1;
                        return self.then_else(c, ca);
                    }
                    self.expect(",")?;
                    let a = self.expr()?;
                    self.expect(",")?;
                    let b = self.expr()?;
                    self.expect(")")?;
                    return Ok(if c.s().ok_or(ca)? != 0.0 { a } else { b });
                }
                let ca = self.here();
                let c = self.expr()?;
                self.then_else(c, ca)
            }
            Tok::Id(name) if self.is_op("(") => {
                let a = self.args()?;
                call(&name, &a).ok_or(at)
            }
            Tok::Id(name) => (self.vars)(&name).or_else(|| predefined(&name)).ok_or(at + name.len() - 1),
            Tok::Op(_) => Err(at),
        }
    }
}

fn predefined(name: &str) -> Option<Val> {
    Some(Val::S(match name {
        "pi" => std::f32::consts::PI,
        "e" => std::f32::consts::E,
        "TPS" => 4800.0,
        _ => return None,
    }))
}

fn call(name: &str, a: &[(Val, usize)]) -> Option<Val> {
    let s = |i: usize| a.get(i).and_then(|x| x.0.s());
    let v = |i: usize| a.get(i).and_then(|x| x.0.v());
    let n = name.to_ascii_lowercase();
    let want = match n.as_str() {
        "pow" | "min" | "max" | "mod" | "comp" => 2,
        "if" | "vif" | "noise" => 3,
        _ => 1,
    };
    if a.len() != want {
        return None;
    }
    let one = |f: fn(f32) -> f32| s(0).map(|x| Val::S(f(x)));
    match n.as_str() {
        "sin" => one(|x| x.to_radians().sin()),
        "cos" => one(|x| x.to_radians().cos()),
        "tan" => one(|x| x.to_radians().tan()),
        "asin" => one(|x| x.asin().to_degrees()),
        "acos" => one(|x| x.acos().to_degrees()),
        "atan" => one(|x| x.atan().to_degrees()),
        "sinh" => one(f32::sinh),
        "cosh" => one(f32::cosh),
        "tanh" => one(f32::tanh),
        "radtodeg" => one(f32::to_degrees),
        "degtorad" => one(f32::to_radians),
        "ceil" => one(f32::ceil),
        "floor" => one(f32::floor),
        "ln" => one(f32::ln),
        "log" => one(f32::log10),
        "exp" => one(f32::exp),
        "sqrt" => one(f32::sqrt),
        "abs" => one(f32::abs),
        "pow" => Some(Val::S(s(0)?.powf(s(1)?))),
        "min" => Some(Val::S(s(0)?.min(s(1)?))),
        "max" => Some(Val::S(s(0)?.max(s(1)?))),
        "mod" => Some(Val::S(s(0)? % s(1)?)),
        "vif" => Some(Val::V(if s(0)? != 0.0 { v(1)? } else { v(2)? })),
        "length" => Some(Val::S(v(0)?.length())),
        "comp" => v(0)?.to_array().get(s(1)?.round() as usize).map(|c| Val::S(*c)),
        "unit" => Some(Val::V(v(0)?.normalize_or_zero())),
        "noise" => Some(Val::S(noise3(s(0)?, s(1)?, s(2)?))),
        _ => None,
    }
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841) ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Smooth 3D value noise in -1..1, the same for the same (p, q, r).
pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let f = |v: f32| {
        let t = v - v.floor();
        t * t * (3.0 - 2.0 * t)
    };
    let (u, v, w) = (f(x), f(y), f(z));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx: i32, dy: i32, dz: i32| hash3(ix + dx, iy + dy, iz + dz);
    lerp(
        lerp(lerp(c(0, 0, 0), c(1, 0, 0), u), lerp(c(0, 1, 0), c(1, 1, 0), u), v),
        lerp(lerp(c(0, 0, 1), c(1, 0, 1), u), lerp(c(0, 1, 1), c(1, 1, 1), u), v),
        w,
    )
}

/// Evaluates `src` with variables from `vars` (then pi, e, TPS). An error
/// is the expression up to the point it failed, as the dialog reports it.
pub fn eval(src: &str, vars: &dyn Fn(&str) -> Option<Val>) -> Result<Val, String> {
    let fail = |at: usize| {
        let mut end = at.min(src.len()).max(1).min(src.len());
        while !src.is_char_boundary(end) {
            end -= 1;
        }
        Err(src[..end].to_owned())
    };
    let toks = match lex(src) {
        Ok(t) => t,
        Err(at) => return fail(at),
    };
    let mut p = Parser { toks, pos: 0, len: src.len(), vars };
    match p.expr() {
        Ok(v) if p.pos == p.toks.len() => Ok(v),
        Ok(_) => fail(p.here()),
        Err(at) => fail(at),
    }
}

/// Where an Expression variable reads its value: a constant, or another
/// track (by node and track pointer) whose controller is copied here and
/// kept current as that track changes (`Scene::sync_expression_sources`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExprSource {
    pub node: u64,
    pub owner: crate::tracks::TrackOwner,
    pub pointer: String,
    /// The track as the dialog lists it ("Box01 \ Position").
    pub label: String,
    #[serde(default)]
    pub copy: Option<SourceCopy>,
    #[serde(skip)]
    pub link: LinkToken,
}

/// The source track's controller (float or vector tracks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceCopy {
    F32(Box<Controller<f32>>),
    Vec3(Box<Controller<Vec3>>),
}

impl SourceCopy {
    pub fn comps_at(&self, t: f32) -> Vec<f32> {
        match self {
            SourceCopy::F32(c) => c.comps_at(t),
            SourceCopy::Vec3(c) => c.comps_at(t),
        }
    }
}

static LINKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts the variables assigned to tracks that exist anywhere, so the
/// per-edit source sync costs nothing in a session without any.
#[derive(Debug)]
pub struct LinkToken(());

impl Default for LinkToken {
    fn default() -> Self {
        LINKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        LinkToken(())
    }
}

impl Clone for LinkToken {
    fn clone(&self) -> Self {
        LinkToken::default()
    }
}

impl Drop for LinkToken {
    fn drop(&mut self) {
        LINKS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl PartialEq for LinkToken {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Whether any Expression variable reads another track.
pub fn any_links() -> bool {
    LINKS.load(std::sync::atomic::Ordering::Relaxed) > 0
}

/// One scalar or vector variable of an Expression controller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExprVar {
    pub name: String,
    pub vector: bool,
    /// Ticks (1/4800 s) added to the time the source is read at.
    #[serde(default)]
    pub tick_offset: i32,
    /// The constant (Assign To Constant); a scalar uses the first component.
    #[serde(default)]
    pub constant: [f32; 3],
    #[serde(default)]
    pub source: Option<ExprSource>,
}

/// The Expression controller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ExprCtrl {
    pub expression: String,
    pub description: String,
    pub vars: Vec<ExprVar>,
}

impl ExprCtrl {
    /// A new controller whose expression is the value `comps`.
    pub fn holding(comps: &[f32]) -> Self {
        let num = |x: f32| {
            let s = format!("{:.4}", x);
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" { "0".to_owned() } else { s.to_owned() }
        };
        let expression = if comps.len() == 3 { format!("[{}, {}, {}]", num(comps[0]), num(comps[1]), num(comps[2])) } else { num(comps.first().copied().unwrap_or(0.0)) };
        Self { expression, ..Default::default() }
    }

    pub fn var(&self, name: &str) -> Option<&ExprVar> {
        self.vars.iter().find(|v| v.name == name)
    }

    /// A variable's value at frame `t`.
    pub fn var_value(&self, v: &ExprVar, t: f32) -> Val {
        let fps = crate::anim::segment().2;
        let c = match &v.source {
            Some(ExprSource { copy: Some(c), .. }) => c.comps_at(t + v.tick_offset as f32 / 4800.0 * fps),
            Some(_) => vec![0.0; 3],
            None => v.constant.to_vec(),
        };
        if v.vector {
            Val::V(Vec3::new(c[0], c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)))
        } else {
            Val::S(c[0])
        }
    }

    /// The predefined time variables at frame `t`: F, NT, S, T.
    pub fn time_vars(t: f32) -> [(&'static str, f32); 4] {
        let (start, end, fps) = crate::anim::segment();
        let secs = (t - start) / fps.max(1e-6);
        [("F", t), ("NT", if end > start { (t - start) / (end - start) } else { 0.0 }), ("S", secs), ("T", secs * 4800.0)]
    }

    /// The expression's value at frame `t`.
    pub fn value(&self, t: f32) -> Result<Val, String> {
        let times = Self::time_vars(t);
        let lookup = |name: &str| self.var(name).map(|v| self.var_value(v, t)).or_else(|| times.iter().find(|x| x.0 == name).map(|x| Val::S(x.1)));
        eval(&self.expression, &lookup)
    }

    /// The value as `dims` components (zero where the expression fails or
    /// returns the other kind of value).
    pub fn eval(&self, t: f32, dims: usize) -> Vec<f32> {
        let clean = |x: f32| if x.is_finite() { x } else { 0.0 };
        match (self.value(t), dims) {
            (Ok(Val::V(v)), 3) => v.to_array().map(clean).to_vec(),
            (Ok(Val::S(x)), 1) => vec![clean(x)],
            _ => vec![0.0; dims],
        }
    }

    /// Whether `src` evaluates to the value kind a `dims`-component track needs.
    pub fn check(&self, src: &str, dims: usize) -> Result<(), String> {
        let probe = ExprCtrl { expression: src.to_owned(), ..self.clone() };
        match probe.value(crate::anim::time())? {
            Val::V(_) if dims == 3 => Ok(()),
            Val::S(_) if dims == 1 => Ok(()),
            Val::V(_) => Err("this track needs a scalar expression".into()),
            Val::S(_) => Err("this track needs a vector expression [x, y, z]".into()),
        }
    }
}

/// Whether `name` can name a new variable: a letter first, then letters,
/// digits or underscores, and not a predefined name.
pub fn valid_name(name: &str) -> bool {
    let mut c = name.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic())
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        && !["pi", "e", "TPS", "F", "NT", "S", "T", "if", "then", "else", "and", "or", "X", "x"].contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Result<Val, String> {
        eval(s, &|n| match n {
            "radius" => Some(Val::S(150.0)),
            "V" => Some(Val::V(Vec3::new(5.0, 6.0, 7.0))),
            _ => None,
        })
    }

    #[test]
    fn operators_functions_and_precedence() {
        assert_eq!(ev("24*6"), Ok(Val::S(144.0)));
        assert_eq!(ev("2+3*4^2"), Ok(Val::S(50.0)));
        assert_eq!(ev("-2^2"), Ok(Val::S(4.0)), "unary minus binds tighter than ^");
        assert_eq!(ev("2**3"), Ok(Val::S(8.0)));
        assert_eq!(ev("(1<2)+(3>=3)+(2=3)"), Ok(Val::S(2.0)));
        assert_eq!(ev("1|0"), Ok(Val::S(1.0)));
        assert_eq!(ev("1&0"), Ok(Val::S(0.0)));
        assert_eq!(ev("V.y"), Ok(Val::S(6.0)));
        assert_eq!(ev("comp([5,6,7],1)"), Ok(Val::S(6.0)));
        assert_eq!(ev("[1,0,0] X [0,1,0]"), Ok(Val::V(Vec3::Z)));
        assert_eq!(ev("[1,2,3]*[4,5,6]"), Ok(Val::S(32.0)), "V*W is the dot product");
        assert_eq!(ev("2*[1,2,3]/2"), Ok(Val::V(Vec3::new(1.0, 2.0, 3.0))));
        assert_eq!(ev("length([3,4,0])"), Ok(Val::S(5.0)));
        assert!(matches!(ev("cos(360*0.25)"), Ok(Val::S(x)) if x.abs() < 1e-6), "trig takes degrees");
        assert_eq!(ev("radius*2"), Ok(Val::S(300.0)));
        assert_eq!(ev("max(3, mod(7, 4))"), Ok(Val::S(3.0)));
        assert_eq!(ev("if(1>2, 5, 6)"), Ok(Val::S(6.0)));
        assert_eq!(ev("if (radius<0) then 0 else if (radius>=0 and radius<=10) then radius*3 else 100"), Ok(Val::S(100.0)));
        assert_eq!(ev("vif(0, [1,1,1], V)"), Ok(Val::V(Vec3::new(5.0, 6.0, 7.0))));
        assert_eq!(ev("noise(1,2,3)"), ev("noise(1,2,3)"));
        assert_eq!(ev("TPS/160"), Ok(Val::S(30.0)));
        assert_eq!(ev("1e-2+1"), Ok(Val::S(1.01)));
    }

    #[test]
    fn errors_show_the_expression_up_to_the_failure() {
        assert_eq!(ev("2 + foo * 3"), Err("2 + foo".into()));
        assert_eq!(ev("[1, 2] + 3"), Err("[1, 2]".into()));
        assert_eq!(ev("1 + [1,2,3]"), Err("1 +".into()));
        assert!(ev("(1 + 2").is_err());
    }

    #[test]
    fn the_circle_example_moves_once_around_per_segment() {
        crate::anim::set_segment(0.0, 100.0, 30.0);
        let mut c = ExprCtrl { expression: "[radius*cos(360*NT), radius*sin(360*NT), 0]".into(), ..Default::default() };
        c.vars.push(ExprVar { name: "radius".into(), vector: false, tick_offset: 0, constant: [150.0, 0.0, 0.0], source: None });
        let at = |t: f32| c.eval(t, 3);
        assert!((at(0.0)[0] - 150.0).abs() < 1e-3);
        assert!((at(25.0)[1] - 150.0).abs() < 1e-3);
        assert!((at(100.0)[0] - 150.0).abs() < 1e-3);
        assert_eq!(c.check("[1,2,3]", 3), Ok(()));
        assert!(c.check("1", 3).is_err(), "a position needs a vector");
    }
}
