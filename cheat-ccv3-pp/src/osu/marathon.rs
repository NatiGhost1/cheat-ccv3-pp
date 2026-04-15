// Relax marathon decay for cheat-ccv3-pp.
//
// Bins aim+speed strain peaks (400 ms sections) into per-minute local SR,
// then applies a decay when consecutive minutes have similar SR.

use crate::osu::PERFORMANCE_BASE_MULTIPLIER;

#[derive(Clone, Copy)]
pub(crate) struct MarathonDecayParams {
    pub tau: f64,
    pub b: f64,
    pub q: f64,
    pub double_at: u32,
}

impl Default for MarathonDecayParams {
    fn default() -> Self {
        Self {
            tau: 0.50,
            b: 0.02,
            q: 1.35,
            double_at: 5,
        }
    }
}

fn decay_divisor(r: u32, p: &MarathonDecayParams) -> f64 {
    let rf = r as f64;
    let base = 1.0 + p.b * rf.powf(p.q);
    if r >= p.double_at {
        2.0 * base
    } else {
        base
    }
}

const DIFFICULTY_MULTIPLIER: f64 = 0.0675;
const PEAK_SECTION_LEN_MS: f64 = 400.0;
const MINUTE_MS: f64 = 60_000.0;

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Replicates OsuStrainSkill::difficulty_value on a raw peak slice.
fn difficulty_value_from_peaks(peaks: &[f64]) -> f64 {
    let mut v: Vec<f64> = peaks.iter().copied().filter(|x| *x > 0.0).collect();
    if v.is_empty() {
        return 0.0;
    }

    v.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let reduced_section_count = 10usize;
    let reduced_baseline = 0.75;
    let decay_weight = 0.9;

    let take = reduced_section_count.min(v.len());
    for i in 0..take {
        let clamped = (i as f64 / reduced_section_count as f64).clamp(0.0, 1.0);
        let scale = lerp(1.0, 10.0, clamped).log10();
        v[i] *= lerp(reduced_baseline, 1.0, scale);
    }

    v.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let mut difficulty = 0.0;
    let mut w = 1.0;
    for s in v {
        difficulty += s * w;
        w *= decay_weight;
    }
    difficulty
}

/// difficulty_to_performance matching the vanilla formula.
fn difficulty_to_performance(difficulty: f64) -> f64 {
    (5.0 * (difficulty / 0.0675).max(1.0) - 4.0).powi(3) / 100_000.0
}

fn star_from_aim_speed(aim_peaks: &[f64], speed_peaks: &[f64]) -> f64 {
    let aim_dv = difficulty_value_from_peaks(aim_peaks);
    let speed_dv = difficulty_value_from_peaks(speed_peaks);

    let aim_rating = aim_dv.sqrt() * DIFFICULTY_MULTIPLIER;
    let speed_rating = speed_dv.sqrt() * DIFFICULTY_MULTIPLIER;

    let base_aim_perf = difficulty_to_performance(aim_rating);
    let base_speed_perf = difficulty_to_performance(speed_rating);

    let base_perf = (base_aim_perf.powf(1.1) + base_speed_perf.powf(1.1)).powf(1.0 / 1.1);

    if base_perf <= 0.00001 {
        return 0.0;
    }

    PERFORMANCE_BASE_MULTIPLIER.cbrt()
        * 0.027
        * ((100_000.0 / 2.0_f64.powf(1.0 / 1.1) * base_perf).cbrt() + 4.0)
}

/// Bin raw 400 ms strain peaks into per-minute local star ratings.
pub(crate) fn local_sr_per_minute(strains_aim: &[f64], strains_speed: &[f64]) -> Vec<f64> {
    let peaks_per_min = (MINUTE_MS / PEAK_SECTION_LEN_MS).round() as usize; // 150
    let len = strains_aim.len().min(strains_speed.len());
    if len == 0 {
        return Vec::new();
    }
    let n_minutes = (len + peaks_per_min - 1) / peaks_per_min;

    let mut out = Vec::with_capacity(n_minutes);
    for k in 0..n_minutes {
        let start = k * peaks_per_min;
        let end = ((k + 1) * peaks_per_min).min(len);
        let aim_slice = &strains_aim[start..end];
        let speed_slice = &strains_speed[start..end];
        out.push(star_from_aim_speed(aim_slice, speed_slice));
    }
    out
}

/// Compute the relax marathon multiplier from per-minute local SR values.
/// Returns a value in [0, 1].
pub(crate) fn relax_marathon_multiplier(
    local_sr: &[f64],
    params: &MarathonDecayParams,
) -> f64 {
    if local_sr.len() < 2 {
        return 1.0;
    }

    let mut r: u32 = 0;
    let mut weighted = 0.0;
    let mut total = 0.0;

    for (k, &sr) in local_sr.iter().enumerate() {
        if k > 0 && (sr - local_sr[k - 1]).abs() <= params.tau {
            r += 1;
        } else {
            r = 0;
        }

        let lambda = 1.0 / decay_divisor(r, params);
        weighted += sr * lambda;
        total += sr;
    }

    if total > 0.0 {
        (weighted / total).clamp(0.0, 1.0)
    } else {
        1.0
    }
}
