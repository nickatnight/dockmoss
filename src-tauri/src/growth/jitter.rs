use jiff::SignedDuration;

use crate::bundle_id::BundleId;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
pub(super) const MAX_JITTER_DAYS: u32 = 1;
const SECONDS_PER_DAY: i64 = 86_400;
const MAX_JITTER_SECONDS: i64 = MAX_JITTER_DAYS as i64 * SECONDS_PER_DAY;
const WHOLE_SECONDS_IN_JITTER_RANGE: u64 = (2 * MAX_JITTER_SECONDS + 1).cast_unsigned();

pub fn jitter(bundle_id: &BundleId) -> SignedDuration {
    jitter_from_hash(fnv1a_64(bundle_id.as_str().as_bytes()))
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "the remainder is below WHOLE_SECONDS_IN_JITTER_RANGE, far inside i64"
)]
fn jitter_from_hash(hash: u64) -> SignedDuration {
    SignedDuration::from_secs((hash % WHOLE_SECONDS_IN_JITTER_RANGE) as i64 - MAX_JITTER_SECONDS)
}

#[cfg(test)]
mod tests;
