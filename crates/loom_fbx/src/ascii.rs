//! FBX ASCII encoding of the same record tree the binary writer uses:
//! `Name: props {` … `}` blocks, arrays as `*N { a: … }`, object names as
//! `Class::Name`, embedded bytes as base64.

use crate::binary::{Prop, Record};
use std::fmt::Write;

/// A whole ASCII file of `version` (7400 → "7.4.0").
pub fn encode(top: &[Record], version: u32) -> Vec<u8> {
    let mut out = format!("; FBX {}.{}.0 project file\n; ----------------------------------------------------\n\n", version / 1000, version % 1000 / 100);
    for r in top {
        // The binary file id has no ASCII form.
        if r.name == "FileId" {
            continue;
        }
        record(&mut out, r, 0);
    }
    out.into_bytes()
}

fn record(out: &mut String, r: &Record, depth: usize) {
    let tab = "\t".repeat(depth);
    let _ = write!(out, "{tab}{}: ", r.name);
    let mut array = false;
    for (i, p) in r.props.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        array |= prop(out, p, depth);
    }
    if r.children.is_empty() {
        if !array && r.props.is_empty() {
            out.push_str(" {\n");
            let _ = writeln!(out, "{tab}}}");
        } else {
            out.push('\n');
        }
        return;
    }
    out.push_str(" {\n");
    for c in &r.children {
        record(out, c, depth + 1);
    }
    let _ = writeln!(out, "{tab}}}");
}

fn string(s: &str) -> String {
    // "Name\0\x01Class" → "Class::Name".
    let s = match s.split_once("\0\u{1}") {
        Some((name, class)) => format!("{class}::{name}"),
        None => s.to_owned(),
    };
    format!("\"{}\"", s.replace('"', "&quot;"))
}

fn list<T: std::fmt::Display>(out: &mut String, v: &[T], depth: usize) {
    let tab = "\t".repeat(depth);
    let _ = write!(out, "*{} {{\n{tab}\ta: ", v.len());
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{x}");
    }
    let _ = write!(out, "\n{tab}}}");
}

/// Writes one property; true when it was an array (which closes its own block).
fn prop(out: &mut String, p: &Prop, depth: usize) -> bool {
    match p {
        Prop::I16(v) => {
            let _ = write!(out, "{v}");
        }
        Prop::Bool(v) => out.push_str(if *v { "Y" } else { "0" }),
        Prop::I32(v) => {
            let _ = write!(out, "{v}");
        }
        Prop::I64(v) => {
            let _ = write!(out, "{v}");
        }
        Prop::F32(v) => {
            let _ = write!(out, "{v}");
        }
        Prop::F64(v) => {
            let _ = write!(out, "{v}");
        }
        Prop::Str(s) => out.push_str(&string(s)),
        Prop::Raw(b) => {
            let _ = write!(out, ", \"{}\"", base64(b));
        }
        Prop::ArrF32(v) => {
            list(out, v, depth);
            return true;
        }
        Prop::ArrF64(v) => {
            list(out, v, depth);
            return true;
        }
        Prop::ArrI32(v) => {
            list(out, v, depth);
            return true;
        }
        Prop::ArrI64(v) => {
            list(out, v, depth);
            return true;
        }
        Prop::ArrBool(v) => {
            let ints: Vec<u8> = v.iter().map(|b| *b as u8).collect();
            list(out, &ints, depth);
            return true;
        }
    }
    false
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn records_arrays_and_object_names() {
        let r = Record::with("Model", vec![Prop::I64(5), Prop::Str("Box\0\u{1}Model".into()), Prop::Str("Mesh".into())])
            .child(Record::leaf("Version", vec![Prop::I32(232)]))
            .child(Record::leaf("Vertices", vec![Prop::ArrF64(vec![0.5, -1.0])]));
        let text = String::from_utf8(encode(&[r], 7400)).unwrap();
        assert!(text.starts_with("; FBX 7.4.0 project file"), "{text}");
        assert!(text.contains("Model: 5, \"Model::Box\", \"Mesh\" {\n\tVersion: 232\n\tVertices: *2 {\n\t\ta: 0.5,-1\n\t}\n}\n"), "{text}");
    }
}
