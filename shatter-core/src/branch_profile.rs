//! Branch frequency profile for profile-guided concolic prioritization.
//!
//! Collects per-branch observation frequencies from the random exploration
//! phase and provides rarity scores that bias concolic effort toward
//! rarely-reached or never-seen branches.

use std::collections::HashMap;
use std::fmt;

/// Branch frequency profile mapping branch IDs to observation frequencies.
///
/// Frequencies are in [0.0, 1.0] where 0.0 means never observed and 1.0
/// means observed in every execution. Rarity is the complement: branches
/// never seen have rarity 1.0, always-seen branches have rarity 0.0.
#[derive(Debug, Clone)]
pub struct BranchProfile {
    frequencies: HashMap<u32, f64>,
}

impl BranchProfile {
    pub fn new(frequencies: HashMap<u32, f64>) -> Self {
        Self { frequencies }
    }

    /// Rarity score for a branch: 1.0 − frequency.
    /// Returns 1.0 for branches not in the profile (never observed).
    pub fn rarity(&self, branch_id: u32) -> f64 {
        1.0 - self.frequencies.get(&branch_id).copied().unwrap_or(0.0)
    }

    /// Number of distinct branches tracked.
    pub fn len(&self) -> usize {
        self.frequencies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frequencies.is_empty()
    }

    /// Iterate over (branch_id, frequency) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &f64)> {
        self.frequencies.iter()
    }
}

impl fmt::Display for BranchProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BranchProfile({} branches)", self.frequencies.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_unknown_branch_is_one() {
        let profile = BranchProfile::new(HashMap::new());
        assert_eq!(profile.rarity(42), 1.0);
    }

    #[test]
    fn rarity_always_seen_is_zero() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 1.0);
        let profile = BranchProfile::new(freqs);
        assert_eq!(profile.rarity(1), 0.0);
    }

    #[test]
    fn rarity_half_frequency() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.5);
        let profile = BranchProfile::new(freqs);
        assert!((profile.rarity(1) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_profile() {
        let profile = BranchProfile::new(HashMap::new());
        assert!(profile.is_empty());
        assert_eq!(profile.len(), 0);
    }

    #[test]
    fn display_format() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.5);
        freqs.insert(2, 0.8);
        let profile = BranchProfile::new(freqs);
        assert_eq!(format!("{profile}"), "BranchProfile(2 branches)");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a BranchProfile with frequencies clamped to [0.0, 1.0].
    fn arb_branch_profile() -> impl Strategy<Value = BranchProfile> {
        proptest::collection::hash_map(any::<u32>(), 0.0..=1.0f64, 0..20)
            .prop_map(BranchProfile::new)
    }

    proptest! {
        #[test]
        fn rarity_always_in_zero_one(profile in arb_branch_profile(), branch_id in any::<u32>()) {
            let r = profile.rarity(branch_id);
            prop_assert!((0.0..=1.0).contains(&r), "rarity {r} out of [0, 1]");
        }

        #[test]
        fn rarity_complement_of_frequency(profile in arb_branch_profile()) {
            for (&id, &freq) in profile.iter() {
                let r = profile.rarity(id);
                let expected = 1.0 - freq;
                prop_assert!((r - expected).abs() < f64::EPSILON,
                    "rarity({id}) = {r}, expected {expected}");
            }
        }

        #[test]
        fn frequencies_are_bounded(profile in arb_branch_profile()) {
            for (_, &freq) in profile.iter() {
                prop_assert!((0.0..=1.0).contains(&freq),
                    "frequency {freq} out of [0, 1]");
            }
        }
    }
}
