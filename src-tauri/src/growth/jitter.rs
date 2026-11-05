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
mod tests {
    use jiff::SignedDuration;

    use super::{fnv1a_64, jitter, jitter_from_hash};
    use crate::bundle_id::BundleId;

    const ONE_DAY: SignedDuration = SignedDuration::from_secs(86_400);

    fn id(text: &str) -> BundleId {
        BundleId::new(text).expect("test bundle IDs are non-empty")
    }

    #[test]
    fn hash_matches_the_standard_fnv1a_64_test_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn jitter_is_pinned_to_values_from_an_independent_fnv1a() {
        let cases = [
            ("com.apple.Safari", 38_842),
            ("com.microsoft.VSCode", 9_553),
            ("com.apple.finder", 31_764),
            ("com.example.Café", 84_638),
        ];

        for (bundle_id, seconds) in cases {
            assert_eq!(
                jitter(&id(bundle_id)),
                SignedDuration::from_secs(seconds),
                "{bundle_id}"
            );
        }
    }

    #[test]
    fn hash_remainder_0_maps_to_minus_one_day_and_172_800_to_plus_one_day() {
        let cases = [
            (0, -ONE_DAY),
            (86_400, SignedDuration::ZERO),
            (172_800, ONE_DAY),
            (172_801, -ONE_DAY),
            (u64::MAX, SignedDuration::from_secs(11_266 - 86_400)),
        ];

        for (hash, expected) in cases {
            assert_eq!(jitter_from_hash(hash), expected, "hash {hash}");
        }
    }

    #[test]
    fn jitter_stays_within_one_day_for_a_thousand_ids() {
        for n in 0..1_000 {
            let bundle_id = format!("com.example.app{n}");

            let offset = jitter(&id(&bundle_id));

            assert!(
                (-ONE_DAY..=ONE_DAY).contains(&offset),
                "{bundle_id} jitter {offset:?} is outside one day"
            );
        }
    }
}
