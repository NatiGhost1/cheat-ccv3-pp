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
                    let base1 =
                        (FRAC_PI_2 * ((100.0 - osu_curr_obj.strain_time) / 25.0).min(1.0)).sin();
                    let base2 = (FRAC_PI_2
                        * ((osu_curr_obj.dists.lazy_jump_dist).clamp(50.0, 100.0) - 50.0)
                        / 50.0)
                        .sin();

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

            let dist_ratio_base =
                (FRAC_PI_2 * (prev_vel - curr_vel).abs() / prev_vel.max(curr_vel)).sin();
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
        // CC V3: BPM-aware variation tiering
        //
        // Nerf multiplier depends on two axes:
        //   1. Pattern variation (from windowed angle + distance stats)
        //   2. Effective BPM
        //
        // Variation tiers:
        //   REPETITIVE  (angle_var < 0.20 rad)  → hardest nerf
        //   SLIGHT_VAR  (0.20 – 0.55 rad)       → heavy nerf
        //   VARIED      (0.55 – 0.90 rad)       → moderate nerf
        //   TECH        (varied angles + high vel_change) → lightest nerf
        //
        // Cross-screen (large constant-distance, low angle var):
        //   → intermediate between SLIGHT_VAR and VARIED
        //
        // BPM curve:
        //   < 300 eff BPM    → full nerf strength
        //   300 – 450        → linear relief
        //   > 450            → minimum nerf (but repetitive still pays)
        //
        // The multiplier floor/ceiling per tier:
        //
        //   Tier          |  <300 BPM  |  450+ BPM
        //   ──────────────┼───────────┼──────────
        //   REPETITIVE    |   0.25    |   0.55
        //   SLIGHT_VAR    |   0.40    |   0.70
        //   CROSS_SCREEN  |   0.50    |   0.78
        //   VARIED        |   0.60    |   0.85
        //   TECH          |   0.75    |   0.95
        // ════════════════════════════════════════════════════════════

        let eff_bpm = 30_000.0 / osu_curr_obj.strain_time;

        // BPM factor: 0.0 at ≤300, 1.0 at ≥450
        let bpm_factor = ((eff_bpm - 300.0) / 150.0).clamp(0.0, 1.0);

        // ── Variation measurement ───────────────────────────────────
        let (angle_mean, angle_stddev, angle_n) =
            windowed_angle_stats(osu_curr_obj, diff_objects, ANGLE_WINDOW);
        let (dist_mean, dist_stddev, dist_n) =
            windowed_dist_stats(osu_curr_obj, diff_objects, ANGLE_WINDOW);

        // Angle variation: 0 = perfectly repetitive, 1 = maximally varied
        let angle_var = if angle_n >= 3 {
            (angle_stddev / 1.0).clamp(0.0, 1.0)
        } else {
            0.5 // neutral if not enough data
        };

        // Distance variation (for cross-screen detection)
        let dist_var = if dist_n >= 3 && dist_mean > 0.0 {
            (dist_stddev / dist_mean).clamp(0.0, 1.0)  // coefficient of variation
        } else {
            0.5
        };

        // Velocity change ratio (for tech detection)
        let tech_signal = if aim_strain > f64::EPSILON {
            (vel_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER / aim_strain)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        // ── Classify the pattern ────────────────────────────────────
        // Cross-screen: large distances (mean > 360 px), low distance
        // variation (constant spacing), but not necessarily low angle var
        let is_cross_screen = dist_mean > 360.0 && dist_var < 0.25 && angle_var < 0.55;

        // Tech: varied angles AND meaningful velocity changes
        let is_tech = angle_var >= 0.55 && tech_signal >= 0.15;

        // Select floor (at <300 BPM) and ceiling (at 450+ BPM) for this pattern
        let (floor, ceiling) = if is_tech {
            // Tech patterns: least nerfed
            (0.75, 0.95)
        } else if angle_var >= 0.55 {
            // Varied (but not tech — missing the vel_change signal)
            (0.60, 0.85)
        } else if is_cross_screen {
            // Cross-screen constant-distance: between slight_var and varied
            (0.50, 0.78)
        } else if angle_var >= 0.20 {
            // Slight variation
            (0.40, 0.70)
        } else {
            // Extremely repetitive
            (0.25, 0.55)
        };

        // ── Final nerf multiplier ───────────────────────────────────
        // Interpolate between floor and ceiling based on BPM factor
        let variation_nerf = floor + (ceiling - floor) * bpm_factor;

        aim_strain *= variation_nerf;

        aim_strain
    }

    fn calc_wide_angle_bonus(angle: f64) -> f64 {
        let base = (3.0 / 4.0 * ((5.0 / 6.0 * PI).min(angle.max(PI / 6.0)) - PI / 6.0)).sin();
        base * base
    }

    fn calc_acute_angle_bonus(angle: f64) -> f64 {
        1.0 - Self::calc_wide_angle_bonus(angle)
    }
}
