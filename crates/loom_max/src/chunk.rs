//! The chunk tree of a .max stream: u16 id, u32 length (u64 when the u32 is
//! 0) with the top bit marking a container of further chunks; lengths count
//! the header.

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: u16,
    /// Data bytes (leaf) or the children (container).
    pub data: Vec<u8>,
    pub children: Option<Vec<Chunk>>,
}

impl Chunk {
    pub fn child(&self, id: u16) -> Option<&Chunk> {
        self.children.as_ref()?.iter().find(|c| c.id == id)
    }
    pub fn kids(&self) -> &[Chunk] {
        self.children.as_deref().unwrap_or(&[])
    }
    pub fn u32s(&self) -> Vec<u32> {
        self.data.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }
    pub fn i32s(&self) -> Vec<i32> {
        self.data.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }
    pub fn f32s(&self) -> Vec<f32> {
        self.data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }
    pub fn utf16(&self) -> String {
        let u: Vec<u16> = self.data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    }
}

/// Parses `b` into chunks; `depth` bounds nesting so hostile files cannot overflow the stack.
pub fn parse(b: &[u8], depth: u32) -> Result<Vec<Chunk>, String> {
    if depth > 64 {
        return Err("chunk tree nested too deep".into());
    }
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < b.len() {
        if b.len() - off < 6 {
            return Err("truncated chunk header".into());
        }
        let id = u16::from_le_bytes([b[off], b[off + 1]]);
        let l32 = u32::from_le_bytes(b[off + 2..off + 6].try_into().unwrap());
        let (len, hdr, container) = if l32 == 0 {
            let l = u64::from_le_bytes(b.get(off + 6..off + 14).ok_or("truncated chunk header")?.try_into().unwrap());
            (l & !(1 << 63), 14usize, l >> 63 == 1)
        } else {
            ((l32 & 0x7FFF_FFFF) as u64, 6usize, l32 >> 31 == 1)
        };
        let len = usize::try_from(len).map_err(|_| "chunk too long")?;
        if len < hdr || len > b.len() - off {
            return Err(format!("chunk {id:#06x} overruns its stream"));
        }
        let body = &b[off + hdr..off + len];
        // A container whose body is not a chunk list (some plug-ins set the
        // flag on raw data) is kept as data below the top level.
        let children = if container {
            match parse(body, depth + 1) {
                Ok(k) => Some(k),
                Err(e) if depth == 0 => return Err(e),
                Err(_) => None,
            }
        } else {
            None
        };
        out.push(match children {
            Some(k) => Chunk { id, data: Vec::new(), children: Some(k) },
            None => Chunk { id, data: body.to_vec(), children: None },
        });
        off += len;
    }
    Ok(out)
}
