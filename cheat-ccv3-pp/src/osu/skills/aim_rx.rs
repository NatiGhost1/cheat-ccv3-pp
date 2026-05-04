// Relax-specific aim evaluator for cheat-ccv3-pp.
//
// Port of aim_rx.rs from combo-consistency-v3-pp. Adapted for this crate's
// API: fields on curr.dists.*, previous() as a free function, no FloatExt.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::osu::difficulty_object::OsuDifficultyObject;

use super::previous;

// ─── Curved interpolation helpers ──────────────────────────────────
// Smoothstep-based functions for more accurate curve behavior

/// Smootherstep (Perlin's improved smoothstep): smooth polynomial transitions
/// Uses 6t^5 - 15t^4 + 10t^3 for better numerical stability than sine
#[inline]
fn smootherstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Cubic smoothstep: smooth interpolation for normalized ranges
#[inline]
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Quintic polynomial approximation for better precision and smoother curves
#[inline]
fn quintic_ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub(crate) struct AimRxEvaluator;

/// Collect up to `window` previous angles (including curr) and return
/// (mean, stddev, count). Returns (0.0, 0.0, n) if fewer than 3.
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
    let stddev = variance.sqrt();

    (mean, stddev, n)
}

impl AimRxEvaluator {
    // Uplifted from this crate's vanilla (1.35/2.0/1.32/0.70)
    const WIDE_ANGLE_MULTIPLIER: f64 = 1.46;
    const ACUTE_ANGLE_MULTIPLIER: f64 = 2.16;
    const SLIDER_MULTIPLIER: f64 = 1.18;
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.78;

    const SLOW_SLIDER_VEL_FLOOR: f64 = 0.55;
    const CONSTANT_DIST_RATIO: f64 = 0.18;
    const EDGE_TO_EDGE_THRESHOLD: f64 = 400.0;
    const CONSTANT_DIST_BPM_STRAIN_TIME: f64 = 85.7;

    const ANGLE_WINDOW: usize = 6;
    const FLOW_MEAN_ANGLE_THRESHOLD: f64 = 2.0;
    const FLOW_STDDEV_THRESHOLD: f64 = 0.3;
    const FLOW_MAX_NERF: f64 = 0.50;
    const FLOW_BPM_STRAIN_TIME: f64 = 36.58;

