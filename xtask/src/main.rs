//! Generates `crates/rdna2-isa/src/generated.rs` from AMD's machine-readable
//! RDNA2 spec: `cargo run -p xtask -- path/to/amdgpu_isa_rdna2.xml`.

use std::collections::BTreeMap;
use std::fmt::Write;

use roxmltree::{Document, Node};

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|c| c.has_tag_name(name))
}

fn children<'a>(node: Node<'a, 'a>, name: &'static str) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children().filter(move |c| c.has_tag_name(name))
}

fn text<'a>(node: Node<'a, 'a>, name: &str) -> &'a str {
    child(node, name).and_then(|c| c.text()).unwrap_or("").trim()
}

/// Bit ranges (offset, count), least significant first.
fn ranges(field: Node) -> Vec<(u8, u8)> {
    let mut ranges: Vec<(u32, u8, u8)> = child(field, "BitLayout")
        .into_iter()
        .flat_map(|l| children(l, "Range"))
        .map(|r| {
            let order = r.attribute("Order").unwrap_or("0").parse().unwrap();
            (order, text(r, "BitOffset").parse().unwrap(), text(r, "BitCount").parse().unwrap())
        })
        .collect();
    ranges.sort();
    ranges.into_iter().map(|(_, offset, count)| (offset, count)).collect()
}

/// A binary string from the spec, most significant bit first.
fn bits(s: &str) -> u64 {
    u64::from_str_radix(s.trim(), 2).unwrap()
}

type Operand<'a> = (&'a str, &'a str, u32);

/// One opcode of a base format, gathered over the format and its variants.
struct Opcode<'a> {
    name: String,
    in_literal: bool,
    in_other: bool,
    /// Which encoding the operands come from: 0 the base, 1 a plain
    /// variant (`VOP3_SDST_ENC`), 2 a literal one, 3 DPP/SDWA/NSA.
    rank: u8,
    operands: Vec<Operand<'a>>,
}

/// Explicit operands (field, type, size) in order; the fixed VCC has no field.
fn operands<'a>(enc: Node<'a, 'a>) -> Vec<Operand<'a>> {
    let mut ops: Vec<(u32, Operand)> = child(enc, "Operands")
        .into_iter()
        .flat_map(|o| children(o, "Operand"))
        .filter(|o| o.attribute("IsImplicit") != Some("true"))
        .map(|o| {
            let order = o.attribute("Order").unwrap_or("0").parse().unwrap();
            let ty = text(o, "OperandType").trim_start_matches("OPR_");
            (order, (text(o, "FieldName"), ty, text(o, "OperandSize").parse().unwrap()))
        })
        .collect();
    ops.sort_by_key(|o| o.0);
    ops.into_iter().map(|o| o.1).collect()
}

