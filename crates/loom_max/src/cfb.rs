//! OLE2 / Compound File Binary reader (the container of a .max file),
//! written from the published MS-CFB layout: a header, the FAT and DIFAT,
//! the directory, and the mini stream for small streams. Every offset is
//! bounds-checked; a damaged file is an error, never a panic.

const FREE: u32 = 0xFFFF_FFFA; // ids at or above this end a chain
const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

pub struct Cfb<'a> {
    data: &'a [u8],
    sector: usize,
    mini_sector: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    entries: Vec<(String, u8, u32, u64)>,
}

fn u16_at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "compound file truncated".to_string())
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "compound file truncated".to_string())
}

fn u64_at(d: &[u8], o: usize) -> Result<u64, String> {
    d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| "compound file truncated".to_string())
}

impl<'a> Cfb<'a> {
    pub fn open(data: &'a [u8]) -> Result<Self, String> {
        if data.len() < 512 || data[..8] != SIGNATURE {
            return Err("not a .max file (no compound-file signature)".into());
        }
        let shift = u16_at(data, 30)?;
        let mini_shift = u16_at(data, 32)?;
        if !(9..=16).contains(&shift) || mini_shift > shift {
            return Err("corrupt compound-file header".into());
        }
        let sector = 1usize << shift;
        let fat_sectors = u32_at(data, 44)? as usize;
        let dir_start = u32_at(data, 48)?;
        let mini_cutoff = u32_at(data, 56)? as u64;
        let mini_fat_start = u32_at(data, 60)?;
        let mut difat_next = u32_at(data, 68)?;
        let mut difat: Vec<u32> = (0..109).map(|i| u32_at(data, 76 + i * 4)).collect::<Result<_, _>>()?;
        let sectors = data.len() / sector;
        let mut seen = vec![false; sectors + 1];
        while difat_next < FREE {
            // A sector visited twice is a loop; a sector past the file end is truncation.
            match seen.get_mut(difat_next as usize) {
                Some(v) if !*v => *v = true,
                Some(_) => return Err("corrupt compound file: DIFAT loop".into()),
                None => return Err("compound file truncated".into()),
            }
            let off = (difat_next as usize + 1) * sector;
            let per = sector / 4;
            for i in 0..per - 1 {
                difat.push(u32_at(data, off + i * 4)?);
            }
            difat_next = u32_at(data, off + (per - 1) * 4)?;
        }
        let mut fat = Vec::new();
        if fat_sectors > sectors {
            return Err("corrupt compound-file header: more FAT sectors than the file holds".into());
        }
        for s in difat.iter().take(fat_sectors) {
            if *s >= FREE {
                break;
            }
            let off = (*s as usize + 1) * sector;
            for i in 0..sector / 4 {
                fat.push(u32_at(data, off + i * 4)?);
            }
        }
        let mut cfb = Cfb { data, sector, mini_sector: 1 << mini_shift, mini_cutoff, fat, mini_fat: Vec::new(), mini_stream: Vec::new(), entries: Vec::new() };
        let dir = cfb.read_chain(dir_start, None)?;
        for e in dir.chunks_exact(128) {
            let len = (u16_at(e, 64)? as usize).min(64);
            let units: Vec<u16> = e[..len.saturating_sub(2)].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let name = String::from_utf16_lossy(&units);
            cfb.entries.push((name, e[66], u32_at(e, 116)?, u64_at(e, 120)?));
        }
        let root = cfb.entries.first().cloned().ok_or("compound file has no root entry")?;
        let mf = cfb.read_chain(mini_fat_start, None)?;
        cfb.mini_fat = mf.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        cfb.mini_stream = cfb.read_chain(root.2, Some(root.3))?;
        Ok(cfb)
    }

    fn read_chain(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut s = start;
        let mut seen = vec![false; self.data.len() / self.sector + 1];
        while s < FREE {
            match seen.get_mut(s as usize) {
                Some(v) if !*v => *v = true,
                Some(_) => return Err("corrupt compound file: sector chain loop".into()),
                None => return Err("compound file truncated".into()),
            }
            let off = (s as usize + 1) * self.sector;
            out.extend_from_slice(self.data.get(off..off + self.sector).ok_or("compound file truncated")?);
            s = *self.fat.get(s as usize).ok_or("corrupt compound file: sector outside the FAT")?;
            if size.is_some_and(|z| out.len() as u64 >= z) {
                break;
            }
        }
        if let Some(z) = size {
            if (out.len() as u64) < z {
                return Err("compound file truncated".into());
            }
            out.truncate(z as usize);
        }
        Ok(out)
    }

    /// The bytes of the named stream.
    pub fn stream(&self, name: &str) -> Result<Vec<u8>, String> {
        let (_, _, start, size) = self.entries.iter().find(|e| e.0 == name && e.1 == 2).ok_or_else(|| format!("the file has no {name} stream"))?.clone();
        if size >= self.mini_cutoff {
            return self.read_chain(start, Some(size));
        }
        let mut out = Vec::new();
        let mut s = start;
        let mut seen = vec![false; self.mini_stream.len() / self.mini_sector + 1];
        while s < FREE && (out.len() as u64) < size {
            match seen.get_mut(s as usize) {
                Some(v) if !*v => *v = true,
                Some(_) => return Err("corrupt compound file: mini chain loop".into()),
                None => return Err("compound file truncated".into()),
            }
            let off = s as usize * self.mini_sector;
            out.extend_from_slice(self.mini_stream.get(off..off + self.mini_sector).ok_or("compound file truncated")?);
            s = *self.mini_fat.get(s as usize).ok_or("corrupt compound file: mini sector outside the mini FAT")?;
        }
        if (out.len() as u64) < size {
            return Err("compound file truncated".into());
        }
        out.truncate(size as usize);
        Ok(out)
    }
}
