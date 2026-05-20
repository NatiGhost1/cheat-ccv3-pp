use std::f64::consts::{FRAC_PI_2, PI};

use crate::{osu::difficulty_object::OsuDifficultyObject, util::CompactVec};

use super::{previous, previous_start_time, OsuStrainSkill, Skill, StrainSkill};

#[derive(Clone, Debug)]
pub(crate) struct Aim {
    curr_strain: f64,
    curr_section_peak: f64,
    curr_section_end: f64,
    pub(crate) strain_peaks: CompactVec,
    with_sliders: bool,
    has_relax: bool,
}

impl Aim {
    const SKILL_MULTIPLIER: f64 = 23.55;
    const STRAIN_DECAY_BASE: f64 = 0.15;

    pub(crate) fn new(with_sliders: bool, has_relax: bool) -> Self {
        Self {
            curr_strain: 0.0,
            curr_section_peak: 0.0,
            curr_section_end: 0.0,
            strain_peaks: CompactVec::new(),
            with_sliders,
            has_relax,
        }
    }

    fn strain_decay(ms: f64) -> f64 {
        Self::STRAIN_DECAY_BASE.powf(ms / 1000.0)
    }
}

impl Skill for Aim {
    #[inline]
    fn process(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
    ) {
        <Self as StrainSkill>::process(self, curr, diff_objects)
    }

    #[inline]
    fn difficulty_value(&mut self) -> f64 {
        <Self as OsuStrainSkill>::difficulty_value(self)
    }
}

impl StrainSkill for Aim {
    #[inline]
    fn strain_peaks_mut(&mut self) -> &mut CompactVec {
        &mut self.strain_peaks
    }

    #[inline]
    fn curr_section_peak(&mut self) -> &mut f64 {
        &mut self.curr_section_peak
    }

    #[inline]
    fn curr_section_end(&mut self) -> &mut f64 {
        &mut self.curr_section_end
    }

    #[inline]
    fn strain_value_at(
        &mut self,
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        self.curr_strain *= Self::strain_decay(curr.delta_time);

        let eval_result = if self.has_relax {
            super::aim_rx::AimRxEvaluator::evaluate_diff_of(curr, diff_objects, self.with_sliders)
        } else {
            AimEvaluator::evaluate_diff_of(curr, diff_objects, self.with_sliders)
        };

        self.curr_strain += eval_result * Self::SKILL_MULTIPLIER;

        self.curr_strain
    }

    #[inline]
    fn calculate_initial_strain(
        &self,
        time: f64,
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
    ) -> f64 {
        self.curr_strain * Self::strain_decay(time - previous_start_time(diff_objects, curr.idx, 0))
    }

    #[inline]
    fn difficulty_value(&mut self) -> f64 {
        <Self as OsuStrainSkill>::difficulty_value(self)
    }
}

impl OsuStrainSkill for Aim {}

// ─── Windowed angle statistics ──────────────────────────────────────
// Shared helper: collects up to `window` previous angles (including curr)
// and returns (mean, stddev, count).

const ANGLE_WINDOW: usize = 8;

fn windowed_angle_stats(
    curr: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
    window: usize,
) -> (f64, f64, usize) {
    let mut angles: Vec<f64> = Vec::with_capacity(window + 1);

    if let Some(a) = curr.dists.angle {
        angles.push(a);
    }
    for back in 0..window {
        if let Some(prev) = previous(diff_objects, curr.idx, back) {
            if let Some(a) = prev.dists.angle {
                angles.push(a);
            }
        } else {
            break;
        }
    }

    let n = angles.len();
    if n < 3 {
        return (0.0, 0.0, n);
    }

    let mean: f64 = angles.iter().sum::<f64>() / n as f64;
    let variance: f64 = angles.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / n as f64;
    (mean, variance.sqrt(), n)
}

// ─── Windowed distance statistics ───────────────────────────────────
// Returns (mean_dist, dist_stddev, count) over recent jump distances.

