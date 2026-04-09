use std::{
    io::{self, Read, Write},
    path::Path,
};

const FILE_MAGIC: &[u8; 4] = b"DIMR";
const FILE_VERSION: u32 = 1;

/// Physical dimension restriction on a way (height, width, weight).
/// Encoding: 0 in any field means "no restriction".
/// - `max_height_dm`: max height in decimetres (0.1 m). E.g. 40 = 4.0 m.
/// - `max_width_dm`: max width in decimetres (0.1 m). E.g. 25 = 2.5 m.
/// - `max_weight_250kg`: max weight in units of 250 kg. E.g. 30 = 7.5 t.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DimRestriction {
    pub max_height_dm: u8,
    pub max_width_dm: u8,
    pub max_weight_250kg: u8,
}

impl DimRestriction {
    pub const NONE: Self = Self {
        max_height_dm: 0,
        max_width_dm: 0,
        max_weight_250kg: 0,
    };

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    /// Returns true if a vehicle with the given dimensions is blocked by this restriction.
    /// Pass 0 for any dimension that should not be checked.
    pub fn blocks_vehicle(self, height_dm: u8, width_dm: u8, weight_250kg: u8) -> bool {
        (self.max_height_dm > 0 && height_dm > 0 && height_dm > self.max_height_dm)
            || (self.max_width_dm > 0 && width_dm > 0 && width_dm > self.max_width_dm)
            || (self.max_weight_250kg > 0
                && weight_250kg > 0
                && weight_250kg > self.max_weight_250kg)
    }
}

/// Lookup table mapping `dim_restriction_idx` (a `u8` stored in each `Way`) to a
/// [`DimRestriction`]. Index 0 is always [`DimRestriction::NONE`].
#[derive(Debug)]
pub struct DimRestrictionsTable {
    entries: Vec<DimRestriction>,
}

impl Default for DimRestrictionsTable {
    fn default() -> Self {
        Self {
            entries: vec![DimRestriction::NONE],
        }
    }
}

impl DimRestrictionsTable {
    /// Look up the restriction for a given index. Returns `NONE` for out-of-range indices.
    #[inline]
    pub fn get(&self, idx: u8) -> DimRestriction {
        self.entries
            .get(idx as usize)
            .copied()
            .unwrap_or(DimRestriction::NONE)
    }

    /// Find an existing entry or append a new one. Returns the assigned index.
    /// Panics if the table is full (>255 entries).
    pub fn get_or_insert(&mut self, entry: DimRestriction) -> u8 {
        if entry.is_none() {
            return 0;
        }
        if let Some(pos) = self.entries.iter().position(|&r| r == entry) {
            return pos as u8;
        }
        let idx = self.entries.len();
        assert!(
            idx < 256,
            "dim-restriction table overflow (> 255 unique entries)"
        );
        self.entries.push(entry);
        idx as u8
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Write the table to a file.
    /// Format: 4-byte magic, 4-byte LE version, 4-byte LE entry count, then entries (3 bytes each).
    pub fn write_to_file(&self, path: &Path) -> io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(FILE_MAGIC)?;
        f.write_all(&FILE_VERSION.to_le_bytes())?;
        f.write_all(&(self.entries.len() as u32).to_le_bytes())?;
        for e in &self.entries {
            f.write_all(&[e.max_height_dm, e.max_width_dm, e.max_weight_250kg])?;
        }
        Ok(())
    }

    /// Read a table previously written by [`write_to_file`].
    pub fn read_from_file(path: &Path) -> io::Result<Self> {
        let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)?;
        if magic != *FILE_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "dim_restrictions: invalid magic number",
            ));
        }
        let mut version_bytes = [0u8; 4];
        f.read_exact(&mut version_bytes)?;
        let version = u32::from_le_bytes(version_bytes);
        if version != FILE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "dim_restrictions: unsupported version {version} (expected {FILE_VERSION})"
                ),
            ));
        }
        let mut count_bytes = [0u8; 4];
        f.read_exact(&mut count_bytes)?;
        let count = u32::from_le_bytes(count_bytes) as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let mut buf = [0u8; 3];
            f.read_exact(&mut buf)?;
            entries.push(DimRestriction {
                max_height_dm: buf[0],
                max_width_dm: buf[1],
                max_weight_250kg: buf[2],
            });
        }
        if entries.first() != Some(&DimRestriction::NONE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "dim_restrictions: first entry is not NONE",
            ));
        }
        Ok(Self { entries })
    }
}
