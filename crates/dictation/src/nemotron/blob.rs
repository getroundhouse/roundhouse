//! Reading `fp16` weight tensors out of a compiled Core ML `.mlmodelc` directory's `model.mil` and `weights/weight.bin`,
//! on any platform, so the Rust decoder and joint load the exact files the app already downloads.

use std::collections::HashMap;
use std::path::Path;

use crate::EngineError;

const BLOB_MAGIC: u32 = 0xDEAD_BEEF;
const BLOB_FP16: u32 = 1;
const HEADER_BYTES: usize = 64;
/// `2^-24` as an IEEE-754 `f32` (`(127 - 24) << 23`), the unit step of an `fp16` subnormal.
const SUBNORMAL_UNIT: f32 = f32::from_bits(0x3380_0000);

pub(super) const MODEL_MIL: &str = "model.mil";
pub(super) const WEIGHT_BIN: &str = "weights/weight.bin";

/// A compiled `.mlmodelc` directory's weight file and the byte offsets its `model.mil` names.
pub(super) struct MlModelWeights {
    dir_name: String,
    bytes: Vec<u8>,
    offsets: HashMap<String, usize>,
}

impl MlModelWeights {
    pub(super) fn read(mlmodelc: &Path) -> Result<Self, EngineError> {
        let dir_name =
            mlmodelc.file_name().map_or_else(|| mlmodelc.display().to_string(), |n| n.to_string_lossy().into());
        let mil_path = mlmodelc.join(MODEL_MIL);
        let mil = std::fs::read_to_string(&mil_path)
            .map_err(|e| EngineError(format!("couldn't read {}: {e}", mil_path.display())))?;
        let bin_path = mlmodelc.join(WEIGHT_BIN);
        let bytes =
            std::fs::read(&bin_path).map_err(|e| EngineError(format!("couldn't read {}: {e}", bin_path.display())))?;
        let offsets = parse_mil_offsets(&mil, &dir_name)?;
        Ok(Self { dir_name, bytes, offsets })
    }

    /// Reads the `fp16` tensor named `var` in `model.mil`, checking that it holds `expected_len` values.
    pub(super) fn tensor(&self, var: &str, expected_len: usize) -> Result<Vec<f32>, EngineError> {
        let bad = |why: &str| EngineError(format!("{} weight {var}: {why}", self.dir_name));
        let &offset = self.offsets.get(var).ok_or_else(|| bad("not found in model.mil"))?;
        let header =
            self.bytes.get(offset..offset.saturating_add(HEADER_BYTES)).ok_or_else(|| bad("truncated header"))?;
        let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let dtype = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if magic != BLOB_MAGIC || dtype != BLOB_FP16 {
            return Err(bad(&format!("unexpected blob header ({magic:#x}, dtype {dtype})")));
        }
        let byte_len = usize::try_from(u64::from_le_bytes([
            header[8], header[9], header[10], header[11], header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| bad("blob size overflows usize"))?;
        let data_at = usize::try_from(u64::from_le_bytes([
            header[16], header[17], header[18], header[19], header[20], header[21], header[22], header[23],
        ]))
        .map_err(|_| bad("blob offset overflows usize"))?;
        if byte_len != expected_len.saturating_mul(2) {
            return Err(bad(&format!("expected {} bytes, found {byte_len}", expected_len.saturating_mul(2))));
        }
        let raw = self.bytes.get(data_at..data_at.saturating_add(byte_len)).ok_or_else(|| bad("truncated data"))?;
        Ok(raw.as_chunks::<2>().0.iter().map(|&pair| f16_to_f32(u16::from_le_bytes(pair))).collect())
    }
}

fn parse_mil_offsets(mil: &str, dir_name: &str) -> Result<HashMap<String, usize>, EngineError> {
    let mut offsets = HashMap::new();
    for line in mil.lines() {
        let Some(blob_pos) = line.find("BLOBFILE(") else { continue };
        let Some(eq_pos) = line[..blob_pos].find('=') else { continue };
        let lhs = line[..eq_pos].trim();
        let Some(var) = lhs.split_whitespace().last() else { continue };
        let after_offset = line[blob_pos..]
            .split("offset")
            .nth(1)
            .and_then(|rest| rest.rsplit_once('('))
            .and_then(|(_, tail)| tail.split(')').next())
            .ok_or_else(|| EngineError(format!("{dir_name} model.mil has an unreadable BLOBFILE line")))?;
        let offset: usize = after_offset
            .trim()
            .parse()
            .map_err(|_| EngineError(format!("{dir_name} model.mil has a bad offset for {var}")))?;
        offsets.insert(var.to_string(), offset);
    }
    Ok(offsets)
}

/// Converts an IEEE-754 binary16 value to `f32`.
pub(super) fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exp = u32::from((bits >> 10) & 0x1f);
    let mant = u32::from(bits & 0x03ff);
    match exp {
        0 if mant == 0 => f32::from_bits(sign),
        0 => {
            #[expect(clippy::cast_precision_loss, reason = "10-bit mantissa fits in f32 without rounding")]
            let mag = (mant as f32) * SUBNORMAL_UNIT;
            if sign == 0 { mag } else { -mag }
        }
        31 => f32::from_bits(sign | 0x7f80_0000 | (mant << 13)),
        _ => f32::from_bits(sign | ((exp + 112) << 23) | (mant << 13)),
    }
}

#[cfg(test)]
mod tests {
    use super::{BLOB_FP16, BLOB_MAGIC, HEADER_BYTES, MlModelWeights, f16_to_f32};

    #[test]
    fn f16_converts_zeros_normals_subnormals_and_non_finites() {
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert!(f16_to_f32(0x8000).is_sign_negative() && f16_to_f32(0x8000) == 0.0);
        assert!((f16_to_f32(0x3c00) - 1.0).abs() < 1e-6);
        assert!((f16_to_f32(0xc000) + 2.0).abs() < 1e-6);
        assert!((f16_to_f32(0x41a8) - 2.828_125).abs() < 1e-6);
        assert!((f16_to_f32(0x0001) - 2.0_f32.powi(-24)).abs() < 1e-13);
        assert!(f16_to_f32(0x7c00).is_infinite() && f16_to_f32(0x7c00).is_sign_positive());
        assert!(f16_to_f32(0x7e00).is_nan());
    }

    #[test]
    fn reads_named_blob_and_rejects_wrong_length() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("weights")).unwrap();
        let mil = r#"tensor<fp16, [2]> w = const()[val = tensor<fp16, [2]>(BLOBFILE(path = tensor<string, []>("@model_path/weights/weight.bin"), offset = tensor<uint64, []>(64)))];"#;
        std::fs::write(dir.path().join("model.mil"), mil).unwrap();
        let mut bin = vec![0u8; HEADER_BYTES * 2 + 4];
        bin[64..68].copy_from_slice(&BLOB_MAGIC.to_le_bytes());
        bin[68..72].copy_from_slice(&BLOB_FP16.to_le_bytes());
        bin[72..80].copy_from_slice(&4u64.to_le_bytes());
        bin[80..88].copy_from_slice(&128u64.to_le_bytes());
        bin[128..130].copy_from_slice(&0x3c00u16.to_le_bytes());
        bin[130..132].copy_from_slice(&0xc000u16.to_le_bytes());
        std::fs::write(dir.path().join("weights/weight.bin"), bin).unwrap();

        let weights = MlModelWeights::read(dir.path()).unwrap();
        assert_eq!(weights.tensor("w", 2).unwrap(), [1.0, -2.0]);
        assert!(weights.tensor("w", 3).is_err());
        assert!(weights.tensor("missing", 2).is_err());
    }
}