fn windowed_dist_stats(
    curr: &OsuDifficultyObject<'_>,
    diff_objects: &[OsuDifficultyObject<'_>],
    window: usize,
) -> (f64, f64, usize) {
    let mut dists: Vec<f64> = Vec::with_capacity(window + 1);
    dists.push(curr.dists.lazy_jump_dist);

    for back in 0..window {
        if let Some(prev) = previous(diff_objects, curr.idx, back) {
            dists.push(prev.dists.lazy_jump_dist);
        } else {
            break;
        }
    }

    let n = dists.len();
    if n < 2 {
        return (0.0, 0.0, n);
    }

    let mean: f64 = dists.iter().sum::<f64>() / n as f64;
    let variance: f64 = dists.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / n as f64;
    (mean, variance.sqrt(), n)
}


// ─── Curved interpolation helpers ──────────────────────────────────
// Smoothstep-based functions for more accurate curve behavior

/// Cubic smoothstep: smooth interpolation from 0 to 1 over [0, 1]
/// More accurate and numerically stable than sine for normalized ranges
#[inline]
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smootherstep (Perlin's improved smoothstep): even smoother transitions
/// Uses 6t^5 - 15t^4 + 10t^3 for better visual smoothness
#[inline]
fn smootherstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Normalized sigmoid-like curve: smooth S-curve from 0 to 1
/// Better than sine for modeling difficulty curves
#[inline]
fn sigmoid_curve(x: f64, steepness: f64) -> f64 {
    1.0 / (1.0 + (-steepness * x).exp())
}

/// Quintic polynomial approximation for better precision
/// Maps a value through a quintic curve for smoother transitions
#[inline]
fn quintic_ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

struct AimEvaluator;

impl AimEvaluator {
    const WIDE_ANGLE_MULTIPLIER: f64 = 1.35;
    const ACUTE_ANGLE_MULTIPLIER: f64 = 2.0;
    const SLIDER_MULTIPLIER: f64 = 0.0; // Sliders give zero PP.
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.7;

    fn evaluate_diff_of(
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
        _with_sliders: bool,
    ) -> f64 {
        let osu_curr_obj = curr;

        let (osu_last_last_obj, osu_last_obj) = if let Some(tuple) =
            previous(diff_objects, curr.idx, 1)
                .zip(previous(diff_objects, curr.idx, 0))
                .filter(|(_, last)| !(curr.base.is_spinner() || last.base.is_spinner()))
        {
            tuple
        } else {
            return 0.0;
        };

        // ── Velocities ──────────────────────────────────────────────
        let mut curr_vel = osu_curr_obj.dists.lazy_jump_dist / osu_curr_obj.strain_time;

        // Slider travel still contributes to velocity calc (the cursor
        // still moves through slider paths), but slider_bonus itself is 0.
        if osu_last_obj.base.is_slider() {
            let travel_vel = osu_last_obj.dists.travel_dist / osu_last_obj.dists.travel_time;
            let movement_vel = osu_curr_obj.dists.min_jump_dist / osu_curr_obj.dists.min_jump_time;
            curr_vel = curr_vel.max(movement_vel + travel_vel);
        }

        let mut prev_vel = osu_last_obj.dists.lazy_jump_dist / osu_last_obj.strain_time;

        if osu_last_last_obj.base.is_slider() {
            let travel_vel =
                osu_last_last_obj.dists.travel_dist / osu_last_last_obj.dists.travel_time;
            let movement_vel = osu_last_obj.dists.min_jump_dist / osu_last_obj.dists.min_jump_time;
            prev_vel = prev_vel.max(movement_vel + travel_vel);
        }

        let mut wide_angle_bonus = 0.0;
        let mut acute_angle_bonus = 0.0;
        let mut vel_change_bonus = 0.0;

        let mut aim_strain = curr_vel;

        // ── Angle bonuses ───────────────────────────────────────────
        if osu_curr_obj.strain_time.max(osu_last_obj.strain_time)
            < 1.25 * osu_curr_obj.strain_time.min(osu_last_obj.strain_time)
        {
            if let Some(((curr_angle, last_angle), last_last_angle)) = osu_curr_obj
                .dists
                .angle
                .zip(osu_last_obj.dists.angle)
                .zip(osu_last_last_obj.dists.angle)
            {
                let angle_bonus = curr_vel.min(prev_vel);

                wide_angle_bonus = Self::calc_wide_angle_bonus(curr_angle);
                acute_angle_bonus = Self::calc_acute_angle_bonus(curr_angle);

                if osu_curr_obj.strain_time > 100.0 {
                    acute_angle_bonus = 0.0;
                } else {
                    // Use smootherstep for better numerical characteristics
                    let time_factor = ((100.0 - osu_curr_obj.strain_time) / 25.0).clamp(0.0, 1.0);
                    let base1 = smootherstep(time_factor);
                    
                    let dist_factor = ((osu_curr_obj.dists.lazy_jump_dist).clamp(50.0, 100.0) - 50.0) / 50.0;
                    let base2 = smootherstep(dist_factor);

                    acute_angle_bonus *= Self::calc_acute_angle_bonus(last_angle)
                        * angle_bonus.min(125.0 / osu_curr_obj.strain_time)
                        * base1
                        * base1
                        * base2
                        * base2;
                }

                // Vanilla pairwise repetition penalties (kept as a base layer)
                wide_angle_bonus *= angle_bonus
                    * (1.0 - wide_angle_bonus.min(Self::calc_wide_angle_bonus(last_angle).powi(3)));
                acute_angle_bonus *= 0.5
                    + 0.5
                        * (1.0
                            - acute_angle_bonus
                                .min(Self::calc_acute_angle_bonus(last_last_angle).powi(3)));
            }
        }

        // ── Velocity change bonus ──────────────────────────────────
        if prev_vel.max(curr_vel).abs() > f64::EPSILON {
            prev_vel = (osu_last_obj.dists.lazy_jump_dist + osu_last_last_obj.dists.travel_dist)
                / osu_last_obj.strain_time;
            curr_vel = (osu_curr_obj.dists.lazy_jump_dist + osu_last_obj.dists.travel_dist)
                / osu_curr_obj.strain_time;

            // Use smootherstep instead of sine for better numerical accuracy
            let vel_change_ratio = ((prev_vel - curr_vel).abs() / prev_vel.max(curr_vel)).clamp(0.0, 1.0);
            let dist_ratio_base = smootherstep(vel_change_ratio);
            let dist_ratio = dist_ratio_base * dist_ratio_base;

            let overlap_vel_buff = (125.0 / osu_curr_obj.strain_time.min(osu_last_obj.strain_time))
                .min((prev_vel - curr_vel).abs());

            vel_change_bonus = overlap_vel_buff * dist_ratio;

            let bonus_base = osu_curr_obj.strain_time.min(osu_last_obj.strain_time)
                / osu_curr_obj.strain_time.max(osu_last_obj.strain_time);
            vel_change_bonus *= bonus_base * bonus_base;
        }

        // ── Combine base aim strain (no slider bonus) ───────────────
        aim_strain += (acute_angle_bonus * Self::ACUTE_ANGLE_MULTIPLIER).max(
            wide_angle_bonus * Self::WIDE_ANGLE_MULTIPLIER
                + vel_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER,
        );
        // Sliders give zero PP — SLIDER_MULTIPLIER is 0.0, skip entirely.

        // ════════════════════════════════════════════════════════════
        // BPM-aware exponential scaling system
        //
        // Calibrated to match old discrete tier system output:
        //   - Anchor: 410 BPM ≈ 0.65x (avg of old SLIGHT_VAR/VARIED at this BPM)
        //   - Exponential growth above 410, reduction below
        //   - Variety modulation: low variety → higher multiplier at high BPM
        //   - Unified scaling for all patterns (removed discrete tiers)
        //   - cap dampening above 520.5 strain
        // ════════════════════════════════════════════════════════════

        // Effective BPM is capped at 520.5 to prevent extreme values from very short strain times
        // This cap ensures that patterns with strain times corresponding to >520.5 BPM do not produce disproportionately high aim strains.
        let eff_bpm = (30_000.0 / osu_curr_obj.strain_time).min(520.5);

        // ── Variety measurement (angle + distance) ──────────────────
        let (angle_mean, angle_stddev, angle_n) =
            windowed_angle_stats(osu_curr_obj, diff_objects, ANGLE_WINDOW);
        let (dist_mean, dist_stddev, dist_n) =
            windowed_dist_stats(osu_curr_obj, diff_objects, ANGLE_WINDOW);

        // Angle variation: 0 = perfectly repetitive, 1 = maximally varied
        let angle_var = if angle_n >= 3 {
            (angle_stddev / 1.0).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // Distance variation: normalized by mean distance
        let dist_var = if dist_n >= 3 && dist_mean > 0.0 {
            (dist_stddev / dist_mean).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // Combined variety: 0 = repetitive, 1 = highly varied
        let combined_variety = (angle_var + dist_var) / 2.0;

        // ── BPM scaling calculation ────────────────────────────────
        // Calibrated to match old system (which ranged ~0.25-0.95x)
        // Anchor at 410 BPM ≈ 0.65x (equivalent to old SLIGHT_VAR/VARIED)
        let bpm_base_factor = if eff_bpm < 410.0 {
            // Below 410: smooth ramp from ~0.38 at 250 BPM to ~0.65 at 410
            let low_bpm_t = ((eff_bpm - 250.0) / (410.0 - 250.0)).clamp(0.0, 1.0);
            let base_floor = 0.38;
            let base_anchor = 0.65;
            base_floor + (base_anchor - base_floor) * smootherstep(low_bpm_t)
        } else {
            // Above 410: exponential growth but conservative (power 1.15)
            // Grows from 0.65 at 410 to ~0.85 at 470, ~1.05 at 500+
            let bpm_ratio = (eff_bpm - 410.0) / 180.0;
            0.65 * (1.0 + bpm_ratio.powf(1.15))
        };

        // Variety modulation: low variety gets boost at high BPM
        // At high BPM (>410), repetitive patterns get less nerf
        // Varied patterns get moderate nerf to maintain balance
        let variety_mod = if eff_bpm > 410.0 {
            let high_bpm_t = ((eff_bpm - 410.0) / 180.0).clamp(0.0, 1.0);
            // Boost range: low variety gains ~10-15% at high BPM
            let boost = combined_variety * 0.12 * smootherstep(high_bpm_t);
            1.0 - boost
        } else {
            1.0
        };

        // Final BPM multiplier = base curve × variety modulation
        let bpm_multiplier = bpm_base_factor * variety_mod;
        let mut scaled_strain = aim_strain * bpm_multiplier;

        // ── Natighost Pattern Nerf ─────────────────────────────────
        // Based on analysis of PookieNati / Aim Assist Farm maps.
        // Detects cheese patterns where a perfectly stacked note (near-zero movement) 
        // is followed immediately by a massive cross-screen snap at exceptionally 
        // short delta times (e.g., 65ms / 1/4th beat at 230 BPM), or vice-versa.
        // These exploit the lack of deceleration requirement to artificially inflate aim strain.
        if curr.delta_time < 80.0 {
            if let Some(prev_obj) = previous(curr, diff_objects, 0) {
                if prev_obj.delta_time < 80.0 {
                    let prev_strain = prev_obj.aim_strain;
                    
                    // Case 1: Stack into a massive jump
                    if prev_strain < 0.5 && curr.aim_strain > 1.5 {
                        let ratio = prev_strain / curr.aim_strain;
                        // Scale down heavily. Perfectly stacked (0) into a spike hits the maximum 0.25x nerf factor.
                        let nerf_factor = 0.25 + 0.75 * (ratio / 0.3);
                        scaled_strain *= nerf_factor.clamp(0.25, 1.0);
                    }
                    // Case 2: Massive jump into a stack (cheesing deceleration/stopping power)
                    else if curr.aim_strain < 0.5 && prev_strain > 1.5 {
                        let ratio = curr.aim_strain / prev_strain;
                        let nerf_factor = 0.4 + 0.6 * (ratio / 0.3);
                        scaled_strain *= nerf_factor.clamp(0.4, 1.0);
                    }
                }
            }
        }

        // ── Wide-to-Acute Angle Transition Nerf ────────────────────
        // Penalizes patterns that exploit radical angle switches back-and-forth across consecutive notes.
        // This targets maps that stack high wide-angle bonuses (linear flow) directly adjacent to 
        // high acute-angle bonuses (sharp snaps/turnarounds), which hyper-inflate cumulative aim strain.
        if let (Some(curr_angle), Some(prev_angle)) = (osu_curr_obj.dists.angle, osu_last_obj.dists.angle) {
            let curr_wide = Self::calc_wide_angle_bonus(curr_angle);
            let curr_acute = Self::calc_acute_angle_bonus(curr_angle);
            let prev_wide = Self::calc_wide_angle_bonus(prev_angle);
            let prev_acute = Self::calc_acute_angle_bonus(prev_angle);

            // Severity spikes when transitioning directly between full fluid flow and a full turnaround
            let transition_severity = (curr_wide * prev_acute).max(curr_acute * prev_wide);

            if transition_severity > 0.1 {
                // The nerf scales up at higher speeds where snapping tools and cursor mechanics yield unrealistic difficulty spikes
                let speed_factor = (125.0 / osu_curr_obj.strain_time.max(40.0)).clamp(0.5, 1.5);
                let nerf_factor = 1.0 - 0.22 * transition_severity * speed_factor;
                scaled_strain *= nerf_factor.clamp(0.68, 1.0);
            }
        }

        // ── Cap dampening above 520.5 strain ───────────────────────
        // Limits extreme growth at very high BPMs while maintaining smooth curve
        // The cap threshold is set at 520.5, which corresponds to the effective BPM cap. 
        // This ensures that strains corresponding to BPMs above 520.5 do not produce disproportionately high values.
        // TODO: Use claude to check all code and fix any remaining issues with this cap implementation, 
        // ensuring it integrates smoothly with the overall scaling system.
        const CAP_THRESHOLD: f64 = 520.5;
        const DAMPENING_FACTOR: f64 = 0.05;

        if scaled_strain > CAP_THRESHOLD {
            let excess = scaled_strain - CAP_THRESHOLD;
            let dampening = 1.0 / (1.0 + DAMPENING_FACTOR * excess);
            scaled_strain = CAP_THRESHOLD + excess * dampening;
        }

        aim_strain = scaled_strain;

        aim_strain
    }

    fn calc_wide_angle_bonus(angle: f64) -> f64 {
        // Normalize angle to [0, 1] range within the meaningful bounds (PI/6 to 5PI/6)
        let normalized = ((angle.max(PI / 6.0).min(5.0 * PI / 6.0)) - PI / 6.0) / (5.0 * PI / 6.0 - PI / 6.0);
        // Use quintic polynomial for smoother, more accurate curve
        let curve = quintic_ease(normalized);
        curve * curve
    }

    fn calc_acute_angle_bonus(angle: f64) -> f64 {
        1.0 - Self::calc_wide_angle_bonus(angle)
    }
}