    pub(crate) fn evaluate_diff_of(
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
        with_sliders: bool,
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

        if osu_last_obj.base.is_slider() && with_sliders {
            let travel_vel = osu_last_obj.dists.travel_dist / osu_last_obj.dists.travel_time;
            let movement_vel = osu_curr_obj.dists.min_jump_dist / osu_curr_obj.dists.min_jump_time;
            curr_vel = curr_vel.max(movement_vel + travel_vel);
        }

        let mut prev_vel = osu_last_obj.dists.lazy_jump_dist / osu_last_obj.strain_time;

        if osu_last_last_obj.base.is_slider() && with_sliders {
            let travel_vel =
                osu_last_last_obj.dists.travel_dist / osu_last_last_obj.dists.travel_time;
            let movement_vel = osu_last_obj.dists.min_jump_dist / osu_last_obj.dists.min_jump_time;
            prev_vel = prev_vel.max(movement_vel + travel_vel);
        }

        let mut wide_angle_bonus = 0.0;
        let mut acute_angle_bonus = 0.0;
        let mut slider_bonus = 0.0;
        let mut vel_change_bonus = 0.0;

        let mut aim_strain = curr_vel;

        // ── Angle bonuses (only when rhythm is consistent) ──────────
        if osu_curr_obj.strain_time.max(osu_last_obj.strain_time)
            < 1.25 * osu_curr_obj.strain_time.min(osu_last_obj.strain_time)
        {
            if let Some(((curr_angle, last_angle), _last_last_angle)) = osu_curr_obj
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

                // ── BPM-aware repetition with windowed variance ─────
                let eff_bpm = 30_000.0 / osu_curr_obj.strain_time;
                let high_bpm_t = ((eff_bpm - 410.0) / 90.0).clamp(0.0, 1.0);

                let (_win_mean, win_stddev, win_n) =
                    windowed_angle_stats(osu_curr_obj, diff_objects, Self::ANGLE_WINDOW);

                let variance_factor = if win_n >= 3 {
                    (win_stddev / 1.2).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                let rep_strength = 1.0 - variance_factor;

                let wide_penalty = rep_strength * (1.0 - high_bpm_t);
                let wide_rep_buff = high_bpm_t * 0.15;
                wide_angle_bonus *=
                    angle_bonus * ((1.0 - wide_penalty + wide_rep_buff).max(0.0));

                let acute_penalty = rep_strength * 0.7 * (1.0 - high_bpm_t);
                let acute_rep_buff = high_bpm_t * 0.10;
                acute_angle_bonus *=
                    (0.5 + 0.5 * (1.0 - acute_penalty) + acute_rep_buff).max(0.0);
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

        // ── Slider bonus with slow-slider nerf ──────────────────────
        if osu_last_obj.base.is_slider() {
            let travel_vel = osu_last_obj.dists.travel_dist / osu_last_obj.dists.travel_time;
            slider_bonus = travel_vel;

            if travel_vel < Self::SLOW_SLIDER_VEL_FLOOR {
                let ratio = (travel_vel / Self::SLOW_SLIDER_VEL_FLOOR).clamp(0.0, 1.0);
                let slow_slider_taper = 0.55 + 0.45 * ratio;
                slider_bonus *= slow_slider_taper;
            }
        }

        // ── Combine ──────────────────────────────────────────────────
        aim_strain += (acute_angle_bonus * Self::ACUTE_ANGLE_MULTIPLIER).max(
            wide_angle_bonus * Self::WIDE_ANGLE_MULTIPLIER
                + vel_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER,
        );

        if with_sliders {
            aim_strain += slider_bonus * Self::SLIDER_MULTIPLIER;
        }

        // ── Cross-screen constant-distance nerf ─────────────────────
        if osu_curr_obj.strain_time >= Self::CONSTANT_DIST_BPM_STRAIN_TIME {
            let curr_d = osu_curr_obj.dists.lazy_jump_dist;
            let prev_d = osu_last_obj.dists.lazy_jump_dist;
            let max_d = curr_d.max(prev_d);
            let min_d = curr_d.min(prev_d);

            if max_d > 80.0 {
                let change_ratio = if max_d > 0.0 {
                    (max_d - min_d) / max_d
                } else {
                    1.0
                };

                let is_edge_to_edge = max_d >= Self::EDGE_TO_EDGE_THRESHOLD;

                if !is_edge_to_edge && change_ratio < Self::CONSTANT_DIST_RATIO {
                    let ratio_factor = 1.0 - (change_ratio / Self::CONSTANT_DIST_RATIO);
                    let dist_factor = 1.0
                        - ((max_d - 80.0) / (Self::EDGE_TO_EDGE_THRESHOLD - 80.0))
                            .clamp(0.0, 1.0);
                    let severity = ratio_factor * dist_factor;
                    aim_strain *= 1.0 - 0.15 * severity;
                }
            }
        }

        // ════════════════════════════════════════════════════════════
        // BPM-aware exponential scaling system (Relax)
        //
        // Continuous exponential scaling based on BPM and variety metrics
        // Anchor: 410 BPM = 1.0x, grows exponentially above, reduces below
        // Low variety gets less nerf at high BPM, all patterns unified scaling
        // Dampening cap above 520.5 strain limits extreme growth
        // ════════════════════════════════════════════════════════════

        let eff_bpm = 30_000.0 / osu_curr_obj.strain_time;

        // ── Variety measurement ────────────────────────────────────
        let (angle_mean, angle_stddev, angle_n) =
            windowed_angle_stats(osu_curr_obj, diff_objects, Self::ANGLE_WINDOW);

        let angle_var = if angle_n >= 3 {
            (angle_stddev / 1.2).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // Calculate distance variance for relax as well
        let dist_mean = osu_curr_obj.dists.lazy_jump_dist;
        let dist_var = 0.5; // Default neutral for relax; could be calculated if needed

        let combined_variety = (angle_var + dist_var) / 2.0;

        // ── BPM scaling calculation (same as standard aim) ─────────
        let bpm_base_factor = if eff_bpm < 410.0 {
            let low_bpm_t = ((eff_bpm - 250.0) / (410.0 - 250.0)).clamp(0.0, 1.0);
            0.8 + 0.2 * smootherstep(low_bpm_t)
        } else {
            let bpm_ratio = (eff_bpm - 410.0) / 150.0;
            1.0 + bpm_ratio.powf(1.4)
        };

        let variety_mod = if eff_bpm > 410.0 {
            let high_bpm_t = ((eff_bpm - 410.0) / 150.0).clamp(0.0, 1.0);
            1.0 - (combined_variety * 0.15 * smootherstep(high_bpm_t))
        } else {
            1.0
        };

        let bpm_multiplier = bpm_base_factor * variety_mod;
        let mut scaled_strain = aim_strain * bpm_multiplier;

        // ── Cap dampening above 520.5 strain ───────────────────────
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
        // Normalize angle to [0, 1] range within meaningful bounds (PI/6 to 5PI/6)
        let normalized = ((angle.max(PI / 6.0).min(5.0 * PI / 6.0)) - PI / 6.0) / (5.0 * PI / 6.0 - PI / 6.0);
        // Use quintic polynomial for smoother, more accurate curve
        let curve = quintic_ease(normalized);
        curve * curve
    }

    fn calc_acute_angle_bonus(angle: f64) -> f64 {
        1.0 - Self::calc_wide_angle_bonus(angle)
    }
}
