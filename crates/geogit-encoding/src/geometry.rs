//! GeoPackage Binary geometry encoding.
//!
//! Kart stores geometries using the Standard GeoPackageBinary format
//! specified in GeoPackage v1.3.0 §2.1.3, with restrictions:
//! - Always little-endian
//! - SRS ID always 0 (CRS stored in schema, not per-geometry)
//! - Non-empty non-Point geometries must have an envelope
//! - Points and empty geometries have no envelope

/// GeoPackage binary header magic bytes
const GP_MAGIC: [u8; 2] = [0x47, 0x50]; // "GP"
const GP_VERSION: u8 = 0x00;

/// Envelope types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeType {
    None = 0,
    Xy = 1,
    Xyz = 2,
    Xym = 3,
    Xyzm = 4,
}

/// A GeoPackage Binary encoded geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct GpkgGeometry {
    pub data: Vec<u8>,
}

impl GpkgGeometry {
    /// Create a GeoPackage Binary from raw WKB geometry bytes.
    ///
    /// Wraps the WKB in a GeoPackage binary header with the appropriate envelope.
    pub fn from_wkb(wkb: &[u8], envelope: Option<Envelope>) -> Self {
        let mut data = Vec::new();

        // Magic
        data.extend_from_slice(&GP_MAGIC);

        // Version
        data.push(GP_VERSION);

        // Flags byte: bit layout (LE):
        // bit 0: byte order (1 = little-endian)
        // bits 1-3: envelope type
        // bit 4: empty geometry flag
        // bit 5: GeoPackageBinary type (0 = standard)
        let envelope_type = match &envelope {
            Some(env) => env.envelope_type() as u8,
            None => 0,
        };
        let flags: u8 = 0x01 | (envelope_type << 1); // LE + envelope type
        data.push(flags);

        // SRS ID (always 0, LE i32)
        data.extend_from_slice(&0i32.to_le_bytes());

        // Envelope (if present)
        if let Some(env) = &envelope {
            env.write_to(&mut data);
        }

        // WKB payload
        data.extend_from_slice(wkb);

        Self { data }
    }

    /// Extract the raw WKB payload from the GeoPackage Binary.
    pub fn to_wkb(&self) -> Result<&[u8], GeometryError> {
        if self.data.len() < 8 {
            return Err(GeometryError::TooShort);
        }
        if self.data[0..2] != GP_MAGIC {
            return Err(GeometryError::InvalidMagic);
        }

        let flags = self.data[3];
        let envelope_type = (flags >> 1) & 0x07;

        let envelope_size = match envelope_type {
            0 => 0,
            1 => 32, // 4 doubles (minx, maxx, miny, maxy)
            2 | 3 => 48, // 6 doubles (+ z or m range)
            4 => 64, // 8 doubles (+ z and m range)
            _ => return Err(GeometryError::InvalidEnvelopeType(envelope_type)),
        };

        let wkb_offset = 8 + envelope_size;
        if self.data.len() < wkb_offset {
            return Err(GeometryError::TooShort);
        }

        Ok(&self.data[wkb_offset..])
    }

    /// Get the raw bytes for MessagePack storage.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Bounding box envelope for GeoPackage Binary.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: Option<f64>,
    pub max_z: Option<f64>,
    pub min_m: Option<f64>,
    pub max_m: Option<f64>,
}

impl Envelope {
    pub fn xy(min_x: f64, max_x: f64, min_y: f64, max_y: f64) -> Self {
        Self {
            min_x,
            max_x,
            min_y,
            max_y,
            min_z: None,
            max_z: None,
            min_m: None,
            max_m: None,
        }
    }

    fn envelope_type(&self) -> EnvelopeType {
        match (self.min_z.is_some(), self.min_m.is_some()) {
            (false, false) => EnvelopeType::Xy,
            (true, false) => EnvelopeType::Xyz,
            (false, true) => EnvelopeType::Xym,
            (true, true) => EnvelopeType::Xyzm,
        }
    }

    fn write_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.min_x.to_le_bytes());
        buf.extend_from_slice(&self.max_x.to_le_bytes());
        buf.extend_from_slice(&self.min_y.to_le_bytes());
        buf.extend_from_slice(&self.max_y.to_le_bytes());
        if let Some(z) = self.min_z {
            buf.extend_from_slice(&z.to_le_bytes());
            buf.extend_from_slice(&self.max_z.unwrap_or(z).to_le_bytes());
        }
        if let Some(m) = self.min_m {
            buf.extend_from_slice(&m.to_le_bytes());
            buf.extend_from_slice(&self.max_m.unwrap_or(m).to_le_bytes());
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GeometryError {
    #[error("geometry data too short")]
    TooShort,
    #[error("invalid GeoPackage binary magic bytes")]
    InvalidMagic,
    #[error("invalid envelope type: {0}")]
    InvalidEnvelopeType(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpkg_geometry_roundtrip() {
        // A simple WKB point (LE): type=1 (Point), x=1.0, y=2.0
        let mut wkb = Vec::new();
        wkb.push(0x01); // LE byte order
        wkb.extend_from_slice(&1u32.to_le_bytes()); // type = Point
        wkb.extend_from_slice(&1.0f64.to_le_bytes()); // x
        wkb.extend_from_slice(&2.0f64.to_le_bytes()); // y

        // Points have no envelope
        let gpkg = GpkgGeometry::from_wkb(&wkb, None);
        assert_eq!(&gpkg.data[0..2], &GP_MAGIC);

        let extracted_wkb = gpkg.to_wkb().unwrap();
        assert_eq!(extracted_wkb, &wkb);
    }

    #[test]
    fn test_gpkg_geometry_with_envelope() {
        let wkb = vec![0x01, 0x03, 0, 0, 0]; // Minimal polygon start
        let env = Envelope::xy(-180.0, 180.0, -90.0, 90.0);
        let gpkg = GpkgGeometry::from_wkb(&wkb, Some(env));

        let flags = gpkg.data[3];
        let env_type = (flags >> 1) & 0x07;
        assert_eq!(env_type, 1); // XY envelope

        let extracted = gpkg.to_wkb().unwrap();
        assert_eq!(extracted, &wkb);
    }
}
