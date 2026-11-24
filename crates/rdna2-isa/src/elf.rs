//! The code in an AMDGPU ELF code object (`.o`, `.hsaco`, `.co`).

/// Code to disassemble and the symbols in it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Code<'a> {
    pub bytes: &'a [u8],
    /// (byte offset into `bytes`, name), sorted by offset.
    pub symbols: Vec<(usize, String)>,
}

/// The `.text` section and its symbols when `file` is a 64-bit
/// little-endian ELF; otherwise the whole file as raw code.
pub fn code(file: &[u8]) -> Result<Code<'_>, &'static str> {
    if !file.starts_with(b"\x7fELF") {
        return Ok(Code { bytes: file, symbols: Vec::new() });
    }
    if file.get(4..6) != Some(&[2, 1]) {
        return Err("only 64-bit little-endian ELF files are supported");
    }
    text(file).ok_or("malformed ELF file or no .text section")
}

fn uint(file: &[u8], at: usize, n: usize) -> Option<usize> {
    let bytes = file.get(at..at.checked_add(n)?)?;
    let v = bytes.iter().rev().fold(0u64, |v, &b| v << 8 | u64::from(b));
    usize::try_from(v).ok()
}

/// A section header: (name offset, type, address, file offset, size, link).
fn section(file: &[u8], i: usize) -> Option<(usize, usize, usize, usize, usize, usize)> {
    let at = uint(file, 0x28, 8)?.checked_add(i.checked_mul(uint(file, 0x3A, 2)?)?)?;
    Some((
        uint(file, at, 4)?,
        uint(file, at + 4, 4)?,
        uint(file, at + 0x10, 8)?,
        uint(file, at + 0x18, 8)?,
        uint(file, at + 0x20, 8)?,
        uint(file, at + 0x28, 4)?,
    ))
}

fn data(file: &[u8], offset: usize, size: usize) -> Option<&[u8]> {
    file.get(offset..offset.checked_add(size)?)
}

/// The NUL-terminated string at `at` in a string table.
fn name(table: &[u8], at: usize) -> Option<String> {
    let rest = table.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

fn text(file: &[u8]) -> Option<Code<'_>> {
    let count = uint(file, 0x3C, 2)?;
    let names = section(file, uint(file, 0x3E, 2)?)?;
    let names = data(file, names.3, names.4)?;
    let index = (0..count).find(|&i| section(file, i).and_then(|s| name(names, s.0)).as_deref() == Some(".text"))?;
    let (_, _, address, offset, size, _) = section(file, index)?;
    let bytes = data(file, offset, size)?;

    let mut symbols = Vec::new();
    // SHT_SYMTAB; its link is the string table.
    if let Some(table) = (0..count).filter_map(|i| section(file, i)).find(|s| s.1 == 2) {
        let strings = section(file, table.5).and_then(|s| data(file, s.3, s.4)).unwrap_or(&[]);
        for entry in data(file, table.3, table.4).unwrap_or(&[]).as_chunks::<24>().0 {
            let kind = entry[4] & 0xF;
            // Functions and plain labels in .text; not sections (3) or files (4).
            if uint(entry, 6, 2) != Some(index) || !matches!(kind, 0 | 2) {
                continue;
            }
            let at = uint(entry, 8, 8).and_then(|v| v.checked_sub(address)).filter(|&at| at < size);
            if let (Some(at), Some(n)) = (at, uint(entry, 0, 4).and_then(|n| name(strings, n)))
                && !n.is_empty()
            {
                symbols.push((at, n));
            }
        }
    }
    symbols.sort();
    symbols.dedup();
    Some(Code { bytes, symbols })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ELF64: null, .text (8 bytes at address 0x100), .symtab with
    /// `main` at 0x100 and `loop` at 0x104, .strtab, .shstrtab.
    fn object() -> Vec<u8> {
        let text = [0x01, 0x03, 0x80, 0xBE, 0x00, 0x00, 0x81, 0xBF];
        let strtab = b"\0main\0loop\0";
        let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
        let mut symtab = vec![0u8; 24];
        for (name, value) in [(1u32, 0x100u64), (6, 0x104)] {
            let mut sym = vec![0u8; 24];
            sym[..4].copy_from_slice(&name.to_le_bytes());
            sym[4] = 0x12; // global function
            sym[6..8].copy_from_slice(&1u16.to_le_bytes());
            sym[8..16].copy_from_slice(&value.to_le_bytes());
            symtab.extend(sym);
        }
        let mut file = vec![0u8; 64];
        file[..6].copy_from_slice(b"\x7fELF\x02\x01");
        let place = |file: &mut Vec<u8>, bytes: &[u8]| {
            let at = file.len();
            file.extend_from_slice(bytes);
            at
        };
        let parts = [
            place(&mut file, &text),
            place(&mut file, &symtab),
            place(&mut file, strtab),
            place(&mut file, shstrtab),
        ];
        let shoff = file.len();
        // (name, type, address, offset, size, link)
        let headers = [
            (0, 0, 0, 0, 0, 0),
            (1, 1, 0x100, parts[0], text.len(), 0),
            (7, 2, 0, parts[1], symtab.len(), 3),
            (15, 3, 0, parts[2], strtab.len(), 0),
            (23, 3, 0, parts[3], shstrtab.len(), 0),
        ];
        for (name, kind, address, offset, size, link) in headers {
            let mut h = vec![0u8; 64];
            h[..4].copy_from_slice(&(name as u32).to_le_bytes());
            h[4..8].copy_from_slice(&(kind as u32).to_le_bytes());
            h[0x10..0x18].copy_from_slice(&(address as u64).to_le_bytes());
            h[0x18..0x20].copy_from_slice(&(offset as u64).to_le_bytes());
            h[0x20..0x28].copy_from_slice(&(size as u64).to_le_bytes());
            h[0x28..0x2C].copy_from_slice(&(link as u32).to_le_bytes());
            file.extend(h);
        }
        file[0x28..0x30].copy_from_slice(&(shoff as u64).to_le_bytes());
        file[0x3A..0x3C].copy_from_slice(&64u16.to_le_bytes());
        file[0x3C..0x3E].copy_from_slice(&5u16.to_le_bytes());
        file[0x3E..0x40].copy_from_slice(&4u16.to_le_bytes());
        file
    }

    #[test]
    fn reads_text_and_symbols() {
        let file = object();
        let code = code(&file).unwrap();
        assert_eq!(code.bytes, [0x01, 0x03, 0x80, 0xBE, 0x00, 0x00, 0x81, 0xBF]);
        assert_eq!(code.symbols, [(0, "main".to_string()), (4, "loop".to_string())]);
    }

    #[test]
    fn raw_code_passes_through_and_bad_elf_is_an_error() {
        assert_eq!(code(&[1, 2, 3, 4]).unwrap().bytes, [1, 2, 3, 4]);
        let mut file = object();
        file.truncate(100);
        assert!(code(&file).is_err());
        // Every truncation fails cleanly instead of panicking.
        let file = object();
        for n in 0..file.len() {
            let _ = code(&file[..n]);
        }
    }
}
