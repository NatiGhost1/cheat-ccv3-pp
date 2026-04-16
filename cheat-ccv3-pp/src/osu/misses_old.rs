// OLD MISS WEIGHTING SYSTEM (1-MINUTE PEAKS) — BACKUP
//
// This file contains the original strain-aware miss weighting implementation
// that used 1-minute local SR peaks (local_sr_per_minute).
// It has been replaced with a 15-second version for more granular accuracy.
// Keep this as reference in case the new system needs to be reverted.

/*

// CC V3: strain-aware miss weighting (non-RX).
//
//   * 0 misses            → 1.0 (FC gets full credit)
//   * 1+ misses on RX     → plain vanilla combo scaling (this system
//                           does NOT apply to Relax)
//   * 1+ misses otherwise → strain-aware miss weight × completion
//   * 4-mod (DT+FL+HD+HR) → pin miss_count to raw n_misses,
//                           add short_map_tax on top
//
// The strain-aware weight is SYMMETRIC around the map midpoint:
// a miss 1000 notes from the start and a miss 1000 notes from the
// end of an equal-length map receive the same miss_weight. Misses
// at the midpoint are weighted most heavily. The maximum loss at
// midpoint is 40% if the local strain there is low relative to
// the map's peak, and 25% if the miss happens on the map's
// hardest peak.
//
// For multi-miss plays, an "escalation damping" keeps subsequent
// misses from cascading pp to zero once the first miss already
// ate a heavy position-based penalty.

let is_4mod = self.mods.dt() && self.mods.fl() && self.mods.hd() && self.mods.hr();

let miss_count = if is_4mod {
    self.state.n_misses as f64
} else {
    self.effective_miss_count
};

let miss_combo_mult = if miss_count <= 0.0 {
    1.0
} else if self.mods.rx() {
    // RX: skip strain-aware system, use plain vanilla combo scaling
    // temporary until we have a better solution for RX misses in CC V3
    // — ideally we'd want to also consider miss position for RX,
    // but that requires more extensive changes to the state and calculator
    if self.attrs.max_combo > 0 {
        ((self.state.max_combo as f64).powf(0.8)
            / (self.attrs.max_combo as f64).powf(0.8))
            .min(1.0)
    } else {
        1.0
    }
} else {
    self.cc_v3_strain_aware_miss_mult_old(miss_count)
};

// === OLD FUNCTION (using 1-minute peaks) ===

    /// CC V3 strain-aware symmetric miss weighting (non-RX, 1+ misses).
    ///
    /// Two multiplicative factors:
    ///
    ///   completion   — "how much of the map did the player play?"
    ///                  Uses combo_ratio^0.4, floored at 0.70 so a low-combo
    ///                  play doesn't get destroyed by completion alone.
    ///
    ///   miss_weight  — "how fair/harsh was the miss location?"
    ///                  SYMMETRIC around the map midpoint. Edges → 1.0.
    ///                  Midpoint → capped based on local strain:
    ///                    - easy section (strain_rel ≈ 0): 40% loss max
    ///                    - peak section (strain_rel ≈ 1): 25% loss max
    ///
    /// For miss_count > 1, an escalation-damping factor prevents additional
    /// misses from cascading pp to zero: the stronger the first miss was
    /// down-weighted, the weaker the per-extra-miss exponent becomes.
    ///
    /// Floor at 0.35 so pathological plays can't completely zero out.
    fn cc_v3_strain_aware_miss_mult_old(&self, miss_count: f64) -> f64 {
        if self.attrs.max_combo == 0 || miss_count <= 0.0 {
            return 1.0;
        }

        let combo_ratio =
            (self.state.max_combo as f64 / self.attrs.max_combo as f64).clamp(0.0, 1.0);

        // Symmetric midpoint proximity: 1.0 exactly halfway, 0.0 at either edge.
        // Triangle shape → a miss at combo_ratio=0.2 and combo_ratio=0.8
        // produce identical prox values (equidistant from 0.5).
        let midpoint_prox = 1.0 - ((combo_ratio - 0.5).abs() / 0.5).min(1.0);

        // Strain context: sample local SR at the miss position, compare to peak.
        // Uses 1-MINUTE peaks from local_sr_per_minute
        let strain_relative = if !self.attrs.local_sr_per_minute.is_empty() {
            let n = self.attrs.local_sr_per_minute.len();
            let idx = ((combo_ratio * n as f64) as usize).min(n - 1);
            let sample = self.attrs.local_sr_per_minute[idx];
            let peak = self
                .attrs
                .local_sr_per_minute
                .iter()
                .copied()
                .fold(0.0_f64, f64::max);
            if peak > 0.0 {
                (sample / peak).clamp(0.0, 1.0)
            } else {
                0.5 // neutral fallback when strain data is degenerate
            }
        } else {
            0.5 // neutral fallback when attrs has no local SR
        };

        // Max loss at midpoint: 40% easy → 25% peak
        let max_loss_at_midpoint = 0.40 - 0.15 * strain_relative;

        // Miss weight — symmetric, tapers from 1.0 at edges to
        // (1 - max_loss_at_midpoint) at combo_ratio = 0.5
        let mut miss_weight = 1.0 - midpoint_prox * max_loss_at_midpoint;

        // Escalation damping for multi-miss plays
        if miss_count > 1.0 {
            let extra = miss_count - 1.0;
            let already_paid = 1.0 - miss_weight; // e.g. 0.40 at worst midpoint
            let per_extra_exp = (0.3 * (1.0 - already_paid * 2.0)).max(0.1);
            miss_weight *= miss_weight.powf(extra * per_extra_exp);
        }

        // Completion factor — separate axis, gentler than vanilla, floored
        let completion_raw = (combo_ratio).powf(0.4);
        let completion = completion_raw.max(0.70);

        // Compose and apply hard floor
        (completion * miss_weight).max(0.35)
    }

*/
