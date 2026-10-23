//! Compact (nBits) <-> 256-bit target conversions, identical to
//! arith_uint256::SetCompact / GetCompact from Bitcoin/Dash.

use primitive_types::U256;

pub fn from_compact(compact: u32) -> U256 {
    let size = (compact >> 24) as usize;
    let word = compact & 0x007f_ffff;
    // Chain targets are never negative nor overflowing, so those flags are ignored.
    if size <= 3 {
        U256::from(word >> (8 * (3 - size)))
    } else {
        U256::from(word) << (8 * (size - 3))
    }
}

pub fn to_compact(target: U256) -> u32 {
    let mut size = target.bits().div_ceil(8);
    let mut compact: u32 = if size <= 3 {
        (target.low_u64() << (8 * (3 - size))) as u32
    } else {
        (target >> (8 * (size - 3))).low_u64() as u32
    };
    // If the sign bit (0x00800000) is set, divide the mantissa by 256 and bump the exponent.
    if compact & 0x0080_0000 != 0 {
        compact >>= 8;
        size += 1;
    }
    compact | ((size as u32) << 24)
}

/// Difficulty as reported by RPC (GetDifficulty), relative to 0x1d00ffff.
pub fn difficulty(bits: u32) -> f64 {
    let mut shift = (bits >> 24) & 0xff;
    let mut diff = 0x0000_ffff as f64 / (bits & 0x00ff_ffff) as f64;
    while shift < 29 {
        diff *= 256.0;
        shift += 1;
    }
    while shift > 29 {
        diff /= 256.0;
        shift -= 1;
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_roundtrip() {
        for bits in [0x1e0fffffu32, 0x1d00ffff, 0x1b0404cb, 0x1c05a3f4, 0x207fffff] {
            assert_eq!(to_compact(from_compact(bits)), bits, "{bits:08x}");
        }
    }

    #[test]
    fn genesis_difficulty_is_one() {
        assert_eq!(difficulty(0x1d00ffff), 1.0);
    }
}