/// Lowercased names of an operand type's values, filtered by `keep`.
fn names(isa: Node, ty: &str, keep: impl Fn(u32) -> bool) -> String {
    let t = child(isa, "OperandTypes")
        .into_iter()
        .flat_map(|t| children(t, "OperandType"))
        .find(|t| text(*t, "OperandTypeName") == ty)
        .unwrap();
    let mut out = String::new();
    for v in child(t, "OperandPredefinedValues").into_iter().flat_map(|p| children(p, "PredefinedValue")) {
        let value: u32 = text(v, "Value").parse().unwrap();
        if keep(value) {
            let _ = writeln!(out, "    ({value}, {:?}),", text(v, "Name").to_lowercase());
        }
    }
    out
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: xtask <amdgpu_isa_rdna2.xml>");
    let xml = std::fs::read_to_string(&path).unwrap();
    let doc = Document::parse_with_options(&xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).unwrap();
    let isa = doc.descendants().find(|n| n.has_tag_name("ISA")).unwrap();

    // Opcodes per base format. Variants (`VOP2_INST_LITERAL`,
    // `VOP3_SDST_ENC`, `MIMG_NSA1`...) belong to the base format their name
    // starts with.
    let mut opcodes: BTreeMap<String, BTreeMap<u32, Opcode>> = BTreeMap::new();
    for inst in children(child(isa, "Instructions").unwrap(), "Instruction") {
        let name = text(inst, "InstructionName").to_lowercase();
        for enc in inst.descendants().filter(|n| n.has_tag_name("InstructionEncoding")) {
            let encoding = text(enc, "EncodingName");
            let base = match encoding.strip_prefix("ENC_") {
                Some(base) => base,
                None => encoding.split('_').next().unwrap(),
            };
            let literal = encoding.ends_with("_INST_LITERAL");
            let rank = if encoding.starts_with("ENC_") {
                0
            } else if ["SDWA", "DPP", "NSA"].iter().any(|v| encoding.contains(v)) {
                3
            } else if literal {
                2
            } else {
                1
            };
            let opcode = text(enc, "Opcode").parse().unwrap();
            let entry = opcodes.entry(format!("ENC_{base}")).or_default().entry(opcode).or_insert_with(|| Opcode {
                name: name.clone(),
                in_literal: false,
                in_other: false,
                rank: u8::MAX,
                operands: Vec::new(),
            });
            if literal {
                entry.in_literal = true;
            } else {
                entry.in_other = true;
            }
            if rank < entry.rank {
                entry.rank = rank;
                entry.operands = operands(enc);
            }
        }
    }

    let encodings: Vec<Node> = children(child(isa, "Encodings").unwrap(), "Encoding").collect();
    let fields_of = |name: &str| -> Vec<(&str, Vec<(u8, u8)>)> {
        encodings
            .iter()
            .filter(|e| text(**e, "EncodingName") == name)
            .flat_map(|e| e.descendants().filter(|n| n.has_tag_name("Field")))
            .map(|f| (text(f, "FieldName"), ranges(f)))
            .collect()
    };

    let mut out = String::from(
        "// @generated by `cargo run -p xtask`; do not edit.\n\
         // From AMD's machine-readable RDNA2 ISA specification:\n\
         // Copyright (c) 2026 Advanced Micro Devices, Inc., or its affiliates. MIT.\n\n\
         use crate::Spec;\n\npub(crate) static ENCODINGS: &[Spec] = &[\n",
    );
    for enc in &encodings {
        let name = text(*enc, "EncodingName");
        // Base formats only; the variants (literal, DPP, SDWA, NSA) are
        // decoded from fields of these.
        let Some(short) = name.strip_prefix("ENC_") else { continue };
        let mut fields = fields_of(name);
        // VOP3B's SDST sits where VOP3A has ABS and OP_SEL.
        for extra in fields_of(&format!("{short}_SDST_ENC")) {
            if !fields.iter().any(|f| f.0 == extra.0) {
                fields.push(extra);
            }
        }
        // The identifiers list every valid opcode; keep only the format's
        // fixed bits, so the opcode is looked up apart.
        let op_bits = fields
            .iter()
            .filter(|(f, _)| *f == "OP")
            .flat_map(|(_, r)| r)
            .fold(0u64, |m, &(offset, count)| m | ((1u64 << count) - 1) << offset);
        let mask = bits(text(*enc, "EncodingIdentifierMask")) & !op_bits;
        let mut ids: Vec<u64> =
            children(child(*enc, "EncodingIdentifiers").unwrap(), "EncodingIdentifier").map(|n| bits(n.text().unwrap()) & mask).collect();
        ids.sort();
        ids.dedup();
        let ids: Vec<String> = ids.iter().map(|id| format!("{id:#x}")).collect();
        let _ = write!(
            out,
            "    Spec {{\n        name: {:?},\n        bits: {},\n        mask: {mask:#x},\n        ids: &[{}],\n        fields: &[\n",
            short.to_lowercase(),
            text(*enc, "BitCount"),
            ids.join(", "),
        );
        for (field, r) in fields {
            let _ = writeln!(out, "            ({field:?}, &{r:?}),");
        }
        out.push_str("        ],\n        opcodes: &[\n");
        for (opcode, op) in opcodes.get(name).into_iter().flatten() {
            let always_literal = op.in_literal && !op.in_other;
            let _ = writeln!(out, "            ({opcode}, {:?}, {always_literal}, &{:?}),", op.name, op.operands);
        }
        out.push_str("        ],\n    },\n");
    }
    out.push_str("];\n\n/// Source operand values that are not plain SGPRs or VGPRs.\npub(crate) static SRC_NAMES: &[(u32, &str)] = &[\n");
    out.push_str(&names(isa, "OPR_SRC", |v| (106..256).contains(&v)));
    out.push_str("];\n\n/// Export targets.\npub(crate) static TGT_NAMES: &[(u32, &str)] = &[\n");
    out.push_str(&names(isa, "OPR_TGT", |_| true));
    out.push_str("];\n");
    let target = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/rdna2-isa/src/generated.rs");
    std::fs::write(target, out).unwrap();
    println!("wrote {target}");
}
