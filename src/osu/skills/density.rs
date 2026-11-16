use crate::osu::difficulty_object::OsuDifficultyObject;

use super::previous;

pub(crate) struct DensitySkill;

impl DensitySkill {
    const WINDOW: usize = 6;
    const DISTANCE_CAP: f64 = 100.0;
    const SPEED_BPM_START: f64 = 180.0;
    const SPEED_BPM_FULL: f64 = 360.0;
    const LINEAR_ANGLE_TOLERANCE: f64 = 25.0_f64.to_radians();
    const FLOW_MIN_ANGLE: f64 = 0.5;
    const FLOW_MAX_ANGLE: f64 = 2.7;
    const FLOW_MAX_STDDEV: f64 = 0.35;
    const VANILLA_LINEAR_MAX_NERF: f64 = 0.85;
    const RELAX_LINEAR_MAX_NERF: f64 = 0.45;

    pub(crate) fn multiplier(
        curr: &OsuDifficultyObject<'_>,
        diff_objects: &[OsuDifficultyObject<'_>],
        has_relax: bool,
    ) -> f64 {
        let curr_dist = curr.dists.lazy_jump_dist;

        if curr.base.is_spinner() || curr_dist >= Self::DISTANCE_CAP {
            return 1.0;
        }

        let mut total_dist = curr_dist;
        let mut count = 1;
        let mut angles = Vec::with_capacity(Self::WINDOW);

        if let Some(angle) = curr.dists.angle {
            angles.push(angle);
        }

        for back in 0..Self::WINDOW - 1 {
            let Some(prev) = previous(diff_objects, curr.idx, back) else {
                break;
            };

            if !prev.base.is_spinner() {
                total_dist += prev.dists.lazy_jump_dist.min(Self::DISTANCE_CAP);
                count += 1;

                if let Some(angle) = prev.dists.angle {
                    angles.push(angle);
                }
            }
        }

        if count < 4 || angles.len() < 4 {
            return 1.0;
        }

        let mean_dist = total_dist / count as f64;
        let bpm = 30_000.0 / curr.strain_time;
        Self::multiplier_for(curr_dist, mean_dist, bpm, &angles, has_relax)
    }

    fn multiplier_for(
        curr_dist: f64,
        mean_dist: f64,
        bpm: f64,
        angles: &[f64],
        has_relax: bool,
    ) -> f64 {
        if curr_dist >= Self::DISTANCE_CAP {
            return 1.0;
        }

        let mean_angle = angles.iter().sum::<f64>() / angles.len() as f64;
        let angle_variance = angles
            .iter()
            .map(|angle| (angle - mean_angle).powi(2))
            .sum::<f64>()
            / angles.len() as f64;
        // Stable turns are flow; angles near 0 or PI instead indicate a straight cursor path.
        let is_flow = mean_angle >= Self::FLOW_MIN_ANGLE
            && mean_angle <= Self::FLOW_MAX_ANGLE
            && angle_variance.sqrt() <= Self::FLOW_MAX_STDDEV;

        if is_flow {
            return 1.0;
        }

        let straightness = angles
            .iter()
            .map(|angle| {
                let deviation_from_line = angle.min(std::f64::consts::PI - angle);
                (1.0 - deviation_from_line / Self::LINEAR_ANGLE_TOLERANCE).clamp(0.0, 1.0)
            })
            .sum::<f64>()
            / angles.len() as f64;

        let density = (1.0 - mean_dist / Self::DISTANCE_CAP).clamp(0.0, 1.0);
        let speed = Self::smoothstep(bpm, Self::SPEED_BPM_START, Self::SPEED_BPM_FULL);
        let max_linear_nerf = if has_relax {
            Self::RELAX_LINEAR_MAX_NERF
        } else {
            Self::VANILLA_LINEAR_MAX_NERF
        };

        1.0 - max_linear_nerf * density * speed * straightness
    }

    fn smoothstep(value: f64, start: f64, end: f64) -> f64 {
        let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

#[cfg(test)]
mod tests {
    use super::DensitySkill;

    #[test]
    fn only_fast_dense_straight_patterns_are_nerfed() {
        let line_angles = [0.0, std::f64::consts::PI, 0.0, std::f64::consts::PI];
        let flow_angles = [1.4, 1.45, 1.5, 1.45];
        let long_jump = DensitySkill::multiplier_for(100.0, 10.0, 360.0, &line_angles, false);
        let slow_line = DensitySkill::multiplier_for(10.0, 10.0, 180.0, &line_angles, false);
        let fast_vanilla = DensitySkill::multiplier_for(10.0, 10.0, 360.0, &line_angles, false);
        let fast_relax = DensitySkill::multiplier_for(10.0, 10.0, 360.0, &line_angles, true);
        let flow = DensitySkill::multiplier_for(10.0, 10.0, 360.0, &flow_angles, false);

        assert_eq!(long_jump, 1.0);
        assert_eq!(slow_line, 1.0);
        assert_eq!(flow, 1.0);
        assert!(fast_vanilla < fast_relax);
        assert!(fast_relax < 1.0);
    }
}
