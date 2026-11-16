// Relax-specific aim evaluator for cheat-ccv3-pp.
//
// Port of aim_rx.rs from old-ccv3-pp. Adapted for this crate's
// API: fields on curr.dists.*, previous() as a free function, no FloatExt.

use std::f64::consts::PI;

use crate::osu::difficulty_object::OsuDifficultyObject;

use super::previous;

#[inline]
fn reverse_lerp(value: f64, start: f64, end: f64) -> f64 {
    ((value - start) / (end - start)).clamp(0.0, 1.0)
}

#[inline]
fn smoothstep(value: f64, start: f64, end: f64) -> f64 {
    let t = reverse_lerp(value, start, end);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn smootherstep(value: f64, start: f64, end: f64) -> f64 {
    let t = reverse_lerp(value, start, end);
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
    const WIDE_ANGLE_MULTIPLIER: f64 = 1.56;
    const ACUTE_ANGLE_MULTIPLIER: f64 = 2.66;
    const SLIDER_MULTIPLIER: f64 = 1.03;
    const VELOCITY_CHANGE_MULTIPLIER: f64 = 0.81;
    const WIGGLE_MULTIPLIER: f64 = 1.02;
    const AIM_CALIBRATION: f64 = 0.92;

    const SLOW_SLIDER_VEL_FLOOR: f64 = 0.55;
    const ANGLE_WINDOW: usize = 8;
    const NORMALIZED_RADIUS: f64 = 50.0;
    const NORMALIZED_DIAMETER: f64 = Self::NORMALIZED_RADIUS * 2.0;

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
        let mut wiggle_bonus = 0.0;
        let mut slider_bonus = 0.0;
        let mut vel_change_bonus = 0.0;
        let mut aim_strain = curr_vel;

        if let Some((curr_angle, last_angle)) =
            osu_curr_obj.dists.angle.zip(osu_last_obj.dists.angle)
        {
            let angle_bonus = curr_vel.min(prev_vel);

            if osu_curr_obj.strain_time.max(osu_last_obj.strain_time)
                < 1.25 * osu_curr_obj.strain_time.min(osu_last_obj.strain_time)
            {
                acute_angle_bonus = Self::calc_acute_angle_bonus(curr_angle);
                acute_angle_bonus *= 0.08
                    + 0.92
                        * (1.0
                            - acute_angle_bonus
                                .min(Self::calc_acute_angle_bonus(last_angle).powi(3)));
                acute_angle_bonus *= angle_bonus
                    * smootherstep(30_000.0 / osu_curr_obj.strain_time, 300.0, 400.0)
                    * smootherstep(
                        osu_curr_obj.dists.lazy_jump_dist,
                        Self::NORMALIZED_DIAMETER,
                        Self::NORMALIZED_DIAMETER * 2.0,
                    );
            }

            wide_angle_bonus = Self::calc_wide_angle_bonus(curr_angle);
            let (_, win_stddev, win_n) =
                windowed_angle_stats(osu_curr_obj, diff_objects, Self::ANGLE_WINDOW);
            let variance_factor = if win_n >= 3 {
                (win_stddev / 1.2).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let wide_rep_nerf = (1.0 - variance_factor) * 0.25;
            wide_angle_bonus *= angle_bonus
                * smootherstep(
                    osu_curr_obj.dists.lazy_jump_dist,
                    0.0,
                    Self::NORMALIZED_DIAMETER,
                )
                * (1.0 - wide_rep_nerf).max(0.0);

            let curr_dist = osu_curr_obj.dists.lazy_jump_dist;
            let last_dist = osu_last_obj.dists.lazy_jump_dist;
            let wiggle_for = |dist: f64, angle: f64| {
                smootherstep(dist, Self::NORMALIZED_RADIUS, Self::NORMALIZED_DIAMETER)
                    * reverse_lerp(
                        dist,
                        Self::NORMALIZED_DIAMETER * 3.0,
                        Self::NORMALIZED_DIAMETER,
                    )
                    .powf(1.8)
                    * smootherstep(angle, 110.0 * PI / 180.0, 60.0 * PI / 180.0)
            };
            wiggle_bonus =
                angle_bonus * wiggle_for(curr_dist, curr_angle) * wiggle_for(last_dist, last_angle);
        }

        if prev_vel.max(curr_vel) > f64::EPSILON {
            prev_vel = (osu_last_obj.dists.lazy_jump_dist + osu_last_last_obj.dists.travel_dist)
                / osu_last_obj.strain_time;
            curr_vel = (osu_curr_obj.dists.lazy_jump_dist + osu_last_obj.dists.travel_dist)
                / osu_curr_obj.strain_time;

            let dist_ratio = smoothstep(
                (prev_vel - curr_vel).abs() / prev_vel.max(curr_vel),
                0.0,
                1.0,
            );
            let overlap_vel_buff = (Self::NORMALIZED_DIAMETER * 1.25
                / osu_curr_obj.strain_time.min(osu_last_obj.strain_time))
            .min((prev_vel - curr_vel).abs());
            vel_change_bonus = overlap_vel_buff * dist_ratio;

            let bonus_base = osu_curr_obj.strain_time.min(osu_last_obj.strain_time)
                / osu_curr_obj.strain_time.max(osu_last_obj.strain_time);
            vel_change_bonus *= bonus_base.powi(2);
        }

        if osu_last_obj.base.is_slider()
            && osu_last_obj.dists.travel_dist > Self::NORMALIZED_RADIUS * 1.2
        {
            let travel_vel = osu_last_obj.dists.travel_dist / osu_last_obj.dists.travel_time;
            slider_bonus = travel_vel;
            if travel_vel < Self::SLOW_SLIDER_VEL_FLOOR {
                let ratio = (travel_vel / Self::SLOW_SLIDER_VEL_FLOOR).clamp(0.0, 1.0);
                slider_bonus *= 0.55 + 0.45 * ratio;
            }
        }

        aim_strain += wiggle_bonus * Self::WIGGLE_MULTIPLIER;
        aim_strain += vel_change_bonus * Self::VELOCITY_CHANGE_MULTIPLIER;
        aim_strain += (acute_angle_bonus * Self::ACUTE_ANGLE_MULTIPLIER)
            .max(wide_angle_bonus * Self::WIDE_ANGLE_MULTIPLIER);

        if with_sliders {
            aim_strain += slider_bonus * Self::SLIDER_MULTIPLIER;
        }

        aim_strain * Self::AIM_CALIBRATION
    }

    fn calc_wide_angle_bonus(angle: f64) -> f64 {
        smoothstep(angle, 40.0 * PI / 180.0, 140.0 * PI / 180.0)
    }

    fn calc_acute_angle_bonus(angle: f64) -> f64 {
        smoothstep(angle, 140.0 * PI / 180.0, 40.0 * PI / 180.0)
    }
}
